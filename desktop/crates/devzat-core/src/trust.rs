//! Trust-on-first-use for server host keys, plus OpenSSH-style randomart for comparing them by eye.

use russh::keys::known_hosts::{check_known_hosts, check_known_hosts_path, known_host_keys_path, learn_known_hosts_path};
use russh::keys::ssh_key::HashAlg;
use russh::keys::PublicKey;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustStatus {
    /// Seen before, either by this app or by the user's OpenSSH client.
    Trusted { via_openssh: bool },
    Unknown,
    /// A different key is on record for this server.
    Changed { previous: Option<String> },
}

pub fn host_label(host: &str, port: u16) -> String {
    if port == 22 { host.to_string() } else { format!("[{host}]:{port}") }
}

pub fn check(host: &str, port: u16, key: &PublicKey, app_store: &Path, use_openssh: bool) -> TrustStatus {
    match check_known_hosts_path(host, port, key, app_store) {
        Ok(true) => return TrustStatus::Trusted { via_openssh: false },
        Err(russh::keys::Error::KeyChanged { .. }) => {
            return TrustStatus::Changed { previous: previous_fingerprint(host, port, key, app_store) };
        }
        _ => {}
    }
    if use_openssh {
        match check_known_hosts(host, port, key) {
            Ok(true) => return TrustStatus::Trusted { via_openssh: true },
            Err(russh::keys::Error::KeyChanged { .. }) => return TrustStatus::Changed { previous: None },
            _ => {}
        }
    }
    TrustStatus::Unknown
}

fn previous_fingerprint(host: &str, port: u16, key: &PublicKey, store: &Path) -> Option<String> {
    known_host_keys_path(host, port, store)
        .ok()?
        .into_iter()
        .find(|(_, k)| k.algorithm() == key.algorithm())
        .map(|(_, k)| k.fingerprint(HashAlg::Sha256).to_string())
}

/// Records the key, replacing anything previously stored for this host in the app's store.
pub fn remember(host: &str, port: u16, key: &PublicKey, app_store: &Path) -> Result<(), String> {
    forget(host, port, app_store)?;
    learn_known_hosts_path(host, port, key, app_store).map_err(|e| e.to_string())
}

pub fn forget(host: &str, port: u16, app_store: &Path) -> Result<(), String> {
    let Ok(text) = std::fs::read_to_string(app_store) else {
        return Ok(());
    };
    let label = host_label(host, port);
    let kept: Vec<&str> = text
        .lines()
        .filter(|line| line.split(' ').next().is_none_or(|hosts| !hosts.split(',').any(|h| h == label)))
        .collect();
    let mut out = kept.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    std::fs::write(app_store, out).map_err(|e| e.to_string())
}

pub fn fingerprint(key: &PublicKey) -> String {
    key.fingerprint(HashAlg::Sha256).to_string()
}

/// The "drunken bishop" picture OpenSSH prints with `VisualHostKey`.
pub fn randomart(key: &PublicKey) -> String {
    const W: usize = 17;
    const H: usize = 9;
    const SYMBOLS: &[u8] = b" .o+=*BOX@%&#/^SE";
    let fp = key.fingerprint(HashAlg::Sha256);
    let len = SYMBOLS.len() - 1;
    let mut field = [[0usize; H]; W];
    let (mut x, mut y) = (W / 2, H / 2);
    for &byte in fp.as_bytes() {
        let mut input = byte;
        for _ in 0..4 {
            x = if input & 1 != 0 { (x + 1).min(W - 1) } else { x.saturating_sub(1) };
            y = if input & 2 != 0 { (y + 1).min(H - 1) } else { y.saturating_sub(1) };
            if field[x][y] < len - 2 {
                field[x][y] += 1;
            }
            input >>= 2;
        }
    }
    field[W / 2][H / 2] = len - 1;
    field[x][y] = len;

    let algo = key.algorithm();
    let name = algo.as_str().trim_start_matches("ssh-").to_uppercase();
    let bits = match name.as_str() {
        "ED25519" => " 256".to_string(),
        _ => String::new(),
    };
    let title = format!("[{name}{bits}]");
    let footer = "[SHA256]";
    let border = |label: &str| {
        let pad = W.saturating_sub(label.len()) / 2;
        let mut s = String::from("+");
        s.push_str(&"-".repeat(pad));
        s.push_str(label);
        s.push_str(&"-".repeat(W.saturating_sub(pad + label.len())));
        s.push('+');
        s
    };
    let mut out = border(&title);
    out.push('\n');
    for row in 0..H {
        out.push('|');
        for col in field.iter() {
            out.push(SYMBOLS[col[row].min(len)] as char);
        }
        out.push_str("|\n");
    }
    out.push_str(&border(footer));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use russh::keys::PrivateKey;
    use russh::keys::ssh_key::private::Ed25519Keypair;

    #[test]
    fn remember_check_forget() {
        let key = PrivateKey::from(Ed25519Keypair::from_seed(&[7u8; 32]));
        let other = PrivateKey::from(Ed25519Keypair::from_seed(&[9u8; 32]));
        let dir = std::env::temp_dir().join(format!("devzat-trust-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = dir.join("known_hosts");
        assert_eq!(check("chat.test", 2221, key.public_key(), &store, false), TrustStatus::Unknown);
        remember("chat.test", 2221, key.public_key(), &store).unwrap();
        assert_eq!(check("chat.test", 2221, key.public_key(), &store, false), TrustStatus::Trusted { via_openssh: false });
        assert!(matches!(check("chat.test", 2221, other.public_key(), &store, false), TrustStatus::Changed { previous: Some(_) }));
        remember("chat.test", 2221, other.public_key(), &store).unwrap();
        assert_eq!(check("chat.test", 2221, other.public_key(), &store, false), TrustStatus::Trusted { via_openssh: false });
        let art = randomart(key.public_key());
        assert_eq!(art.lines().count(), 11);
        assert!(art.starts_with("+--[ED25519 256]--+"), "{art}");
        assert!(art.ends_with("+----[SHA256]-----+"), "{art}");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
