//! Runs the SSH connection on a background thread. The UI sends [`Command`]s and drains [`Event`]s.

use crate::protocol::{Request, SUBSYSTEM, ServerEvent};
use crate::trust::{self, TrustStatus};
use base64::Engine;
use russh::client;
use russh::keys::{PrivateKey, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::ChannelMsg;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};

const MAX_LINE: usize = 16 << 20;

#[derive(Clone)]
pub struct ConnectRequest {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub key: Arc<PrivateKey>,
    pub known_hosts: PathBuf,
    /// Also accept keys already trusted in the user's `~/.ssh/known_hosts`.
    pub use_openssh_known_hosts: bool,
}

pub enum Command {
    Connect(ConnectRequest),
    Disconnect,
    Send(String),
    Join(String),
    Upload { req: u64, data: Vec<u8>, caption: String, hq: bool, to: Option<String> },
    Fetch { req: u64, reference: String },
    TrustDecision { prompt: u64, accept: bool },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    Idle,
    Connecting { attempt: u32 },
    Authenticating,
    Joining,
    Online,
    /// The connection dropped; another attempt follows after `retry_in`.
    Retrying { attempt: u32, retry_in: Duration, reason: String },
    Offline { reason: String },
}

#[derive(Debug, Clone)]
pub struct HostKeyPrompt {
    pub id: u64,
    pub host: String,
    pub port: u16,
    pub fingerprint: String,
    pub algorithm: String,
    pub randomart: String,
    /// Set when a different key was on record, with its fingerprint when known.
    pub changed: Option<Option<String>>,
}

pub enum Event {
    Status(Status),
    HostKey(HostKeyPrompt),
    /// The server key was already trusted through OpenSSH's known_hosts.
    TrustedViaOpenSsh,
    Server(ServerEvent),
    Blob { req: u64, reference: String, result: Result<(String, Vec<u8>), String> },
    Latency(Duration),
}

pub struct ClientHandle {
    commands: mpsc::UnboundedSender<Command>,
    events: std::sync::mpsc::Receiver<Event>,
}

impl ClientHandle {
    /// Starts the network thread. `wake` is called after each event so the UI can repaint.
    pub fn spawn(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::unbounded_channel();
        let (evt_tx, evt_rx) = std::sync::mpsc::channel();
        let emitter = Emitter { tx: evt_tx, wake: Arc::new(wake) };
        std::thread::Builder::new()
            .name("devzat-net".into())
            .spawn(move || {
                let rt = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()
                    .expect("tokio runtime");
                rt.block_on(dispatch(cmd_rx, emitter));
            })
            .expect("network thread");
        ClientHandle { commands: cmd_tx, events: evt_rx }
    }

    pub fn send(&self, cmd: Command) {
        let _ = self.commands.send(cmd);
    }

    pub fn poll(&self) -> Vec<Event> {
        self.events.try_iter().collect()
    }
}

#[derive(Clone)]
struct Emitter {
    tx: std::sync::mpsc::Sender<Event>,
    wake: Arc<dyn Fn() + Send + Sync>,
}

impl Emitter {
    fn emit(&self, e: Event) {
        let _ = self.tx.send(e);
        (self.wake)();
    }
    fn status(&self, s: Status) {
        self.emit(Event::Status(s));
    }
}

type Prompts = Arc<Mutex<HashMap<u64, oneshot::Sender<bool>>>>;

struct Session {
    out: mpsc::UnboundedSender<Vec<u8>>,
    task: tokio::task::JoinHandle<()>,
}

async fn dispatch(mut commands: mpsc::UnboundedReceiver<Command>, emitter: Emitter) {
    let prompts: Prompts = Arc::default();
    let mut session: Option<Session> = None;
    while let Some(cmd) = commands.recv().await {
        let request = match cmd {
            Command::Connect(req) => {
                if let Some(old) = session.take() {
                    old.task.abort();
                }
                let (out_tx, out_rx) = mpsc::unbounded_channel();
                let task = tokio::spawn(connection_loop(req, out_rx, emitter.clone(), prompts.clone()));
                session = Some(Session { out: out_tx, task });
                continue;
            }
            Command::Disconnect => {
                if let Some(old) = session.take() {
                    old.task.abort();
                }
                emitter.status(Status::Offline { reason: "Disconnected".into() });
                continue;
            }
            Command::TrustDecision { prompt, accept } => {
                if let Some(tx) = prompts.lock().expect("prompts").remove(&prompt) {
                    let _ = tx.send(accept);
                }
                continue;
            }
            Command::Send(text) => Request::Send { text },
            Command::Join(room) => Request::Join { room },
            Command::Upload { req, data, caption, hq, to } => Request::Upload {
                req,
                data: base64::engine::general_purpose::STANDARD.encode(data),
                caption,
                hq,
                to,
            },
            Command::Fetch { req, reference } => Request::Fetch { req, reference },
        };
        if let Some(s) = &session {
            let _ = s.out.send(request.to_line());
        }
    }
}

