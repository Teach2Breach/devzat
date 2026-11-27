//! The `devzat-json` subsystem: newline-delimited JSON over an SSH channel.

use chrono::{DateTime, FixedOffset};
use serde::{Deserialize, Serialize};

pub const SUBSYSTEM: &str = "devzat-json";

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ServerEvent {
    Welcome(Welcome),
    Msg(ChatMessage),
    State(RoomState),
    History(HistoryMark),
    Clear,
    Bye {
        #[serde(default)]
        reason: String,
    },
    Ack(Ack),
    Blob(RawBlob),
    Pong(Ack),
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Welcome {
    #[serde(default)]
    pub protocol: u32,
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub plain: String,
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub history_hours: i64,
    #[serde(default)]
    pub max_upload: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageKind {
    Chat,
    Bot,
    System,
    DmIn,
    DmOut,
    #[serde(other)]
    Other,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChatMessage {
    pub kind: MessageKind,
    #[serde(default)]
    pub room: String,
    /// Display name with ANSI colors. For `DmOut` this is the recipient.
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub from_plain: String,
    /// Markdown that may contain ANSI color codes.
    #[serde(default)]
    pub text: String,
    pub ts: DateTime<FixedOffset>,
    #[serde(default)]
    pub mention: bool,
    #[serde(default)]
    pub history: bool,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct RoomInfo {
    pub name: String,
    #[serde(default)]
    pub count: usize,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct UserInfo {
    pub name: String,
    pub plain: String,
    #[serde(default)]
    pub you: bool,
    #[serde(default)]
    pub admin: bool,
    #[serde(default)]
    pub idle: bool,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
pub struct RoomState {
    pub room: String,
    #[serde(default)]
    pub dm: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub plain: String,
    #[serde(default)]
    pub rooms: Vec<RoomInfo>,
    #[serde(default)]
    pub users: Vec<UserInfo>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HistoryMark {
    pub phase: String,
    #[serde(default)]
    pub room: String,
    #[serde(default)]
    pub label: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Ack {
    #[serde(default)]
    pub req: u64,
    #[serde(default)]
    pub ok: bool,
    #[serde(default, rename = "ref")]
    pub reference: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RawBlob {
    #[serde(default)]
    pub req: u64,
    #[serde(default, rename = "ref")]
    pub reference: String,
    #[serde(default)]
    pub mime: String,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum Request {
    Send {
        text: String,
    },
    Join {
        room: String,
    },
    Upload {
        req: u64,
        data: String,
        caption: String,
        hq: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        to: Option<String>,
    },
    Fetch {
        req: u64,
        #[serde(rename = "ref")]
        reference: String,
    },
    Ping {
        req: u64,
    },
}

impl Request {
    pub fn to_line(&self) -> Vec<u8> {
        let mut line = serde_json::to_vec(self).unwrap_or_default();
        line.push(b'\n');
        line
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_server_lines() {
        let msg: ServerEvent = serde_json::from_str(
            r##"{"t":"msg","kind":"chat","room":"#main","from":"\u001b[38;5;218malice\u001b[39m","from_plain":"alice","text":"hi","ts":"2026-11-26T23:54:54.428173007-06:00"}"##,
        )
        .unwrap();
        assert!(matches!(msg, ServerEvent::Msg(ref m) if m.from_plain == "alice" && m.kind == MessageKind::Chat));
        let other: ServerEvent = serde_json::from_str(r#"{"t":"something_new","x":1}"#).unwrap();
        assert!(matches!(other, ServerEvent::Unknown));
        let line = String::from_utf8(Request::Join { room: "#fun".into() }.to_line()).unwrap();
        assert_eq!(line, "{\"t\":\"join\",\"room\":\"#fun\"}\n");
    }
}
