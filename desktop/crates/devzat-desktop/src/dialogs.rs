use crate::app::{DevzatApp, ToastKind};
use crate::images::ImgState;
use crate::theme::{self, ThemeId, alpha};
use crate::widgets;
use devzat_core::Command;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Id, Key, Layout, Margin, Order, RichText, Sense, Stroke, StrokeKind,
    vec2,
};

impl DevzatApp {
    pub fn dialogs(&mut self, ctx: &egui::Context) {
        self.host_key_dialog(ctx);
        self.name_prompt_dialog(ctx);
        self.key_import_dialog(ctx);
        self.pubkey_dialog(ctx);
        self.new_room_dialog(ctx);
        self.send_image_dialog(ctx);
        self.settings_window(ctx);
        self.lightbox_ui(ctx);
    }

    fn host_key_dialog(&mut self, ctx: &egui::Context) {
        let Some((prompt, ack)) = self.hostkey.as_mut() else { return };
        let pal = self.settings.theme.palette();
        let changed = prompt.changed.is_some();
        let mut decision: Option<bool> = None;
        let label = devzat_core::trust::host_label(&prompt.host, prompt.port);
        egui::Modal::new(Id::unique("hostkey")).show(ctx, |ui| {
            ui.set_width(460.0);
            if changed {
                ui.label(RichText::new("⚠  This server's key changed").font(theme::bold(20.0)).color(pal.bad));
                ui.add_space(6.0);
                ui.label(format!(
                    "{label} is presenting a different key than the one you trusted before. That happens when a server is reinstalled, and it's also what an impostor looks like."
                ));
                if let Some(Some(old)) = &prompt.changed {
                    ui.add_space(4.0);
                    ui.label(RichText::new(format!("Before: {old}")).monospace().size(12.0).color(pal.text_dim));
                }
            } else {
                ui.label(RichText::new("First visit to this server").font(theme::bold(20.0)));
                ui.add_space(6.0);
                ui.label(format!(
                    "Check that this fingerprint matches what the admin of {label} shared. After you trust it, the app warns you if it ever changes."
                ));
            }
            ui.add_space(10.0);
            egui::Frame::new()
                .fill(pal.code_bg)
                .stroke(Stroke::new(1.0, alpha(if changed { pal.bad } else { pal.accent }, 0.6)))
                .corner_radius(CornerRadius::same(10))
                .inner_margin(Margin::same(12))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_top(|ui| {
                        ui.label(RichText::new(&prompt.randomart).monospace().size(12.0).color(pal.accent2));
                        ui.add_space(8.0);
                        ui.vertical(|ui| {
                            ui.label(RichText::new(&prompt.algorithm).font(theme::bold(12.0)).color(pal.text_dim));
                            ui.add(egui::Label::new(RichText::new(&prompt.fingerprint).monospace().size(12.5)).wrap());
                            ui.add_space(6.0);
                            ui.label(
                                RichText::new("The admin can print this with:\nssh-keygen -lv -f devzat-sshkey")
                                    .size(11.5)
                                    .color(pal.text_dim),
                            );
                        });
                    });
                });
            ui.add_space(12.0);
            if changed {
                ui.checkbox(ack, "I confirmed with the admin that the key changed");
            }
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Don't connect").clicked() {
                    decision = Some(false);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let text = if changed { "Replace key & connect" } else { "Trust & connect" };
                    let enabled = !changed || *ack;
                    if widgets::gradient_button(ui, text, vec2(210.0, 38.0), &pal, enabled).clicked() {
                        decision = Some(true);
                    }
                });
            });
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                decision = Some(false);
            }
        });
        if let Some(accept) = decision {
            let id = prompt.id;
            self.client.send(Command::TrustDecision { prompt: id, accept });
            self.hostkey = None;
        }
    }

    fn name_prompt_dialog(&mut self, ctx: &egui::Context) {
        let Some((message, name)) = self.name_prompt.as_mut() else { return };
        let pal = self.settings.theme.palette();
        let mut send: Option<String> = None;
        let mut cancel = false;
        egui::Modal::new(Id::unique("nameprompt")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Pick another name").font(theme::bold(18.0)));
            ui.label(RichText::new(message.as_str()).color(pal.text_dim));
            ui.add_space(8.0);
            let r = ui.add(egui::TextEdit::singleline(name).hint_text("new-name").desired_width(f32::INFINITY).char_limit(27));
            focus_when_ready(&r);
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    cancel = true;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Use this name").clicked() || ui.input(|i| i.key_pressed(Key::Enter)) {
                        send = Some(name.trim().to_string());
                    }
                });
            });
        });
        if let Some(n) = send.filter(|n| !n.is_empty()) {
            self.form.name = n.clone();
            self.client.send(Command::Send(n));
            self.name_prompt = None;
        }
        if cancel {
            self.name_prompt = None;
            self.disconnect();
        }
    }

    fn key_import_dialog(&mut self, ctx: &egui::Context) {
        let Some(k) = self.key_import.as_mut() else { return };
        let mut done: Option<bool> = None;
        let pal = self.settings.theme.palette();
        egui::Modal::new(Id::unique("passphrase")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.label(RichText::new("Unlock your key").font(theme::bold(18.0)));
            ui.label(RichText::new(crate::launch::short_path(&k.path)).monospace().color(pal.text_dim));
            ui.add_space(8.0);
            let r = ui.add(egui::TextEdit::singleline(&mut k.passphrase).password(true).hint_text("passphrase").desired_width(f32::INFINITY));
            focus_when_ready(&r);
            let enter = ui.input(|i| i.key_pressed(Key::Enter));
            if let Some(e) = &k.error {
                ui.label(RichText::new(e).color(pal.bad));
            }
            ui.label(RichText::new("The passphrase is only used to unlock the key in memory. It isn't saved.").size(11.5).color(pal.text_dim));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() {
                    done = Some(false);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Unlock").clicked() || enter {
                        done = Some(true);
                    }
                });
            });
        });
        match done {
            Some(true) => {
                let path = k.path.clone();
                match devzat_core::Identity::load(&path, Some(&k.passphrase)) {
                    Ok(id) => {
                        self.identity = Ok(id);
                        self.settings.identity_path = Some(path);
                        self.key_import = None;
                        self.toast(ToastKind::Good, "Key unlocked");
                    }
                    Err(e) => k.error = Some(e.to_string()),
                }
            }
            Some(false) => {
                self.key_import = None;
                if self.identity.is_err() {
                    self.settings.identity_path = None;
                    self.reload_identity();
                }
            }
            None => {}
        }
    }

    fn pubkey_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_pubkey {
            return;
        }
        let Ok(id) = self.identity.clone() else {
            self.show_pubkey = false;
            return;
        };
        let pal = self.pal();
        let mut close = false;
        egui::Modal::new(Id::unique("pubkey")).show(ctx, |ui| {
            ui.set_width(520.0);
            ui.label(RichText::new("Your public key").font(theme::bold(18.0)));
            ui.label(RichText::new("Safe to share. A private server's admin can add your ID to the allowlist.").color(pal.text_dim));
            ui.add_space(8.0);
            let mut text = id.public_openssh();
            ui.add(egui::TextEdit::multiline(&mut text).font(FontId::monospace(12.0)).desired_rows(3).desired_width(f32::INFINITY));
            ui.add_space(4.0);
            ui.label(RichText::new(format!("Fingerprint  {}", id.fingerprint())).monospace().size(12.0));
            ui.label(RichText::new(format!("Devzat ID    {}", id.devzat_id())).monospace().size(12.0));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Copy public key").clicked() {
                    ui.ctx().copy_text(id.public_openssh());
                }
                if ui.button("Copy ID").clicked() {
                    ui.ctx().copy_text(id.devzat_id());
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Close").clicked() {
                        close = true;
                    }
                });
            });
            if ui.input(|i| i.key_pressed(Key::Escape)) {
                close = true;
            }
        });
        if close {
            self.show_pubkey = false;
        }
    }

    fn new_room_dialog(&mut self, ctx: &egui::Context) {
        let Some(name) = self.new_room.as_mut() else { return };
        let pal = self.settings.theme.palette();
        let mut result: Option<Option<String>> = None;
        egui::Modal::new(Id::unique("newroom")).show(ctx, |ui| {
            ui.set_width(340.0);
            ui.label(RichText::new("Join or create a room").font(theme::bold(18.0)));
            ui.label(RichText::new("Rooms appear when someone joins them and disappear a day after they empty out.").color(pal.text_dim).size(12.5));
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("#").font(theme::bold(18.0)).color(pal.accent2));
                let r = ui.add(egui::TextEdit::singleline(name).hint_text("room-name").desired_width(f32::INFINITY).char_limit(29));
                focus_when_ready(&r);
            });
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                result = Some(Some(name.clone()));
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    result = Some(None);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Join").clicked() {
                        result = Some(Some(name.clone()));
                    }
                });
            });
        });
        if let Some(r) = result {
            self.new_room = None;
            if let Some(n) = r {
                let n: String = n.trim().trim_start_matches('#').chars().filter(|c| !c.is_whitespace()).collect();
                if !n.is_empty() {
                    self.join_room(&format!("#{n}"));
                }
            }
        }
    }

    fn send_image_dialog(&mut self, ctx: &egui::Context) {
        let Some(img) = self.pending_image.as_mut() else { return };
        let pal = self.settings.theme.palette();
        let mut result: Option<bool> = None;
        let target = self.active.title();
        egui::Modal::new(Id::unique("sendimage")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.label(RichText::new(format!("Share in {target}")).font(theme::bold(18.0)));
            ui.label(RichText::new(&img.name).color(pal.text_dim).size(12.0));
            ui.add_space(8.0);
            if let Some(tex) = &img.preview {
                let aspect = img.size[1] as f32 / img.size[0].max(1) as f32;
                let w = 420.0_f32.min(img.size[0] as f32);
                let mut size = vec2(w, w * aspect);
                if size.y > 300.0 {
                    size *= 300.0 / size.y;
                }
                ui.vertical_centered(|ui| {
                    ui.add(egui::Image::new((tex.id(), size)).corner_radius(CornerRadius::same(10)));
                });
            }
            ui.add_space(8.0);
            ui.add(egui::TextEdit::singleline(&mut img.caption).hint_text("Add a caption (optional)").desired_width(f32::INFINITY));
            if ui.input(|i| i.key_pressed(Key::Enter)) {
                result = Some(true);
            }
            ui.add_space(6.0);
            ui.checkbox(&mut img.hq, "Send sharp (HQ). Otherwise it shows in devzat's retro pixel style.");
            ui.label(
                RichText::new(format!(
                    "{} KB · stored on the server for as long as chat history is kept",
                    img.bytes.len() / 1024
                ))
                .size(11.5)
                .color(pal.text_dim),
            );
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(Key::Escape)) {
                    result = Some(false);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::gradient_button(ui, "Send picture", vec2(160.0, 36.0), &pal, true).clicked() {
                        result = Some(true);
                    }
                });
            });
        });
        match result {
            Some(true) => self.send_pending_image(),
            Some(false) => self.pending_image = None,
            None => {}
        }
    }

    fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let pal = self.pal();
        let mut open = true;
        egui::Window::new(RichText::new("Settings").font(theme::bold(17.0)))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(540.0)
            .anchor(Align2::CENTER_CENTER, vec2(0.0, 0.0))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (i, tab) in ["Appearance", "Chat", "Security", "About"].iter().enumerate() {
                        ui.selectable_value(&mut self.settings_tab, i, *tab);
                    }
                });
                ui.separator();
                ui.add_space(4.0);
                match self.settings_tab {
                    0 => self.settings_appearance(ui),
                    1 => self.settings_chat(ui),
                    2 => self.settings_security(ui),
                    _ => about(ui, &pal),
                }
            });
        self.settings_open = open;
    }

    fn settings_appearance(&mut self, ui: &mut egui::Ui) {
        let current = self.settings.theme;
        ui.label(RichText::new("Theme").font(theme::bold(14.0)));
        ui.horizontal_wrapped(|ui| {
            for t in ThemeId::ALL {
                let p = t.palette();
                let (r, resp) = ui.allocate_exact_size(vec2(118.0, 84.0), Sense::click());
                let painter = ui.painter();
                painter.rect_filled(r, CornerRadius::same(12), p.bg);
                let side = egui::Rect::from_min_size(r.min, vec2(30.0, r.height()));
                painter.rect_filled(side, CornerRadius { nw: 12, sw: 12, ne: 0, se: 0 }, p.panel);
                widgets::gradient_rect(painter, egui::Rect::from_min_size(r.min + vec2(40.0, 14.0), vec2(64.0, 10.0)), 5.0, p.accent, p.accent2);
                painter.rect_filled(egui::Rect::from_min_size(r.min + vec2(40.0, 30.0), vec2(50.0, 6.0)), CornerRadius::same(3), p.text_dim);
                painter.rect_filled(egui::Rect::from_min_size(r.min + vec2(40.0, 40.0), vec2(38.0, 6.0)), CornerRadius::same(3), p.text_dim);
                painter.text(r.left_bottom() + vec2(40.0, -16.0), Align2::LEFT_CENTER, p.name, theme::bold(12.5), p.text);
                let stroke = if t == current { Stroke::new(2.5, p.accent) } else if resp.hovered() { Stroke::new(1.5, p.border) } else { Stroke::new(1.0, p.border) };
                painter.rect_stroke(r, CornerRadius::same(12), stroke, StrokeKind::Inside);
                if resp.on_hover_text(p.blurb).clicked() {
                    self.settings.theme = t;
                }
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label("Size");
            ui.add(egui::Slider::new(&mut self.settings.zoom, 0.8..=1.6).step_by(0.05).custom_formatter(|v, _| format!("{:.0}%", v * 100.0)));
        });
        ui.checkbox(&mut self.settings.effects, "Animations and confetti");
        ui.checkbox(&mut self.settings.crt, "CRT scanlines (for the nostalgic)");
    }

    fn settings_chat(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        ui.checkbox(&mut self.settings.retro_images, "Retro pixel pictures")
            .on_hover_text("Ordinary pictures look like they do in the terminal. Pictures sent as HQ are always sharp.");
        ui.label(RichText::new("Turn off to see every picture sharp. Click any picture to open it full size.").size(12.0).color(pal.text_dim));
        ui.add_space(6.0);
        ui.checkbox(&mut self.settings.clock_24h, "24-hour clock");
        ui.checkbox(&mut self.settings.show_joins, "Show joins and leaves");
        ui.checkbox(&mut self.settings.show_members, "Show the member list");
        ui.add_space(8.0);
        ui.label(RichText::new("Shortcuts").font(theme::bold(14.0)));
        for (k, v) in [
            ("Enter", "send"),
            ("Shift+Enter", "new line"),
            ("↑ in an empty box", "edit your last message again"),
            ("/", "commands"),
            ("@", "mention someone"),
            ("Ctrl+V", "paste a picture"),
        ] {
            ui.horizontal(|ui| {
                ui.label(RichText::new(k).monospace().color(pal.accent2));
                ui.label(RichText::new(v).color(pal.text_dim));
            });
        }
    }

    fn settings_security(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        match self.identity.clone() {
            Ok(id) => {
                ui.label(RichText::new("Your identity").font(theme::bold(14.0)));
                ui.label(RichText::new(format!("Fingerprint  {}", id.fingerprint())).monospace().size(12.0));
                ui.label(RichText::new(format!("Devzat ID    {}", id.devzat_id())).monospace().size(12.0));
                ui.label(RichText::new(format!("Key file     {}", crate::launch::short_path(&id.path))).monospace().size(12.0));
                ui.horizontal(|ui| {
                    if ui.button("Show public key").clicked() {
                        self.show_pubkey = true;
                    }
                    if ui.button("Open key folder").clicked()
                        && let Some(dir) = id.path.parent() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(format!("file://{}", dir.display())));
                        }
                });
            }
            Err(e) => {
                ui.label(RichText::new(e).color(pal.warn));
            }
        }
        ui.add_space(10.0);
        ui.label(RichText::new("Server keys").font(theme::bold(14.0)));
        ui.checkbox(&mut self.settings.use_openssh_known_hosts, "Also trust servers already in ~/.ssh/known_hosts");
        let store = devzat_core::paths::known_hosts_file();
        let entries = std::fs::read_to_string(&store).unwrap_or_default();
        let hosts: Vec<String> = entries.lines().filter_map(|l| l.split(' ').next()).filter(|h| !h.is_empty()).map(String::from).collect();
        if hosts.is_empty() {
            ui.label(RichText::new("No servers trusted yet.").color(pal.text_dim));
        }
        let mut forget = None;
        for h in hosts {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&h).monospace());
                if ui.small_button("Forget").on_hover_text("You'll be asked to check the key again next time").clicked() {
                    forget = Some(h.clone());
                }
            });
        }
        if let Some(h) = forget {
            let (host, port) = parse_host_label(&h);
            if let Err(e) = devzat_core::trust::forget(&host, port, &store) {
                self.toast(ToastKind::Bad, e);
            }
        }
    }

    fn lightbox_ui(&mut self, ctx: &egui::Context) {
        let Some(src) = self.lightbox.clone() else { return };
        let pal = self.pal();
        let screen = ctx.content_rect();
        let mut close = ctx.input(|i| i.key_pressed(Key::Escape));
        egui::Area::new(Id::unique("lightbox")).order(Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
            let (rect, bg) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(215));
            if let Some(ImgState::Ready(pic)) = self.images.peek(&src) {
                let max = rect.shrink(48.0).size();
                let s = (max.x / pic.size[0] as f32).min(max.y / pic.size[1] as f32).min(2.0);
                let size = vec2(pic.size[0] as f32 * s, pic.size[1] as f32 * s);
                let img_rect = egui::Rect::from_center_size(rect.center(), size);
                ui.painter().image(pic.full.id(), img_rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
                let caption = format!("{} × {}  ·  click anywhere or press Esc to close", pic.size[0], pic.size[1]);
                ui.painter().text(egui::pos2(rect.center().x, img_rect.bottom() + 20.0), Align2::CENTER_CENTER, caption, FontId::proportional(12.5), pal.text_dim);
            }
            if bg.clicked() {
                close = true;
            }
        });
        if close {
            self.lightbox = None;
        }
    }

    pub fn toasts_ui(&mut self, ctx: &egui::Context) {
        let now = ctx.input(|i| i.time);
        self.toasts.retain(|t| now - t.born < 3.6);
        if self.toasts.is_empty() {
            return;
        }
        let pal = self.pal();
        let screen = ctx.content_rect();
        egui::Area::new(Id::unique("toasts"))
            .order(Order::Tooltip)
            .anchor(Align2::RIGHT_BOTTOM, vec2(-18.0, -86.0))
            .interactable(false)
            .show(ctx, |ui| {
                ui.set_max_width(screen.width().min(360.0));
                for t in &self.toasts {
                    let age = (now - t.born) as f32;
                    let fade = (1.0 - ((age - 3.0) / 0.6).clamp(0.0, 1.0)) * (age / 0.15).clamp(0.0, 1.0);
                    let color = match t.kind {
                        ToastKind::Good => pal.good,
                        ToastKind::Bad => pal.bad,
                        ToastKind::Info => pal.accent2,
                    };
                    egui::Frame::new()
                        .fill(alpha(pal.surface, 0.97 * fade))
                        .stroke(Stroke::new(1.0, alpha(color, 0.7 * fade)))
                        .corner_radius(CornerRadius::same(10))
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (r, _) = ui.allocate_exact_size(vec2(8.0, 8.0), Sense::hover());
                                ui.painter().circle_filled(r.center(), 4.0, alpha(color, fade));
                                ui.label(RichText::new(&t.text).color(alpha(pal.text, fade)));
                            });
                        });
                    ui.add_space(6.0);
                }
            });
        ctx.request_repaint_after(std::time::Duration::from_millis(50));
    }
}