enum Outcome {
    /// The server said goodbye (exit, kick, ban). Don't reconnect.
    Bye(String),
    /// Retrying won't help (rejected host key, refused login, old server).
    Fatal(String),
    /// Network trouble. Try again.
    Dropped { reason: String, was_online: bool },
}

async fn connection_loop(
    req: ConnectRequest,
    mut out: mpsc::UnboundedReceiver<Vec<u8>>,
    emitter: Emitter,
    prompts: Prompts,
) {
    let mut attempt = 0u32;
    loop {
        attempt += 1;
        emitter.status(Status::Connecting { attempt });
        match run_session(&req, &mut out, &emitter, &prompts).await {
            Outcome::Bye(reason) => {
                emitter.status(Status::Offline { reason });
                return;
            }
            Outcome::Fatal(reason) => {
                emitter.status(Status::Offline { reason });
                return;
            }
            Outcome::Dropped { reason, was_online } => {
                if was_online {
                    attempt = 1;
                }
                let retry_in = Duration::from_secs([1, 2, 4, 8, 15, 30][(attempt as usize - 1).min(5)]);
                emitter.status(Status::Retrying { attempt, retry_in, reason });
                tokio::time::sleep(retry_in).await;
            }
        }
    }
}

struct Verifier {
    host: String,
    port: u16,
    store: PathBuf,
    use_openssh: bool,
    emitter: Emitter,
    prompts: Prompts,
    rejected: Arc<AtomicBool>,
}

static PROMPT_IDS: AtomicU64 = AtomicU64::new(1);

impl client::Handler for Verifier {
    type Error = russh::Error;

    async fn check_server_key(&mut self, key: &PublicKeyOrCertificate) -> Result<bool, Self::Error> {
        let key = key.public_key();
        let status = trust::check(&self.host, self.port, &key, &self.store, self.use_openssh);
        let changed = match status {
            TrustStatus::Trusted { via_openssh } => {
                if via_openssh {
                    self.emitter.emit(Event::TrustedViaOpenSsh);
                }
                return Ok(true);
            }
            TrustStatus::Unknown => None,
            TrustStatus::Changed { previous } => Some(previous),
        };
        let id = PROMPT_IDS.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        self.prompts.lock().expect("prompts").insert(id, tx);
        self.emitter.emit(Event::HostKey(HostKeyPrompt {
            id,
            host: self.host.clone(),
            port: self.port,
            fingerprint: trust::fingerprint(&key),
            algorithm: key.algorithm().as_str().to_string(),
            randomart: trust::randomart(&key),
            changed,
        }));
        if rx.await.unwrap_or(false) {
            if let Err(e) = trust::remember(&self.host, self.port, &key, &self.store) {
                log::warn!("could not save host key: {e}");
            }
            Ok(true)
        } else {
            self.rejected.store(true, Ordering::Relaxed);
            Ok(false)
        }
    }
}

