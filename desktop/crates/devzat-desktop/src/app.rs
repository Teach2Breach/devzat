use crate::fx::{self, Fx};
use crate::images::{self, Images};
use crate::model::{self, ConvKey, Conversation};
use crate::settings::{ServerProfile, Settings};
use crate::theme::{self, Palette, ThemeId};
use devzat_core::protocol::{MessageKind, RoomState, ServerEvent, Welcome};
use devzat_core::{ClientHandle, Command, ConnectRequest, Event, HostKeyPrompt, Identity, Status, paths};
use eframe::egui::{self, Id, LayerId, Order, TextureHandle};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::Duration;

#[derive(PartialEq, Eq, Clone, Copy)]
pub enum Screen {
    Launch,
    Chat,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Info,
    Good,
    Bad,
}

pub struct Toast {
    pub text: String,
    pub kind: ToastKind,
    pub born: f64,
}

pub struct PendingImage {
    pub bytes: Vec<u8>,
    pub preview: Option<TextureHandle>,
    pub size: [usize; 2],
    pub caption: String,
    pub hq: bool,
    pub name: String,
}

pub struct KeyImport {
    pub path: PathBuf,
    pub passphrase: String,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct Composer {
    pub text: String,
    pub hq: bool,
    pub emoji_open: bool,
    pub sent: Vec<String>,
    pub recall: Option<usize>,
    pub popup_index: usize,
    pub want_focus: bool,
}

pub struct DevzatApp {
    pub settings: Settings,
    pub client: ClientHandle,
    pub identity: Result<Identity, String>,
    pub status: Status,
    pub latency: Option<Duration>,
    pub screen: Screen,
    pub form: ServerProfile,
    pub port_text: String,
    pub launch_error: Option<String>,
    pub profile: Option<ServerProfile>,
    pub convs: BTreeMap<ConvKey, Conversation>,
    pub active: ConvKey,
    pub state: RoomState,
    pub welcome: Option<Welcome>,
    pub images: Images,
    pub fx: Fx,
    pub toasts: Vec<Toast>,
    pub hostkey: Option<(HostKeyPrompt, bool)>,
    pub composer: Composer,
    pub pending_image: Option<PendingImage>,
    pub key_import: Option<KeyImport>,
    pub lightbox: Option<String>,
    pub settings_open: bool,
    pub settings_tab: usize,
    pub new_room: Option<String>,
    /// The server asked for a different display name before letting us in: (its message, what we're typing).
    pub name_prompt: Option<(String, String)>,
    pub show_pubkey: bool,
    pub time: f32,
    last_frame: f64,
    applied: Option<(ThemeId, i32)>,
    pub unseen_while_away: u32,
    celebrate_connect: bool,
    pub confetti_queued: usize,
    pub burst_at: Option<egui::Pos2>,
    pub copied: Option<String>,
    pub stick_frames: u8,
    pub row_heights: crate::chat::RowHeights,
    last_title: String,
}

impl DevzatApp {
    pub fn new(cc: &eframe::CreationContext) -> Self {
        theme::install_fonts(&cc.egui_ctx);
        let settings: Settings = cc.storage.and_then(|s| eframe::get_value(s, eframe::APP_KEY)).unwrap_or_default();
        let ctx = cc.egui_ctx.clone();
        let client = ClientHandle::spawn(move || ctx.request_repaint());
        let mut form = settings.last.clone();
        if form.name.is_empty() {
            form.name = default_name();
        }
        let port_text = form.port.to_string();
        let mut app = DevzatApp {
            identity: Err("loading".into()),
            client,
            status: Status::Idle,
            latency: None,
            screen: Screen::Launch,
            form,
            port_text,
            launch_error: None,
            profile: None,
            convs: BTreeMap::new(),
            active: ConvKey::Room("#main".into()),
            state: RoomState::default(),
            welcome: None,
            images: Images::new(cc.egui_ctx.clone()),
            fx: Fx::new(),
            toasts: Vec::new(),
            hostkey: None,
            composer: Composer::default(),
            pending_image: None,
            key_import: None,
            lightbox: None,
            settings_open: false,
            settings_tab: 0,
            new_room: None,
            name_prompt: None,
            show_pubkey: false,
            time: 0.0,
            last_frame: 0.0,
            applied: None,
            unseen_while_away: 0,
            celebrate_connect: false,
            confetti_queued: 0,
            burst_at: None,
            copied: None,
            stick_frames: 8,
            row_heights: Default::default(),
            last_title: String::new(),
            settings,
        };
        app.reload_identity();
        app
    }

