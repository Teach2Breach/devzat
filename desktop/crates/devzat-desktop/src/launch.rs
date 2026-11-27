use crate::app::{DevzatApp, KeyImport, ToastKind};
use crate::avatar;
use crate::theme::{self, alpha};
use crate::widgets;
use devzat_core::Status;
use eframe::egui::{self, Align, CornerRadius, Key, Layout, Margin, RichText, Stroke, vec2};

impl DevzatApp {
    pub fn launch_ui(&mut self, ui: &mut egui::Ui, dt: f32) {
        let pal = self.pal();
        let rect = ui.max_rect();
        let painter = ui.painter().clone();
        if self.settings.effects {
            self.fx.backdrop(&painter, rect, self.time, dt, &pal);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(33));
        } else {
            painter.rect_filled(rect, 0.0, pal.bg);
        }

        let busy = matches!(self.status, Status::Connecting { .. } | Status::Authenticating | Status::Joining | Status::Retrying { .. })
            && self.profile.is_some();

        let (strip, _) = ui.allocate_exact_size(vec2(rect.width(), crate::chrome::TITLE_H), egui::Sense::hover());
        crate::chrome::title_strip(ui, strip, &pal);

        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                let top = (ui.available_height() * 0.08).clamp(18.0, 70.0);
                ui.add_space(top);
                widgets::wordmark(ui, "DEVZAT", 46.0, self.time, &pal);
                ui.add_space(6.0);
                ui.label(RichText::new("chat over SSH · now with a window").color(pal.text_dim).size(15.0));
                ui.add_space(22.0);

                let card_w = 440.0_f32.min(ui.available_width() - 32.0);
                egui::Frame::new()
                    .fill(alpha(pal.surface, if pal.dark { 0.92 } else { 0.96 }))
                    .stroke(Stroke::new(1.0, alpha(pal.accent, 0.35)))
                    .corner_radius(CornerRadius::same(18))
                    .inner_margin(Margin::same(24))
                    .shadow(egui::Shadow { offset: [0, 14], blur: 40, spread: 0, color: egui::Color32::from_black_alpha(if pal.dark { 160 } else { 40 }) })
                    .show(ui, |ui| {
                        ui.set_width(card_w - 48.0);
                        ui.with_layout(Layout::top_down(Align::Min), |ui| {
                            self.server_form(ui, busy);
                            ui.add_space(14.0);
                            self.identity_card(ui);
                        });
                    });

                if !self.settings.servers.is_empty() && !busy {
                    ui.add_space(16.0);
                    self.saved_servers(ui);
                }

