use crate::app::DevzatApp;
use crate::model::ConvKey;
use crate::theme::{self, alpha};
use crate::widgets;
use eframe::egui::{
    self, Align2, CornerRadius, FontId, Key, KeyboardShortcut, Margin, Modifiers, RichText, Sense, Stroke, StrokeKind, vec2,
};

/// Devzat's own commands, plus three the app handles locally (marked local).
pub const COMMANDS: &[(&str, &str, &str, bool)] = &[
    ("cd", "#room", "Join a room, or list rooms", false),
    ("nick", "name", "Change your name", false),
    ("color", "name|#hex|rainbow", "Change your name's color", false),
    ("users", "", "List people in this room", false),
    ("history", "", "Replay this room's saved messages", false),
    ("bio", "[user]", "Show someone's bio, or set yours", false),
    ("tic", "1-9", "Play tic-tac-toe", false),
    ("hang", "word|letter", "Play hangman", false),
    ("8ball", "question", "Ask the magic 8-ball", false),
    ("shrug", "[text]", "¯\\_(ツ)_/¯", false),
    ("art", "", "Panda art", false),
    ("emojis", "", "Emoji shortcodes", false),
    ("uptime", "", "How long the server has been up", false),
    ("help", "", "Devzat's help", false),
    ("cmds", "", "All devzat commands", false),
    ("id", "user", "Show someone's ID", false),
    ("admins", "", "List admin IDs", false),
    ("kick", "user", "Kick someone (admin)", false),
    ("ban", "user [reason] [duration]", "Ban someone (admin)", false),
    ("mute", "user", "Mute someone (admin)", false),
    ("unmute", "user", "Unmute someone (admin)", false),
    ("exit", "", "Leave the server", false),
    ("confetti", "", "Celebrate, just on your screen", true),
    ("theme", "neon|phosphor|sunset|paper", "Switch theme", true),
    ("crt", "", "Toggle CRT scanlines", true),
];

pub fn is_server_command(name: &str) -> bool {
    COMMANDS.iter().any(|(n, _, _, local)| *n == name && !local)
        || matches!(name, "man" | "bell" | "clear" | "tz" | "prompt" | "eg-code" | "lsbans" | "unban" | "pwd" | "uname" | "rmdir" | "logs" | "devmonk")
}

const EMOJI: &[&str] = &[
    "😀", "😂", "🥲", "😊", "😍", "😎", "🤔", "😅", "😭", "😡", "🥳", "😴", "🤯", "🙃", "😬", "🤖",
    "👍", "👎", "👏", "🙌", "🙏", "💪", "👀", "🤝", "✌", "🤘", "👋", "🫡", "❤", "🧡", "💛", "💚",
    "💙", "💜", "🔥", "✨", "⭐", "🎉", "🎊", "💯", "✅", "❌", "⚡", "💡", "🚀", "🛸", "🌈", "☕",
    "🍕", "🍩", "🍺", "🎮", "🎧", "💻", "🐧", "🦀", "🐍", "🐱", "🐶", "🐼", "🦊", "🌮", "🌙", "☀",
];

enum Popup {
    Commands(Vec<usize>),
    Mentions(Vec<String>),
}

impl DevzatApp {
    fn popup(&self) -> Option<Popup> {
        let text = &self.composer.text;
        if let Some(rest) = text.strip_prefix('/') {
            let finished = COMMANDS.iter().any(|(n, ..)| *n == rest);
            if !rest.contains(' ') && !finished {
                let hits: Vec<usize> = COMMANDS
                    .iter()
                    .enumerate()
                    .filter(|(_, (n, ..))| n.starts_with(rest))
                    .map(|(i, _)| i)
                    .take(8)
                    .collect();
                if !hits.is_empty() {
                    return Some(Popup::Commands(hits));
                }
            }
        }
        let last = text.rsplit(char::is_whitespace).next().unwrap_or("");
        if let Some(prefix) = last.strip_prefix('@') {
            let me = self.my_plain();
            let hits: Vec<String> = self
                .state
                .users
                .iter()
                .filter(|u| u.plain != me && u.plain.to_lowercase().starts_with(&prefix.to_lowercase()))
                .map(|u| u.plain.clone())
                .take(6)
                .collect();
            if !hits.is_empty() && !(hits.len() == 1 && hits[0] == prefix) {
                return Some(Popup::Mentions(hits));
            }
        }
        None
    }

    fn accept_popup(&mut self, popup: &Popup, index: usize) {
        match popup {
            Popup::Commands(hits) => {
                let (name, args, _, _) = COMMANDS[hits[index.min(hits.len() - 1)]];
                self.composer.text = if args.is_empty() { format!("/{name}") } else { format!("/{name} ") };
            }
            Popup::Mentions(names) => {
                let name = &names[index.min(names.len() - 1)];
                let cut = self.composer.text.rfind('@').unwrap_or(self.composer.text.len());
                self.composer.text.truncate(cut);
                self.composer.text.push_str(&format!("@{name} "));
            }
        }
        self.composer.popup_index = 0;
        self.composer.want_focus = true;
    }