    pub fn pal(&self) -> Palette {
        self.settings.theme.palette()
    }

    pub fn reload_identity(&mut self) {
        self.identity = match self.settings.identity_path.clone() {
            Some(path) => match Identity::load(&path, None) {
                Ok(id) => Ok(id),
                Err(devzat_core::identity::IdentityError::NeedsPassphrase) => {
                    self.key_import = Some(KeyImport { path: path.clone(), passphrase: String::new(), error: None });
                    Err(format!("{} needs its passphrase", path.display()))
                }
                Err(e) => Err(format!("{}: {e}", path.display())),
            },
            None => Identity::load_or_create(&paths::identity_file()).map_err(|e| e.to_string()),
        };
    }

    pub fn toast(&mut self, kind: ToastKind, text: impl Into<String>) {
        let text = text.into();
        self.toasts.retain(|t| t.text != text);
        self.toasts.push(Toast { text, kind, born: self.last_frame });
        if self.toasts.len() > 4 {
            self.toasts.remove(0);
        }
    }

    pub fn my_plain(&self) -> String {
        if !self.state.plain.is_empty() {
            return self.state.plain.clone();
        }
        self.welcome.as_ref().map(|w| w.plain.clone()).unwrap_or_default()
    }

    pub fn connect(&mut self) {
        let host = self.form.host.trim().trim_start_matches("ssh://").to_lowercase();
        let name = self.form.name.trim().to_string();
        let port = match self.port_text.trim().parse::<u16>() {
            Ok(p) if p > 0 => p,
            _ => {
                self.launch_error = Some("The port should be a number like 22 or 2221.".into());
                return;
            }
        };
        if host.is_empty() {
            self.launch_error = Some("Enter the server's address.".into());
            return;
        }
        if name.is_empty() || name.contains(char::is_whitespace) {
            self.launch_error = Some("Pick a display name without spaces.".into());
            return;
        }
        let key = match &self.identity {
            Ok(id) => id.key.clone(),
            Err(e) => {
                self.launch_error = Some(format!("Your key isn't ready: {e}"));
                return;
            }
        };
        let profile = ServerProfile { host: host.clone(), port, name: name.clone() };
        self.form = profile.clone();
        self.settings.remember_server(&profile);
        self.profile = Some(profile);
        self.launch_error = None;
        self.convs.clear();
        self.state = RoomState::default();
        self.welcome = None;
        self.active = ConvKey::Room("#main".into());
        self.latency = None;
        self.celebrate_connect = true;
        self.client.send(Command::Connect(ConnectRequest {
            host,
            port,
            username: name,
            key,
            known_hosts: paths::known_hosts_file(),
            use_openssh_known_hosts: self.settings.use_openssh_known_hosts,
        }));
    }

    pub fn retry_now(&mut self) {
        if let Some(p) = self.profile.clone() {
            self.form = p;
            self.port_text = self.form.port.to_string();
            let convs = std::mem::take(&mut self.convs);
            self.connect();
            self.convs = convs;
        }
    }

    pub fn disconnect(&mut self) {
        self.client.send(Command::Disconnect);
        self.screen = Screen::Launch;
        self.profile = None;
    }

    pub fn join_room(&mut self, room: &str) {
        let room = if room.starts_with('#') { room.to_string() } else { format!("#{room}") };
        if room == self.state.room {
            self.active = ConvKey::Room(room);
        } else {
            self.client.send(Command::Join(room.clone()));
            self.active = ConvKey::Room(room.clone());
        }
        self.convs.entry(self.active.clone()).or_default();
        self.clear_unread();
        self.stick_frames = 8;
    }

