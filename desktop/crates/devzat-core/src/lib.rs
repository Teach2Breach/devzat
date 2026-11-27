//! Everything the Devzat desktop clients share across platforms: the wire
//! protocol, the SSH connection, identities, host key trust, and parsing of
//! devzat's markdown-with-ANSI message text.

pub mod ansi;
pub mod client;
pub mod identity;
pub mod markup;
pub mod paths;
pub mod protocol;
pub mod trust;

pub use client::{ClientHandle, Command, ConnectRequest, Event, HostKeyPrompt, Status};
pub use identity::Identity;