    pub fn composer_ui(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        let popup = self.popup();
        let popup_len = match &popup {
            Some(Popup::Commands(h)) => h.len(),
            Some(Popup::Mentions(n)) => n.len(),
            None => 0,
        };
        if popup_len == 0 {
            self.composer.popup_index = 0;
        } else {
            self.composer.popup_index = self.composer.popup_index.min(popup_len - 1);
        }

        let edit_id = egui::Id::unique("composer-edit");
        let has_focus = ui.memory(|m| m.has_focus(edit_id));
        let mut accept = false;
        if has_focus && popup.is_some() {
            ui.input_mut(|i| {
                if i.consume_key(Modifiers::NONE, Key::ArrowDown) {
                    self.composer.popup_index = (self.composer.popup_index + 1) % popup_len;
                }
                if i.consume_key(Modifiers::NONE, Key::ArrowUp) {
                    self.composer.popup_index = (self.composer.popup_index + popup_len - 1) % popup_len;
                }
                if i.consume_key(Modifiers::NONE, Key::Tab) || i.consume_key(Modifiers::NONE, Key::Enter) {
                    accept = true;
                }
            });
        }
        if accept
            && let Some(p) = &popup {
                self.accept_popup(p, self.composer.popup_index);
            }

        if let Some(p) = &popup {
            self.popup_ui(ui, p);
        }

        let placeholder = match &self.active {
            ConvKey::Room(r) => format!("Message {r}"),
            ConvKey::Dm(p) => format!("Message @{p} privately"),
        };
        let mut send_now = false;
        let frame_stroke = if has_focus { Stroke::new(1.5, alpha(pal.accent, 0.8)) } else { Stroke::new(1.0, pal.border) };
        egui::Frame::new()
            .fill(pal.surface)
            .stroke(frame_stroke)
            .corner_radius(CornerRadius::same(14))
            .inner_margin(Margin::symmetric(8, 6))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if widgets::icon_button(ui, "📎", "Share a picture (or paste / drop one)", &pal).clicked() {
                        self.pick_image(ui.ctx());
                    }
                    let spacing = ui.spacing().item_spacing.x;
                    let right_w = 30.0 + 34.0 + 34.0 + spacing * 4.0;
                    let edit_w = (ui.available_width() - right_w).max(80.0);
                    let rows = self.composer.text.lines().count().clamp(1, 6);
                    let resp = ui.add(
                        egui::TextEdit::multiline(&mut self.composer.text)
                            .id(edit_id)
                            .frame(egui::Frame::NONE)
                            .hint_text(placeholder)
                            .desired_rows(rows)
                            .desired_width(edit_w)
                            .font(FontId::proportional(15.0))
                            .margin(vec2(4.0, 6.0))
                            .lock_focus(true)
                            .return_key(Some(KeyboardShortcut::new(Modifiers::SHIFT, Key::Enter))),
                    );
                    if self.composer.want_focus {
                        resp.request_focus();
                        self.composer.want_focus = false;
                    }
                    if resp.has_focus() && popup.is_none() {
                        let (enter, up) = ui.input(|i| (i.key_pressed(Key::Enter) && !i.modifiers.shift, i.key_pressed(Key::ArrowUp)));
                        if enter {
                            send_now = true;
                        }
                        if up && (self.composer.text.is_empty() || self.composer.recall.is_some()) && !self.composer.sent.is_empty() {
                            let i = self.composer.recall.map_or(self.composer.sent.len() - 1, |i| i.saturating_sub(1));
                            self.composer.recall = Some(i);
                            self.composer.text = self.composer.sent[i].clone();
                        }
                    }

                    let emoji = widgets::icon_button(ui, "😀", "Emoji", &pal);
                    if emoji.clicked() {
                        self.composer.emoji_open = !self.composer.emoji_open;
                    }
                    self.hq_toggle(ui);
                    let can_send = !self.composer.text.trim().is_empty();
                    let (r, s) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                    let p = ui.painter();
                    if can_send {
                        widgets::gradient_rect(p, r, 17.0, pal.accent, pal.accent2);
                    } else {
                        p.rect_filled(r, CornerRadius::same(17), pal.surface_hi);
                    }
                    p.text(r.center() + vec2(1.0, 0.0), Align2::CENTER_CENTER, "➤", FontId::proportional(16.0), if can_send { pal.on_accent } else { pal.text_dim });
                    if s.on_hover_text("Send (Enter). Shift+Enter adds a line.").clicked() && can_send {
                        send_now = true;
                    }
                });
            });
        if self.composer.emoji_open {
            self.emoji_picker(ui);
        }
        if send_now {
            let text = std::mem::take(&mut self.composer.text);
            self.submit(text);
            self.composer.want_focus = true;
            self.composer.emoji_open = false;
        }
    }

    fn hq_toggle(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        let (r, resp) = ui.allocate_exact_size(vec2(34.0, 24.0), Sense::click());
        let on = self.composer.hq;
        let p = ui.painter();
        p.rect_filled(r, CornerRadius::same(6), if on { alpha(pal.accent2, 0.22) } else { pal.surface_hi });
        p.rect_stroke(r, CornerRadius::same(6), Stroke::new(1.0, if on { pal.accent2 } else { pal.border }), StrokeKind::Inside);
        p.text(r.center(), Align2::CENTER_CENTER, "HQ", theme::bold(11.0), if on { pal.accent2 } else { pal.text_dim });
        let tip = if on {
            "Pictures you share now are shown sharp for everyone (HQ). Click to go back to retro pixels."
        } else {
            "Pictures you share now use devzat's retro pixel look. Click to send the next ones sharp (HQ)."
        };
        if resp.on_hover_text(tip).clicked() {
            self.composer.hq = !self.composer.hq;
        }
    }

    fn popup_ui(&mut self, ui: &mut egui::Ui, popup: &Popup) {
        let pal = self.pal();
        let mut chosen = None;
        egui::Frame::new()
            .fill(pal.surface)
            .stroke(Stroke::new(1.0, pal.border))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(6))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                let title = match popup {
                    Popup::Commands(_) => "COMMANDS · Tab or Enter to pick",
                    Popup::Mentions(_) => "PEOPLE · Tab or Enter to pick",
                };
                ui.label(RichText::new(title).font(theme::bold(10.0)).color(pal.text_dim));
                let rows: Vec<(String, String, bool)> = match popup {
                    Popup::Commands(hits) => hits
                        .iter()
                        .map(|&i| {
                            let (n, a, d, local) = COMMANDS[i];
                            (format!("/{n} {a}"), d.to_string(), local)
                        })
                        .collect(),
                    Popup::Mentions(names) => names.iter().map(|n| (format!("@{n}"), String::new(), false)).collect(),
                };
                for (i, (head, desc, local)) in rows.iter().enumerate() {
                    let selected = i == self.composer.popup_index;
                    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click());
                    if selected || resp.hovered() {
                        ui.painter().rect_filled(r, CornerRadius::same(7), if selected { theme::lerp(pal.surface_hi, pal.accent, 0.2) } else { pal.surface_hi });
                    }
                    ui.painter().text(r.left_center() + vec2(8.0, 0.0), Align2::LEFT_CENTER, head, FontId::monospace(13.0), pal.text);
                    let mut right = desc.clone();
                    if *local {
                        right.push_str("  · just you");
                    }
                    ui.painter().text(r.right_center() - vec2(8.0, 0.0), Align2::RIGHT_CENTER, right, FontId::proportional(12.0), pal.text_dim);
                    if resp.clicked() {
                        chosen = Some(i);
                    }
                }
            });
        if let Some(i) = chosen {
            self.accept_popup(popup, i);
        }
        ui.add_space(4.0);
    }

    fn emoji_picker(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        let mut pick = None;
        egui::Frame::new()
            .fill(pal.surface)
            .stroke(Stroke::new(1.0, pal.border))
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(8))
            .outer_margin(Margin { top: 6, ..Default::default() })
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal_wrapped(|ui| {
                    ui.spacing_mut().item_spacing = vec2(2.0, 2.0);
                    for e in EMOJI {
                        let (r, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                        if resp.hovered() {
                            ui.painter().rect_filled(r, CornerRadius::same(8), pal.surface_hi);
                        }
                        let size = if resp.hovered() { 24.0 } else { 20.0 };
                        ui.painter().text(r.center(), Align2::CENTER_CENTER, *e, FontId::proportional(size), pal.text);
                        if resp.clicked() {
                            pick = Some(*e);
                        }
                    }
                });
            });
        if let Some(e) = pick {
            self.composer.text.push_str(e);
            self.composer.want_focus = true;
        }
    }

    pub fn pick_image(&mut self, ctx: &egui::Context) {
        let file = rfd::FileDialog::new()
            .set_title("Share a picture")
            .add_filter("Pictures", &["png", "jpg", "jpeg", "gif", "webp"])
            .pick_file();
        if let Some(path) = file {
            let name = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            match std::fs::read(&path) {
                Ok(bytes) => self.stage_image(ctx, bytes, name),
                Err(e) => self.toast(crate::app::ToastKind::Bad, format!("Couldn't read {name}: {e}")),
            }
        }
    }
}