    pub fn open_dm(&mut self, peer: &str) {
        let peer = peer.trim_start_matches('@').to_string();
        if peer.is_empty() || peer == self.my_plain() {
            return;
        }
        self.active = ConvKey::Dm(peer);
        self.convs.entry(self.active.clone()).or_default();
        self.clear_unread();
        self.composer.want_focus = true;
        self.stick_frames = 8;
    }

    pub fn clear_unread(&mut self) {
        if let Some(c) = self.convs.get_mut(&self.active) {
            c.unread = 0;
            c.mentions = 0;
        }
    }

    /// Sends what the person typed, after handling the few commands the app does itself.
    pub fn submit(&mut self, raw: String) {
        let text = raw.trim().to_string();
        if text.is_empty() {
            return;
        }
        self.composer.sent.push(text.clone());
        if self.composer.sent.len() > 50 {
            self.composer.sent.remove(0);
        }
        self.composer.recall = None;
        if let Some(rest) = text.strip_prefix('/') {
            let (cmd, arg) = rest.split_once(' ').unwrap_or((rest, ""));
            match cmd {
                "confetti" => {
                    self.confetti_queued += 160;
                    return;
                }
                "theme" => {
                    match ThemeId::from_name(arg) {
                        Some(t) => self.settings.theme = t,
                        None => self.toast(ToastKind::Info, "Themes: neon, phosphor, sunset, paper"),
                    }
                    return;
                }
                "crt" => {
                    self.settings.crt = !self.settings.crt;
                    return;
                }
                _ => {}
            }
        }
        let mut line = match text.strip_prefix('/') {
            Some(rest) if crate::composer::is_server_command(rest.split(' ').next().unwrap_or("")) => rest.to_string(),
            _ => text,
        };
        match self.active.clone() {
            ConvKey::Dm(peer) => line = format!("={peer} {line}"),
            ConvKey::Room(_) => {
                if let Some(arg) = line.strip_prefix("cd ").map(str::trim) {
                    if arg == ".." {
                        self.join_room("#main");
                        return;
                    }
                    if arg.starts_with('#') && !arg.contains(' ') {
                        self.join_room(arg);
                        return;
                    }
                    if !arg.is_empty() && !arg.contains(' ') {
                        self.open_dm(arg);
                        return;
                    }
                }
            }
        }
        self.stick_frames = 8;
        self.client.send(Command::Send(line));
    }

    pub fn send_pending_image(&mut self) {
        let Some(img) = self.pending_image.take() else { return };
        let bytes = shrink_for_upload(img.bytes);
        let to = match &self.active {
            ConvKey::Dm(p) => Some(p.clone()),
            ConvKey::Room(_) => None,
        };
        let req = (self.time * 1000.0) as u64 + 1;
        self.client.send(Command::Upload { req, data: bytes, caption: img.caption, hq: img.hq, to });
        self.toast(ToastKind::Info, "Sending picture…");
        self.stick_frames = 8;
    }

    pub fn stage_image(&mut self, ctx: &egui::Context, bytes: Vec<u8>, name: String) {
        if image::guess_format(&bytes).is_err() {
            self.toast(ToastKind::Bad, format!("{name} isn't a picture devzat can show"));
            return;
        }
        let (preview, size) = match images::preview(&bytes) {
            Some(img) => {
                let size = img.size;
                (Some(ctx.load_texture("send-preview", img, egui::TextureOptions::LINEAR)), size)
            }
            None => (None, [0, 0]),
        };
        self.pending_image = Some(PendingImage { bytes, preview, size, caption: String::new(), hq: self.composer.hq, name });
    }