/// Keeps a dialog's text box focused, but only once the window itself has focus. Grabbing text focus
/// earlier (on the first frame of a dialog shown at startup) turns on IME before the window is ready,
/// and on X11 the window then gets no keyboard or mouse input at all.
pub fn focus_when_ready(r: &egui::Response) {
    let window_focused = r.ctx.input(|i| i.viewport().focused == Some(true));
    if window_focused && !r.has_focus() && !r.lost_focus() {
        r.request_focus();
    }
}

fn parse_host_label(label: &str) -> (String, u16) {
    if let Some(rest) = label.strip_prefix('[')
        && let Some((host, port)) = rest.split_once("]:") {
            return (host.to_string(), port.parse().unwrap_or(22));
        }
    (label.to_string(), 22)
}

fn about(ui: &mut egui::Ui, pal: &theme::Palette) {
    ui.label(RichText::new(format!("Devzat Desktop {}", env!("CARGO_PKG_VERSION"))).font(theme::bold(15.0)));
    ui.label(RichText::new("A desktop client for devzat, chat over SSH. Everything travels inside an SSH connection, and your identity is your SSH key.").color(pal.text_dim));
    ui.add_space(8.0);
    ui.label(RichText::new("Open source").font(theme::bold(13.0)));
    ui.label(RichText::new(
        "MIT licensed. Built with egui/eframe, russh, tokio, serde, image, pulldown-cmark, and other MIT, Apache-2.0, BSD, ISC, and Zlib libraries. Fonts: Inter, JetBrains Mono, and Press Start 2P under the SIL Open Font License.",
    )
    .color(pal.text_dim)
    .size(12.5));
    ui.add_space(6.0);
    ui.hyperlink_to("Source code", "https://github.com/Teach2Breach/devzat");
}