async fn run_session(
    req: &ConnectRequest,
    out: &mut mpsc::UnboundedReceiver<Vec<u8>>,
    emitter: &Emitter,
    prompts: &Prompts,
) -> Outcome {
    let dropped = |reason: String| Outcome::Dropped { reason, was_online: false };
    let rejected = Arc::new(AtomicBool::new(false));
    let verifier = Verifier {
        host: req.host.clone(),
        port: req.port,
        store: req.known_hosts.clone(),
        use_openssh: req.use_openssh_known_hosts,
        emitter: emitter.clone(),
        prompts: prompts.clone(),
        rejected: rejected.clone(),
    };
    let config = Arc::new(client::Config {
        keepalive_interval: Some(Duration::from_secs(15)),
        keepalive_max: 3,
        inactivity_timeout: None,
        nodelay: true,
        ..Default::default()
    });

    // The host key prompt waits on a person, so only the TCP connect gets a deadline.
    let addr = format!("{}:{}", req.host, req.port);
    let stream = match tokio::time::timeout(Duration::from_secs(12), tokio::net::TcpStream::connect(&addr)).await {
        Err(_) => return dropped(format!("Timed out reaching {addr}")),
        Ok(Err(e)) => return dropped(friendly_io(&e, &addr)),
        Ok(Ok(s)) => s,
    };
    let _ = stream.set_nodelay(true);
    let mut handle = match client::connect_stream(config, stream, verifier).await {
        Ok(h) => h,
        Err(e) => {
            if rejected.load(Ordering::Relaxed) {
                return Outcome::Fatal("You didn't trust the server's key, so nothing was sent.".into());
            }
            return dropped(format!("SSH handshake failed: {e}"));
        }
    };

    emitter.status(Status::Authenticating);
    let hash = if req.key.algorithm().is_rsa() {
        handle.best_supported_rsa_hash().await.ok().flatten().flatten()
    } else {
        None
    };
    match handle
        .authenticate_publickey(req.username.clone(), PrivateKeyWithHashAlg::new(req.key.clone(), hash))
        .await
    {
        Ok(r) if r.success() => {}
        Ok(_) => return Outcome::Fatal("The server refused your key.".into()),
        Err(e) => return dropped(format!("Login failed: {e}")),
    }

    emitter.status(Status::Joining);
    let channel = match handle.channel_open_session().await {
        Ok(c) => c,
        Err(e) => return dropped(format!("Could not open a session: {e}")),
    };
    if let Err(e) = channel.request_subsystem(true, SUBSYSTEM).await {
        return dropped(format!("Could not start chat: {e}"));
    }
    let (mut rx, tx) = channel.split();

    let mut buf: Vec<u8> = Vec::new();
    let mut online = false;
    let mut bye: Option<String> = None;
    // What the server said before letting us in, such as a ban or allowlist notice.
    let mut gatekeeper: Vec<String> = Vec::new();
    let mut pings: HashMap<u64, Instant> = HashMap::new();
    let mut next_ping = 1u64 << 48;
    let mut ticker = tokio::time::interval(Duration::from_secs(10));
    ticker.tick().await;

    loop {
        tokio::select! {
            msg = rx.wait() => match msg {
                Some(ChannelMsg::Data { data }) => {
                    buf.extend_from_slice(&data);
                    while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
                        let line: Vec<u8> = buf.drain(..=pos).collect();
                        let line = &line[..line.len() - 1];
                        if line.is_empty() {
                            continue;
                        }
                        match serde_json::from_slice::<ServerEvent>(line) {
                            Ok(ServerEvent::Pong(ack)) => {
                                if let Some(sent) = pings.remove(&ack.req) {
                                    emitter.emit(Event::Latency(sent.elapsed()));
                                }
                            }
                            Ok(ServerEvent::Blob(b)) => {
                                let result = match (b.error, b.data) {
                                    (Some(e), _) => Err(e),
                                    (None, Some(d)) => base64::engine::general_purpose::STANDARD
                                        .decode(d.as_bytes())
                                        .map(|bytes| (b.mime, bytes))
                                        .map_err(|e| e.to_string()),
                                    (None, None) => Err("empty image".into()),
                                };
                                emitter.emit(Event::Blob { req: b.req, reference: b.reference, result });
                            }
                            Ok(ServerEvent::Bye { reason }) => {
                                bye = Some(reason);
                            }
                            Ok(ev) => {
                                if !online && matches!(ev, ServerEvent::Welcome(_)) {
                                    online = true;
                                    emitter.status(Status::Online);
                                }
                                if !online
                                    && let ServerEvent::Msg(m) = &ev {
                                        gatekeeper.push(crate::ansi::strip(&m.text).replace(['*', '_'], ""));
                                    }
                                emitter.emit(Event::Server(ev));
                            }
                            Err(e) => log::debug!("ignoring server line: {e}"),
                        }
                    }
                    if buf.len() > MAX_LINE {
                        return dropped("The server sent a message that was too large.".into());
                    }
                }
                Some(ChannelMsg::Failure) if !online => {
                    return Outcome::Fatal("This server doesn't support the desktop app yet. It needs a devzat build with the devzat-json subsystem.".into());
                }
                Some(ChannelMsg::Eof) | Some(ChannelMsg::Close) | None => {
                    return match bye {
                        Some(reason) => Outcome::Bye(if reason.is_empty() { "The server closed the session.".into() } else { reason }),
                        None if !online && !gatekeeper.is_empty() => Outcome::Fatal(gatekeeper.join(" ")),
                        None => Outcome::Dropped { reason: "Connection lost".into(), was_online: online },
                    };
                }
                _ => {}
            },
            line = out.recv() => match line {
                Some(bytes) => {
                    if let Err(e) = tx.data_bytes(bytes).await {
                        return Outcome::Dropped { reason: format!("Send failed: {e}"), was_online: online };
                    }
                }
                None => return Outcome::Fatal("Closed".into()),
            },
            _ = ticker.tick() => {
                next_ping += 1;
                pings.insert(next_ping, Instant::now());
                pings.retain(|_, t| t.elapsed() < Duration::from_secs(60));
                if tx.data_bytes(Request::Ping { req: next_ping }.to_line()).await.is_err() {
                    return Outcome::Dropped { reason: "Connection lost".into(), was_online: online };
                }
            }
        }
    }
}

fn friendly_io(e: &std::io::Error, addr: &str) -> String {
    match e.kind() {
        std::io::ErrorKind::ConnectionRefused => format!("Nothing is listening at {addr}"),
        std::io::ErrorKind::NotFound => format!("Couldn't find {addr}"),
        _ => format!("Couldn't reach {addr}: {e}"),
    }
}