    fn handle(&mut self, ctx: &egui::Context, ev: Event) {
        match ev {
            Event::Status(s) => {
                match &s {
                    Status::Online => {
                        self.screen = Screen::Chat;
                        self.images.retry_unfinished();
                        if std::mem::take(&mut self.celebrate_connect) && self.settings.effects {
                            self.confetti_queued += 140;
                        }
                    }
                    Status::Offline { reason } => {
                        if reason.ends_with("has left the chat") {
                            self.launch_error = None;
                            let place = self.profile.as_ref().map(|p| p.label()).unwrap_or_else(|| "the server".into());
                            self.toast(ToastKind::Info, format!("You left {place}"));
                        } else if reason != "Disconnected" {
                            self.launch_error = Some(reason.clone());
                        }
                        self.screen = Screen::Launch;
                        self.hostkey = None;
                        self.name_prompt = None;
                    }
                    _ => {}
                }
                self.status = s;
            }
            Event::HostKey(p) => self.hostkey = Some((p, false)),
            Event::TrustedViaOpenSsh => self.toast(ToastKind::Info, "Server key matched your OpenSSH known_hosts"),
            Event::Server(se) => self.on_server(ctx, se),
            Event::Blob { req, reference, result } => self.images.on_blob(req, reference, result),
            Event::Latency(d) => self.latency = Some(d),
        }
    }

    fn on_server(&mut self, ctx: &egui::Context, ev: ServerEvent) {
        match ev {
            ServerEvent::Welcome(w) => {
                self.toast(ToastKind::Good, format!("Welcome, {}", w.plain));
                self.welcome = Some(w);
            }
            ServerEvent::Msg(m) if self.welcome.is_none() && m.text.contains("Pick a different one") => {
                let text = devzat_core::ansi::strip(&m.text);
                self.name_prompt = Some((text, String::new()));
            }
            ServerEvent::Msg(m) => {
                let key = model::route(&m);
                let me = self.my_plain();
                let msg = model::to_ui(&m, &me, &self.state.name);
                let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
                let is_active = key == self.active && focused;
                let countable = !m.history && msg.kind != MessageKind::System && !msg.mine;
                let alert = msg.mention || msg.kind == MessageKind::DmIn;
                let party = !m.history && (msg.plain.contains('🎉') || msg.plain.contains('🥳'));
                let follow = key == self.active && (m.history || msg.mine);
                let conv = self.convs.entry(key.clone()).or_default();
                if conv.insert(msg) {
                    if follow {
                        self.stick_frames = 8;
                    }
                    if countable && !is_active {
                        conv.unread += 1;
                        if alert {
                            conv.mentions += 1;
                        }
                    }
                    if countable && !focused {
                        self.unseen_while_away += 1;
                        if alert {
                            ctx.send_viewport_cmd(egui::ViewportCommand::RequestUserAttention(egui::UserAttentionType::Informational));
                        }
                    }
                    if party && self.settings.effects {
                        self.confetti_queued += 120;
                    }
                }
            }
            ServerEvent::State(s) => {
                let moved = s.room != self.state.room;
                self.state = s;
                if moved {
                    let key = ConvKey::Room(self.state.room.clone());
                    self.convs.entry(key.clone()).or_default();
                    if matches!(self.active, ConvKey::Room(_)) {
                        self.active = key;
                        self.stick_frames = 8;
                    }
                }
            }
            ServerEvent::Clear => {
                if let Some(c) = self.convs.get_mut(&self.active) {
                    c.messages.clear();
                }
            }
            ServerEvent::Ack(a) => {
                if a.ok && a.reference.is_some() {
                    self.toast(ToastKind::Good, "Picture sent");
                } else if let Some(e) = a.error {
                    self.toast(ToastKind::Bad, e);
                }
            }
            _ => {}
        }
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        let zoom = (self.settings.zoom * 100.0).round() as i32;
        if self.applied != Some((self.settings.theme, zoom)) {
            theme::apply(ctx, &self.pal());
            ctx.set_zoom_factor(self.settings.zoom.clamp(0.7, 2.0));
            self.applied = Some((self.settings.theme, zoom));
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let focused = ctx.input(|i| i.viewport().focused.unwrap_or(true));
        if focused {
            self.unseen_while_away = 0;
        }
        let place = match (self.screen, &self.active) {
            (Screen::Launch, _) => String::new(),
            (Screen::Chat, k) => format!(" — {}", k.title()),
        };
        let count = if self.unseen_while_away > 0 { format!("({}) ", self.unseen_while_away) } else { String::new() };
        let title = format!("{count}Devzat{place}");
        if title != self.last_title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.last_title = title;
        }
    }

