use chrono::{DateTime, Local};
use devzat_core::ansi::{self, AnsiSpan};
use devzat_core::markup::{self, Doc};
use devzat_core::protocol::{ChatMessage, MessageKind};
use std::collections::HashSet;
use std::hash::{Hash, Hasher};

const MAX_MESSAGES: usize = 2000;

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ConvKey {
    Room(String),
    Dm(String),
}

impl ConvKey {
    pub fn title(&self) -> String {
        match self {
            ConvKey::Room(r) => r.clone(),
            ConvKey::Dm(p) => format!("@{p}"),
        }
    }
}

pub struct UiMessage {
    pub key: u64,
    pub kind: MessageKind,
    pub ts: DateTime<Local>,
    /// Plain name of whoever wrote it. Empty for system lines.
    pub author: String,
    pub author_spans: Vec<AnsiSpan>,
    pub doc: Doc,
    pub plain: String,
    pub mention: bool,
    pub mine: bool,
    pub join_or_leave: bool,
}

#[derive(Default)]
pub struct Conversation {
    pub messages: Vec<UiMessage>,
    seen: HashSet<u64>,
    pub unread: u32,
    pub mentions: u32,
    pub last: Option<DateTime<Local>>,
}

impl Conversation {
    /// Adds a message in time order. Returns false for a repeat (history replays resend old lines).
    pub fn insert(&mut self, m: UiMessage) -> bool {
        if !self.seen.insert(m.key) {
            return false;
        }
        self.last = Some(self.last.map_or(m.ts, |l| l.max(m.ts)));
        let pos = self.messages.partition_point(|x| x.ts <= m.ts);
        self.messages.insert(pos, m);
        if self.messages.len() > MAX_MESSAGES {
            let extra = self.messages.len() - MAX_MESSAGES;
            self.messages.drain(..extra);
        }
        true
    }
}

pub fn route(m: &ChatMessage) -> ConvKey {
    match m.kind {
        MessageKind::DmIn | MessageKind::DmOut => ConvKey::Dm(m.from_plain.clone()),
        _ => ConvKey::Room(if m.room.is_empty() { "#main".into() } else { m.room.clone() }),
    }
}

pub fn key(m: &ChatMessage) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    m.ts.timestamp_nanos_opt().unwrap_or_default().hash(&mut h);
    m.from_plain.hash(&mut h);
    m.text.hash(&mut h);
    matches!(m.kind, MessageKind::DmOut).hash(&mut h);
    h.finish()
}

pub fn to_ui(m: &ChatMessage, my_plain: &str, my_ansi: &str) -> UiMessage {
    let doc = markup::parse(&m.text);
    let plain = doc.plain_text();
    let (author, author_spans, mine) = match m.kind {
        MessageKind::System => (String::new(), Vec::new(), false),
        MessageKind::DmOut => (my_plain.to_string(), ansi::parse(if my_ansi.is_empty() { my_plain } else { my_ansi }), true),
        _ => (m.from_plain.clone(), ansi::parse(&m.from), !my_plain.is_empty() && m.from_plain == my_plain),
    };
    let join_or_leave = m.kind == MessageKind::System
        && (plain.starts_with("-->") || plain.starts_with("<--") || plain.contains(" is joining "));
    UiMessage {
        key: key(m),
        kind: m.kind,
        ts: m.ts.with_timezone(&Local),
        author,
        author_spans,
        doc,
        plain,
        mention: m.mention,
        mine,
        join_or_leave,
    }
}
