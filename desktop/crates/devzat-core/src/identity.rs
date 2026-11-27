//! The user's SSH key. Devzat identifies people by the SHA-256 of their public key.

use russh::keys::ssh_key::private::Ed25519Keypair;
use russh::keys::ssh_key::{HashAlg, LineEnding};
use russh::keys::{PrivateKey, decode_secret_key};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum IdentityError {
    #[error("this key is protected with a passphrase")]
    NeedsPassphrase,
    #[error("wrong passphrase, or the key could not be read")]
    BadKey,
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

#[derive(Clone)]
pub struct Identity {
    pub key: Arc<PrivateKey>,
    pub path: PathBuf,
    /// True for the key this app generated, false for one the user brought.
    pub generated: bool,
}

impl Identity {
    /// Loads the app's own key, creating a new Ed25519 key the first time.
    pub fn load_or_create(path: &Path) -> Result<Self, IdentityError> {
        if path.exists() {
            let mut id = Self::load(path, None)?;
            id.generated = true;
            return Ok(id);
        }
        let mut seed = [0u8; 32];
        getrandom::fill(&mut seed).map_err(|e| IdentityError::Other(e.to_string()))?;
        let mut key = PrivateKey::from(Ed25519Keypair::from_seed(&seed));
        seed.fill(0);
        key.set_comment(format!("devzat-desktop@{}", hostname()));
        let pem = key
            .to_openssh(LineEnding::LF)
            .map_err(|e| IdentityError::Other(e.to_string()))?;
        write_private(path, pem.as_bytes())?;
        Ok(Identity { key: Arc::new(key), path: path.to_path_buf(), generated: true })
    }

    /// Loads an existing key such as `~/.ssh/id_ed25519` (OpenSSH, PEM, or PuTTY format).
    pub fn load(path: &Path, passphrase: Option<&str>) -> Result<Self, IdentityError> {
        let text = std::fs::read_to_string(path)?;
        let key = match decode_secret_key(&text, passphrase) {
            Ok(k) => k,
            Err(russh::keys::Error::KeyIsEncrypted) => return Err(IdentityError::NeedsPassphrase),
            Err(_) if passphrase.is_none() && text.contains("ENCRYPTED") => return Err(IdentityError::NeedsPassphrase),
            Err(_) => return Err(IdentityError::BadKey),
        };
        if key.is_encrypted() {
            return Err(IdentityError::NeedsPassphrase);
        }
        Ok(Identity { key: Arc::new(key), path: path.to_path_buf(), generated: false })
    }

    pub fn public_openssh(&self) -> String {
        self.key.public_key().to_openssh().unwrap_or_default()
    }

    pub fn fingerprint(&self) -> String {
        self.key.public_key().fingerprint(HashAlg::Sha256).to_string()
    }

    /// The ID devzat shows for this key (`id` and `admins` commands, allowlists).
    pub fn devzat_id(&self) -> String {
        let blob = self.key.public_key().to_bytes().unwrap_or_default();
        hex(&Sha256::digest(blob))
    }

    pub fn algorithm(&self) -> String {
        self.key.algorithm().as_str().to_string()
    }
}

/// Finds keys in `~/.ssh` that a person might want to reuse so they keep their terminal identity.
pub fn existing_ssh_keys() -> Vec<PathBuf> {
    let Some(home) = crate::paths::home_dir() else {
        return Vec::new();
    };
    ["id_ed25519", "id_ecdsa", "id_rsa"]
        .iter()
        .map(|n| home.join(".ssh").join(n))
        .filter(|p| p.is_file())
        .collect()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn hostname() -> String {
    std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .ok()
        .filter(|h| !h.is_empty())
        .unwrap_or_else(|| "desktop".into())
}

fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut opts = std::fs::OpenOptions::new();
    opts.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.mode(0o600);
    }
    use std::io::Write;
    let mut f = opts.open(path)?;
    f.write_all(bytes)?;
    f.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_then_reloads_same_key() {
        let dir = std::env::temp_dir().join(format!("devzat-id-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("identity_ed25519");
        let a = Identity::load_or_create(&path).unwrap();
        let b = Identity::load_or_create(&path).unwrap();
        assert_eq!(a.devzat_id(), b.devzat_id());
        assert_eq!(a.devzat_id().len(), 64);
        assert!(a.public_openssh().starts_with("ssh-ed25519 "));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
