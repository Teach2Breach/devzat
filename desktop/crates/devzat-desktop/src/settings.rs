use crate::theme::ThemeId;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerProfile {
    pub host: String,
    pub port: u16,
    pub name: String,
}

impl ServerProfile {
    pub fn label(&self) -> String {
        if self.port == 22 { self.host.clone() } else { format!("{}:{}", self.host, self.port) }
    }
}

impl Default for ServerProfile {
    fn default() -> Self {
        ServerProfile { host: String::new(), port: 22, name: String::new() }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: ThemeId,
    pub zoom: f32,
    /// Show ordinary pictures pixelated like the terminal does. `!hq` pictures are always sharp.
    pub retro_images: bool,
    pub clock_24h: bool,
    pub show_joins: bool,
    pub crt: bool,
    pub effects: bool,
    pub show_members: bool,
    pub use_openssh_known_hosts: bool,
    /// A key the user brought (like ~/.ssh/id_ed25519). None means the app's own key.
    pub identity_path: Option<PathBuf>,
    pub servers: Vec<ServerProfile>,
    pub last: ServerProfile,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: ThemeId::Neon,
            zoom: 1.0,
            retro_images: true,
            clock_24h: false,
            show_joins: true,
            crt: false,
            effects: true,
            show_members: true,
            use_openssh_known_hosts: true,
            identity_path: None,
            servers: Vec::new(),
            last: ServerProfile::default(),
        }
    }
}

impl Settings {
    pub fn remember_server(&mut self, p: &ServerProfile) {
        self.servers.retain(|s| !(s.host == p.host && s.port == p.port));
        self.servers.insert(0, p.clone());
        self.servers.truncate(8);
        self.last = p.clone();
    }
}