    fn take_input_files(&mut self, ctx: &egui::Context) {
        if self.screen != Screen::Chat {
            return;
        }
        let (dropped, pasted) = ctx.input(|i| {
            let pasted: Vec<_> = i
                .events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::PasteImage(img) => Some(img.clone()),
                    _ => None,
                })
                .collect();
            (i.raw.dropped_files.clone(), pasted)
        });
        for f in dropped {
            let name = f.path().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "file".into());
            match f.bytes() {
                Ok(bytes) => self.stage_image(ctx, bytes, name),
                Err(e) => self.toast(ToastKind::Bad, format!("Couldn't read {name}: {e}")),
            }
        }
        for img in pasted {
            if let Some(png) = images::png_from_color_image(&img) {
                self.stage_image(ctx, png, "pasted picture".into());
            }
        }
    }
}

impl eframe::App for DevzatApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        for ev in self.client.poll() {
            self.handle(ctx, ev);
        }
        self.images.pump(ctx);
        self.apply_theme(ctx);
        self.update_title(ctx);
        self.take_input_files(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let now = ctx.input(|i| i.time);
        let dt = (now - self.last_frame).clamp(0.0, 0.1) as f32;
        self.last_frame = now;
        self.time = now as f32;

        match self.screen {
            Screen::Launch => self.launch_ui(ui, dt),
            Screen::Chat => self.chat_ui(ui),
        }
        self.dialogs(&ctx);
        self.toasts_ui(&ctx);

        let screen = ctx.content_rect();
        if self.confetti_queued > 0 {
            let pal = self.pal();
            let colors = [pal.accent, pal.accent2, pal.warn, pal.good, egui::Color32::WHITE];
            self.fx.rain(screen, self.confetti_queued.min(260), &colors);
            self.confetti_queued = 0;
        }
        if let Some(at) = self.burst_at.take() {
            let pal = self.pal();
            self.fx.burst(at, 70, &[pal.accent, pal.accent2, pal.warn, pal.good]);
        }
        if self.fx.active() {
            let painter = ctx.layer_painter(LayerId::new(Order::Foreground, Id::unique("devzat-fx")));
            self.fx.paint_particles(&painter, dt);
            ctx.request_repaint();
        }
        if self.settings.crt {
            fx::crt(&ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("devzat-crt"))), screen);
        }
        crate::chrome::window_frame(&ctx, &self.pal());
        if let Some(text) = self.copied.take() {
            self.toast(ToastKind::Good, text);
        }
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, eframe::APP_KEY, &self.settings);
    }

    fn auto_save_interval(&self) -> Duration {
        Duration::from_secs(5)
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        crate::chrome::release_after_grab(ctx, raw_input);
    }
}

fn default_name() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("USERNAME"))
        .unwrap_or_default()
        .chars()
        .filter(|c| c.is_ascii_graphic())
        .take(20)
        .collect()
}

/// Large photos are scaled down so they fit the server's 8 MB limit.
fn shrink_for_upload(bytes: Vec<u8>) -> Vec<u8> {
    const LIMIT: usize = 7 * 1024 * 1024;
    if bytes.len() <= LIMIT {
        return bytes;
    }
    let Ok(img) = image::load_from_memory(&bytes) else { return bytes };
    let img = img.thumbnail(2048, 2048).to_rgb8();
    let mut out = std::io::Cursor::new(Vec::new());
    if image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 85).encode_image(&img).is_ok() {
        out.into_inner()
    } else {
        bytes
    }
}