                ui.add_space(18.0);
                ui.label(
                    RichText::new("Your private key never leaves this computer. Servers know you by its fingerprint.")
                        .color(pal.text_dim)
                        .size(12.0),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let w = 260.0;
                    ui.add_space((ui.available_width() - w).max(0.0) / 2.0);
                    if ui.link(RichText::new("⚙ Settings").size(12.0)).clicked() {
                        self.settings_open = true;
                    }
                    ui.label(RichText::new("·").color(pal.text_dim));
                    ui.label(RichText::new(format!("v{}", env!("CARGO_PKG_VERSION"))).color(pal.text_dim).size(12.0));
                });
                ui.add_space(24.0);
            });
        });
    }

    fn server_form(&mut self, ui: &mut egui::Ui, busy: bool) {
        let pal = self.pal();
        ui.label(RichText::new("Join a server").font(theme::bold(18.0)));
        ui.add_space(10.0);

        let field = |ui: &mut egui::Ui, label: &str| {
            ui.label(RichText::new(label).color(pal.text_dim).size(12.0));
        };
        let submit = ui.input(|i| i.key_pressed(Key::Enter))
            && self.hostkey.is_none()
            && self.key_import.is_none()
            && !self.show_pubkey
            && !self.settings_open;
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                field(ui, "SERVER");
                let w = ui.available_width() - 92.0;
                ui.add_enabled(
                    !busy,
                    egui::TextEdit::singleline(&mut self.form.host)
                        .hint_text("chat.example.com")
                        .desired_width(w)
                        .margin(vec2(10.0, 8.0)),
                );
            });
            ui.vertical(|ui| {
                field(ui, "PORT");
                ui.add_enabled(!busy, egui::TextEdit::singleline(&mut self.port_text).desired_width(70.0).margin(vec2(10.0, 8.0)));
            });
        });
        ui.add_space(6.0);
        field(ui, "DISPLAY NAME");
        ui.add_enabled(
            !busy,
            egui::TextEdit::singleline(&mut self.form.name)
                .hint_text("your-name")
                .desired_width(f32::INFINITY)
                .margin(vec2(10.0, 8.0)),
        );
        ui.add_space(14.0);

        if busy {
            let label = match &self.status {
                Status::Connecting { attempt } if *attempt > 1 => format!("Connecting… (try {attempt})"),
                Status::Connecting { .. } => "Connecting…".to_string(),
                Status::Authenticating => "Checking your key…".to_string(),
                Status::Joining => "Joining the chat…".to_string(),
                Status::Retrying { retry_in, reason, .. } => format!("{reason}. Trying again in {}s…", retry_in.as_secs()),
                _ => String::new(),
            };
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(18.0).color(pal.accent));
                ui.scope(|ui| {
                    ui.set_max_width(ui.available_width() - 80.0);
                    ui.add(egui::Label::new(RichText::new(label).color(pal.text)).wrap());
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Cancel").clicked() {
                        self.disconnect();
                    }
                });
            });
        } else {
            let ready = self.identity.is_ok();
            if widgets::gradient_button(ui, "Connect", vec2(ui.available_width(), 44.0), &pal, ready).clicked() || (submit && ready) {
                self.connect();
            }
        }
        if let Some(err) = &self.launch_error
            && !busy {
                ui.add_space(8.0);
                egui::Frame::new()
                    .fill(alpha(pal.bad, 0.12))
                    .stroke(Stroke::new(1.0, alpha(pal.bad, 0.5)))
                    .corner_radius(CornerRadius::same(8))
                    .inner_margin(Margin::symmetric(10, 8))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.label(RichText::new(err).color(pal.text));
                    });
            }
    }

    fn identity_card(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        egui::Frame::new()
            .fill(pal.surface_hi)
            .corner_radius(CornerRadius::same(12))
            .inner_margin(Margin::same(12))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                match self.identity.clone() {
                    Ok(id) => {
                        let devzat_id = id.devzat_id();
                        ui.horizontal(|ui| {
                            avatar::show(ui, 40.0, &devzat_id, &pal).on_hover_text("Your avatar comes from your key");
                            ui.vertical(|ui| {
                                let source = if id.generated { "App key".to_string() } else { short_path(&id.path) };
                                ui.label(RichText::new(format!("{source} · {}", id.algorithm())).font(theme::bold(13.0)));
                                ui.label(RichText::new(format!("ID {}…{}", &devzat_id[..8], &devzat_id[56..])).monospace().color(pal.text_dim).size(12.0))
                                    .on_hover_text("Admins use this ID for allowlists and admin rights");
                            });
                        });
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui.small_button("Copy ID").clicked() {
                                ui.ctx().copy_text(devzat_id.clone());
                                self.toast(ToastKind::Good, "Copied your devzat ID");
                            }
                            if ui.small_button("Public key").clicked() {
                                self.show_pubkey = true;
                            }
                            self.key_menu(ui, id.generated);
                        });
                    }
                    Err(e) => {
                        ui.label(RichText::new("No key loaded").font(theme::bold(13.0)).color(pal.warn));
                        ui.label(RichText::new(e).color(pal.text_dim).size(12.0));
                        ui.horizontal(|ui| {
                            if ui.small_button("Use the app's own key").clicked() {
                                self.settings.identity_path = None;
                                self.reload_identity();
                            }
                            self.key_menu(ui, false);
                        });
                    }
                }
            });
    }

    fn key_menu(&mut self, ui: &mut egui::Ui, generated: bool) {
        ui.menu_button("Change key ▾", |ui| {
            for path in devzat_core::identity::existing_ssh_keys() {
                if ui.button(format!("Use {}", short_path(&path))).on_hover_text("Keep the same identity you use in the terminal").clicked() {
                    self.use_key(path);
                    ui.close();
                }
            }
            if ui.button("Choose a key file…").clicked() {
                if let Some(path) = rfd::FileDialog::new().set_title("Choose your SSH private key").pick_file() {
                    self.use_key(path);
                }
                ui.close();
            }
            if !generated && ui.button("Go back to the app's own key").clicked() {
                self.settings.identity_path = None;
                self.reload_identity();
                ui.close();
            }
        });
    }

    fn use_key(&mut self, path: std::path::PathBuf) {
        match devzat_core::Identity::load(&path, None) {
            Ok(id) => {
                self.settings.identity_path = Some(path);
                self.identity = Ok(id);
                self.toast(ToastKind::Good, "Key loaded");
            }
            Err(devzat_core::identity::IdentityError::NeedsPassphrase) => {
                self.key_import = Some(KeyImport { path, passphrase: String::new(), error: None });
            }
            Err(e) => self.toast(ToastKind::Bad, format!("Couldn't use that key: {e}")),
        }
    }

    fn saved_servers(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        ui.label(RichText::new("RECENT").font(theme::bold(10.5)).color(pal.text_dim));
        ui.add_space(4.0);
        let mut pick = None;
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            let total: f32 = self.settings.servers.iter().map(|s| 40.0 + s.label().len() as f32 * 7.5 + s.name.len() as f32 * 7.0).sum();
            ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
            for (i, s) in self.settings.servers.iter().enumerate() {
                let text = RichText::new(format!("{}  ·  {}", s.label(), s.name)).size(12.5);
                let r = ui.add(egui::Button::new(text).corner_radius(CornerRadius::same(14)).fill(alpha(pal.surface, 0.9)));
                if r.clicked() {
                    pick = Some(i);
                }
                r.context_menu(|ui| {
                    if ui.button("Forget this server").clicked() {
                        remove = Some(i);
                        ui.close();
                    }
                });
            }
        });
        if let Some(i) = pick {
            self.form = self.settings.servers[i].clone();
            self.port_text = self.form.port.to_string();
            self.connect();
        }
        if let Some(i) = remove {
            self.settings.servers.remove(i);
        }
    }
}

pub fn short_path(p: &std::path::Path) -> String {
    match devzat_core::paths::home_dir().and_then(|h| p.strip_prefix(&h).ok().map(|r| format!("~/{}", r.display()))) {
        Some(s) => s,
        None => p.display().to_string(),
    }
}
