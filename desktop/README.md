# Devzat Desktop

A desktop client for Devzat. It is still chat over SSH: the app logs in with an SSH key, checks the server's host key, and talks to the server inside an encrypted SSH channel. People using the desktop app and people using a terminal are in the same rooms.

The same code builds for Linux and Windows.

## What it does

- Rooms, direct messages, a member list, mentions, and unread badges
- Markdown, code blocks, links, and devzat's colored names (rainbow names included)
- Pictures: paste, drag in, or attach one. Ordinary pictures keep devzat's pixelated look; send one as HQ and it shows sharp. Click any picture to see it full size.
- Timestamps in your own time zone, with the server's 24 hours of history when you join
- `/` opens a command list, `@` completes names, and there's an emoji picker
- Four themes (Neon, Phosphor, Sunset, Paper), optional CRT scanlines, and confetti
- A borderless window with its own title bar: drag the top bar to move it, double-click to maximize, and resize from any edge or corner
- Reconnects on its own after a network drop

## Security

- **Your identity is an SSH key.** On first run the app makes an Ed25519 key in its data folder. You can switch to an existing key such as `~/.ssh/id_ed25519` to keep the same name, color, and admin rights you have in the terminal. Passphrase-protected keys are unlocked in memory only.
- **Server keys are checked.** The first time you connect, the app shows the server's fingerprint and OpenSSH-style randomart. If the key ever changes, you get a warning and the app refuses to connect until you confirm. Servers already trusted in `~/.ssh/known_hosts` are recognized.
- **Pictures come through the server.** The app never contacts image hosts directly, so posting a link can't reveal your IP address to whoever runs that host.

## Server requirement

The app needs a devzat server from this repository that includes the `devzat-json` SSH subsystem (`gui.go`). Nothing has to be configured; the subsystem is on whenever the server runs.

## Build and run

You need Rust 1.95 or newer from [rustup.rs](https://rustup.rs).

### Linux

```sh
cd desktop
cargo run --release
```

The binary is `target/release/devzat-desktop`. It runs on X11 and Wayland with any working Vulkan or OpenGL driver.

### Windows

On Windows, install Rust with the default (MSVC) toolchain and run the same commands. The result is `target\release\devzat-desktop.exe`, a single file with no extra DLLs.

To build the Windows `.exe` from Linux instead:

```sh
rustup target add x86_64-pc-windows-gnu
sudo apt install mingw-w64
cd desktop
cargo build --release --target x86_64-pc-windows-gnu
```

The output is `target/x86_64-pc-windows-gnu/release/devzat-desktop.exe`.

## Where it keeps things

Everything lives in one folder: `~/.local/share/devzat-desktop/` on Linux and `%APPDATA%\devzat-desktop\` on Windows. `identity_ed25519` is your key, `known_hosts` holds the server keys you trusted, and `app.ron` holds settings and saved servers.

Set `DEVZAT_DESKTOP_HOME=/some/folder` to keep everything in one throwaway folder, which is handy for testing with a second identity.

## Trying it against a local server

From the repository root:

```sh
go build -o /tmp/devzat .
mkdir -p /tmp/devzat-try
printf 'port: 2221\nalt_port: 2222\nprofile_port: 5556\ndata_dir: /tmp/devzat-try/data\nkey_file: /tmp/devzat-try/hostkey\n' > /tmp/devzat-try/devzat.yml
DEVZAT_CONFIG=/tmp/devzat-try/devzat.yml /tmp/devzat
```

Then start the app, enter `127.0.0.1`, port `2221`, and a name. A terminal user can join the same chat with `ssh -p 2221 you@127.0.0.1`.

## Layout

- `crates/devzat-core`: everything that isn't drawing. The SSH connection and reconnects, the `devzat-json` protocol, keys and host key trust, and parsing devzat's markdown-with-ANSI messages.
- `crates/devzat-desktop`: the egui interface.

## Licenses

The app is MIT licensed, like devzat. Its dependencies are under MIT, Apache-2.0, BSD, ISC, Zlib, and similar licenses; `deny.toml` lists the allowed set, so `cargo deny check licenses` verifies it. The bundled fonts, Inter, JetBrains Mono, and Press Start 2P, are under the SIL Open Font License, and their license files are in `crates/devzat-desktop/assets/fonts/`.
