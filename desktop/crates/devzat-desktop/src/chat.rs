use crate::app::{DevzatApp, ToastKind};
use crate::avatar;
use crate::model::{ConvKey, UiMessage};
use crate::rich::{self, RichCtx};
use crate::theme::{self, Palette, alpha, lerp};
use crate::widgets;
use chrono::{Datelike, Local};
use devzat_core::Status;
use devzat_core::protocol::MessageKind;
use eframe::egui::{
    self, Align, Align2, Color32, CornerRadius, FontId, Layout, Margin, RichText, Sense, Stroke, StrokeKind, vec2,
};

const GROUP_WINDOW_SECS: i64 = 6 * 60;

impl DevzatApp {
    pub fn chat_ui(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        let full_width = ui.available_width();
        egui::Panel::top("topbar")
            .exact_size(52.0)
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(Margin::symmetric(14, 8)).stroke(Stroke::new(1.0, pal.border)))
            .show(ui, |ui| self.top_bar(ui));

        let narrow = full_width < 860.0;
        egui::Panel::left("rooms")
            .resizable(true)
            .default_size(if narrow { 190.0 } else { 236.0 })
            .size_range(170.0..=340.0)
            .frame(egui::Frame::new().fill(pal.panel).inner_margin(Margin::symmetric(10, 12)))
            .show(ui, |ui| self.sidebar(ui));

        let roomy = full_width >= 980.0;
        if self.settings.show_members && roomy {
            egui::Panel::right("members")
                .resizable(true)
                .default_size(214.0)
                .size_range(170.0..=320.0)
                .frame(egui::Frame::new().fill(pal.panel).inner_margin(Margin::symmetric(10, 12)))
                .show(ui, |ui| self.members(ui));
        }

        egui::Panel::bottom("composer")
            .frame(egui::Frame::new().fill(pal.bg).inner_margin(Margin { left: 16, right: 16, top: 6, bottom: 14 }))
            .show(ui, |ui| self.composer_ui(ui));

        // With no member list, the timeline's scrollbar sits at the window edge; leave room for the resize grip.
        let right = if self.settings.show_members && roomy { 4 } else { 10 };
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(pal.bg).inner_margin(Margin { left: 6, right, top: 0, bottom: 0 }))
            .show(ui, |ui| self.timeline(ui));
    }

    fn top_bar(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        crate::chrome::drag_region(ui, ui.max_rect().expand2(vec2(14.0, 8.0)));
        ui.horizontal_centered(|ui| {
            ui.label(RichText::new("DZ").font(theme::pixel(15.0)).color(pal.accent));
            ui.add_space(4.0);
            if let Some(p) = &self.profile {
                ui.label(RichText::new(p.label()).color(pal.text_dim).size(12.5));
            }
            ui.add_space(12.0);
            let (icon, title, sub) = match &self.active {
                ConvKey::Room(r) => ("#", r.trim_start_matches('#').to_string(), format!("{} here", self.state.users.len())),
                ConvKey::Dm(p) => ("@", p.clone(), "direct message".to_string()),
            };
            ui.label(RichText::new(icon).font(theme::bold(20.0)).color(pal.accent2));
            ui.label(RichText::new(title).font(theme::bold(17.0)));
            ui.label(RichText::new(sub).color(pal.text_dim).size(12.5));

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                crate::chrome::window_buttons(ui, &pal);
                let (sep, _) = ui.allocate_exact_size(vec2(9.0, 24.0), Sense::hover());
                ui.painter().line_segment([sep.center_top(), sep.center_bottom()], Stroke::new(1.0, pal.border));
                if widgets::icon_button(ui, "⏏", "Leave server", &pal).clicked() {
                    self.disconnect();
                }
                if widgets::icon_button(ui, "⚙", "Settings", &pal).clicked() {
                    self.settings_open = true;
                }
                let members_tip = if self.settings.show_members { "Hide member list" } else { "Show member list" };
                if widgets::icon_button(ui, "👥", members_tip, &pal).clicked() {
                    self.settings.show_members = !self.settings.show_members;
                }
                if widgets::icon_button(ui, "🎨", "Next theme", &pal).clicked() {
                    let all = theme::ThemeId::ALL;
                    let i = all.iter().position(|t| *t == self.settings.theme).unwrap_or(0);
                    self.settings.theme = all[(i + 1) % all.len()];
                    self.toast(ToastKind::Info, format!("Theme: {}", self.settings.theme.palette().name));
                }
                ui.add_space(6.0);
                self.status_pill(ui);
            });
        });
    }

    fn status_pill(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        let (color, text, pulsing) = match &self.status {
            Status::Online => {
                let ms = match self.latency.map(|d| d.as_millis()) {
                    Some(0) => " · <1 ms".to_string(),
                    Some(ms) => format!(" · {ms} ms"),
                    None => String::new(),
                };
                (pal.good, format!("Secure{ms}"), false)
            }
            Status::Retrying { retry_in, .. } => (pal.warn, format!("Reconnecting in {}s", retry_in.as_secs()), true),
            Status::Connecting { .. } | Status::Authenticating | Status::Joining => (pal.warn, "Connecting".into(), true),
            Status::Offline { .. } | Status::Idle => (pal.bad, "Offline".into(), false),
        };
        let galley = ui.painter().layout_no_wrap(text, FontId::proportional(12.5), pal.text);
        let size = vec2(galley.size().x + 34.0, 26.0);
        let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
        ui.painter().rect_filled(rect, CornerRadius::same(13), alpha(color, 0.13));
        ui.painter().rect_stroke(rect, CornerRadius::same(13), Stroke::new(1.0, alpha(color, 0.45)), StrokeKind::Inside);
        widgets::status_dot(ui.painter(), rect.left_center() + vec2(14.0, 0.0), color, pulsing, self.time);
        ui.painter().galley(rect.left_center() + vec2(25.0, -galley.size().y / 2.0), galley, pal.text);
        if pulsing {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
        }
        if self.status == Status::Online {
            resp.on_hover_text("Connected over SSH. The server's identity was checked against your trusted keys.");
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        let footer_h = 64.0;
        let list_h = (ui.available_height() - footer_h).max(80.0);
        egui::ScrollArea::vertical().max_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                widgets::section_label(ui, "ROOMS", &pal);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if widgets::icon_button(ui, "+", "Create or join a room", &pal).clicked() {
                        self.new_room = Some(String::new());
                    }
                });
            });
            let mut rooms = self.state.rooms.clone();
            if rooms.is_empty() {
                rooms.push(devzat_core::protocol::RoomInfo { name: self.state.room.clone().max("#main".into()), count: 0 });
            }
            let mut join = None;
            for r in &rooms {
                let key = ConvKey::Room(r.name.clone());
                let selected = self.active == key;
                let here = r.name == self.state.room;
                let (unread, mentions) = self.convs.get(&key).map(|c| (c.unread, c.mentions)).unwrap_or((0, 0));
                if nav_row(ui, &pal, "#", r.name.trim_start_matches('#'), selected, here, r.count, unread, mentions, None).clicked() {
                    join = Some(r.name.clone());
                }
            }
            if let Some(r) = join {
                self.join_room(&r);
            }

            ui.add_space(14.0);
            widgets::section_label(ui, "DIRECT MESSAGES", &pal);
            let mut dms: Vec<(String, u32, u32, Option<chrono::DateTime<Local>>)> = self
                .convs
                .iter()
                .filter_map(|(k, c)| match k {
                    ConvKey::Dm(p) => Some((p.clone(), c.unread, c.mentions, c.last)),
                    _ => None,
                })
                .collect();
            dms.sort_by_key(|d| std::cmp::Reverse(d.3));
            if dms.is_empty() {
                ui.label(RichText::new("Click someone in the member list to message them privately.").color(pal.text_dim).size(12.0));
            }
            let mut open = None;
            for (peer, unread, mentions, _) in dms {
                let key = ConvKey::Dm(peer.clone());
                let online = self.state.users.iter().any(|u| u.plain == peer);
                if nav_row(ui, &pal, "@", &peer, self.active == key, online, 0, unread, mentions, Some(&peer)).clicked() {
                    open = Some(peer);
                }
            }
            if let Some(p) = open {
                self.open_dm(&p);
            }
        });

        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            egui::Frame::new().fill(pal.surface).corner_radius(CornerRadius::same(12)).inner_margin(Margin::same(8)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    let seed = self.welcome.as_ref().map(|w| w.id.clone()).unwrap_or_default();
                    let me = avatar::show(ui, 34.0, &seed, &pal);
                    if me.clicked() && self.settings.effects {
                        self.burst_at = Some(me.rect.center());
                    }
                    ui.vertical(|ui| {
                        let name = if self.state.name.is_empty() { self.my_plain() } else { self.state.name.clone() };
                        let job = rich::spans_job(&devzat_core::ansi::parse(&name), theme::bold(14.0), pal.text, &pal);
                        ui.label(job);
                        if let Some(w) = &self.welcome {
                            let id = w.id.clone();
                            let r = ui.add(
                                egui::Label::new(RichText::new(format!("ID {}…", &id[..id.len().min(10)])).monospace().size(11.0).color(pal.text_dim))
                                    .sense(Sense::click()),
                            );
                            if r.on_hover_text("Click to copy your devzat ID").clicked() {
                                ui.ctx().copy_text(id);
                                self.toast(ToastKind::Good, "Copied your devzat ID");
                            }
                        }
                    });
                });
            });
        });
    }

    fn members(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        widgets::section_label(ui, &format!("HERE — {}", self.state.users.len()), &pal);
        ui.add_space(4.0);
        let users = self.state.users.clone();
        let mut action: Option<(u8, String)> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            for u in &users {
                let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(rect, CornerRadius::same(8), pal.surface_hi);
                }
                let av = egui::Rect::from_min_size(rect.min + vec2(6.0, 5.0), vec2(24.0, 24.0));
                avatar::paint(ui.painter(), av, &u.plain, &pal);
                if u.idle {
                    ui.painter().rect_filled(av, CornerRadius::same(7), alpha(pal.panel, 0.55));
                }
                let job = rich::spans_job(&devzat_core::ansi::parse(&u.name), theme::bold(13.5), pal.text, &pal);
                let galley = ui.painter().layout_job(job);
                let text_pos = egui::pos2(av.right() + 9.0, rect.center().y - galley.size().y / 2.0);
                let color = if u.idle { pal.text_dim } else { pal.text };
                ui.painter().galley(text_pos, galley.clone(), color);
                let mut x = text_pos.x + galley.size().x + 6.0;
                if u.admin {
                    ui.painter().text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, "👑", FontId::proportional(12.0), pal.warn);
                    x += 18.0;
                }
                if u.you {
                    ui.painter().text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, "you", FontId::proportional(11.0), pal.text_dim);
                }
                let resp = resp.on_hover_text(if u.idle { "Away for a while" } else if u.you { "That's you" } else { "Click for options" });
                if !u.you {
                    resp.context_menu(|ui| member_menu(ui, &u.plain, &mut action));
                    if resp.clicked() {
                        action = Some((0, u.plain.clone()));
                    }
                }
            }
        });
        if let Some((kind, name)) = action {
            match kind {
                0 => self.open_dm(&name),
                1 => {
                    if !self.composer.text.is_empty() && !self.composer.text.ends_with(' ') {
                        self.composer.text.push(' ');
                    }
                    self.composer.text.push_str(&format!("@{name} "));
                    self.composer.want_focus = true;
                }
                2 => self.client.send(devzat_core::Command::Send(format!("bio {name}"))),
                _ => {}
            }
        }
    }

    fn timeline(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        self.banners(ui);

        let key = self.active.clone();
        let focused = ui.ctx().input(|i| i.viewport().focused.unwrap_or(true));
        if focused {
            self.clear_unread();
        }
        let show_joins = self.settings.show_joins;
        let clock24 = self.settings.clock_24h;
        let retro = self.settings.retro_images;
        let mut open_image = None;
        let mut copied = None;
        let mut quote = None;
        let mut dm = None;

        let Some(conv) = self.convs.get(&key) else {
            empty_state(ui, &pal, &key, self.time);
            return;
        };
        let visible: Vec<&UiMessage> = conv.messages.iter().filter(|m| show_joins || !m.join_or_leave).collect();
        if visible.is_empty() {
            empty_state(ui, &pal, &key, self.time);
            return;
        }

        // Jumping to the bottom is repeated for a few frames because pictures and wrapped text settle their height late.
        let stick = self.stick_frames > 0;
        self.stick_frames = self.stick_frames.saturating_sub(1);
        let area = egui::ScrollArea::vertical().auto_shrink([false, false]).stick_to_bottom(true);
        let images = &mut self.images;
        let client = &self.client;
        let heights = &mut self.row_heights;
        area.show_viewport(ui, |ui, viewport| {
            // Only rows near the visible part are laid out; the rest are stood in for by their last measured height.
            let width = ui.available_width().round();
            if heights.width != width {
                heights.width = width;
                heights.rows.clear();
            }
            let (top, bottom) = (viewport.min.y - 600.0, viewport.max.y + 600.0);
            ui.add_space(10.0);
            let mut y = 10.0;
            let mut skipped = 0.0;
            let mut prev: Option<&UiMessage> = None;
            for m in visible.iter().copied() {
                let new_day = prev.is_none_or(|p| p.ts.date_naive() != m.ts.date_naive());
                let grouped = !new_day
                    && prev.is_some_and(|p| {
                        p.kind != MessageKind::System
                            && m.kind != MessageKind::System
                            && p.author == m.author
                            && p.mine == m.mine
                            && (m.ts - p.ts).num_seconds() < GROUP_WINDOW_SECS
                    });
                prev = Some(m);
                let estimate = heights.rows.get(&(m.key, grouped, new_day)).copied().unwrap_or_else(|| estimate_height(m, grouped, new_day));
                if y + estimate < top || y > bottom {
                    skipped += estimate;
                    y += estimate;
                    continue;
                }
                if skipped > 0.0 {
                    ui.add_space(skipped);
                    skipped = 0.0;
                }
                let start = ui.cursor().top();
                if new_day {
                    day_divider(ui, &pal, m.ts);
                }
                let mut cx = RichCtx { pal: &pal, images, client, retro, size: 14.5, open_image: &mut open_image, copied: &mut copied };
                if m.kind == MessageKind::System {
                    system_line(ui, m, &pal, clock24, &mut cx);
                } else {
                    message_row(ui, m, grouped, &pal, clock24, &mut cx, &mut quote, &mut dm);
                }
                let actual = ui.cursor().top() - start;
                heights.rows.insert((m.key, grouped, new_day), actual);
                y += actual;
            }
            if skipped > 0.0 {
                ui.add_space(skipped);
            }
            ui.add_space(8.0);
            if stick {
                ui.scroll_to_cursor_animation(Some(Align::BOTTOM), egui::style::ScrollAnimation::none());
                ui.ctx().request_repaint();
            }
        });
        if open_image.is_some() {
            self.lightbox = open_image;
        }
        if let Some(text) = copied {
            self.copied = Some(text);
        }
        if let Some(text) = quote {
            let quoted: String = text.lines().map(|l| format!("> {l}\n")).collect();
            self.composer.text = format!("{quoted}{}", self.composer.text);
            self.composer.want_focus = true;
        }
        if let Some(peer) = dm {
            self.open_dm(&peer);
        }
    }

    fn banners(&mut self, ui: &mut egui::Ui) {
        let pal = self.pal();
        let mut retry = false;
        if let Status::Retrying { reason, retry_in, .. } = &self.status {
            let text = format!("{reason}. Reconnecting in {}s — messages you send now go out when it's back.", retry_in.as_secs());
            retry = banner(ui, &pal, pal.warn, &text, Some("Retry now"));
        }
        if retry {
            self.retry_now();
        }
        if !self.state.dm.is_empty() && matches!(self.active, ConvKey::Room(_)) {
            let text = format!("You're in devzat's DM mode with {} (from a cd command). Everything you type goes to them.", self.state.dm);
            if banner(ui, &pal, pal.accent2, &text, Some("Leave DM mode")) {
                self.client.send(devzat_core::Command::Send("cd ..".into()));
            }
        }
    }
}

/// Measured row heights for the timeline, keyed by message and how it was drawn.
#[derive(Default)]
pub struct RowHeights {
    width: f32,
    rows: std::collections::HashMap<(u64, bool, bool), f32>,
}

fn estimate_height(m: &UiMessage, grouped: bool, new_day: bool) -> f32 {
    let mut h = if m.kind == MessageKind::System { 28.0 } else if grouped { 26.0 } else { 52.0 };
    h += m.plain.len() as f32 / 90.0 * 20.0;
    h += m.doc.images().len() as f32 * 240.0;
    if new_day {
        h += 32.0;
    }
    h
}

fn member_menu(ui: &mut egui::Ui, name: &str, action: &mut Option<(u8, String)>) {
    if ui.button("✉  Message privately").clicked() {
        *action = Some((0, name.to_string()));
        ui.close();
    }
    if ui.button("@  Mention").clicked() {
        *action = Some((1, name.to_string()));
        ui.close();
    }
    if ui.button("ℹ  Show bio").clicked() {
        *action = Some((2, name.to_string()));
        ui.close();
    }
}

#[allow(clippy::too_many_arguments)]
fn nav_row(
    ui: &mut egui::Ui,
    pal: &Palette,
    glyph: &str,
    label: &str,
    selected: bool,
    lit: bool,
    count: usize,
    unread: u32,
    mentions: u32,
    avatar_seed: Option<&str>,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::click());
    let painter = ui.painter();
    if selected {
        painter.rect_filled(rect, CornerRadius::same(8), lerp(pal.surface_hi, pal.accent, 0.16));
        painter.rect_filled(egui::Rect::from_min_size(rect.min + vec2(0.0, 7.0), vec2(3.0, 18.0)), CornerRadius::same(2), pal.accent);
    } else if resp.hovered() {
        painter.rect_filled(rect, CornerRadius::same(8), pal.surface_hi);
    }
    let mut x = rect.left() + 12.0;
    if let Some(seed) = avatar_seed {
        let av = egui::Rect::from_min_size(egui::pos2(x - 2.0, rect.center().y - 10.0), vec2(20.0, 20.0));
        avatar::paint(painter, av, seed, pal);
        if lit {
            painter.circle_filled(av.right_bottom() - vec2(1.0, 1.0), 4.0, pal.good);
            painter.circle_stroke(av.right_bottom() - vec2(1.0, 1.0), 4.0, Stroke::new(1.5, pal.panel));
        }
        x += 26.0;
    } else {
        let gc = if selected || lit { pal.accent2 } else { pal.text_dim };
        painter.text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, glyph, theme::bold(15.0), gc);
        x += 16.0;
    }
    let strong = selected || unread > 0;
    let font = if strong { theme::bold(14.0) } else { FontId::proportional(14.0) };
    let color = if strong || lit { pal.text } else { pal.text_dim };
    painter.text(egui::pos2(x, rect.center().y), Align2::LEFT_CENTER, label, font, color);
    let right = rect.right_center() - vec2(8.0, 0.0);
    if mentions > 0 {
        widgets::count_badge(painter, right, mentions, pal.accent, pal.on_accent);
    } else if unread > 0 {
        widgets::count_badge(painter, right, unread, pal.surface_hi, pal.text);
    } else if count > 0 {
        painter.text(right, Align2::RIGHT_CENTER, count.to_string(), FontId::proportional(12.0), pal.text_dim);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn banner(ui: &mut egui::Ui, pal: &Palette, color: Color32, text: &str, button: Option<&str>) -> bool {
    let mut clicked = false;
    egui::Frame::new()
        .fill(alpha(color, 0.12))
        .stroke(Stroke::new(1.0, alpha(color, 0.45)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(12, 8))
        .outer_margin(Margin { left: 8, right: 8, top: 8, bottom: 2 })
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let button_w = if button.is_some() { 120.0 } else { 0.0 };
                ui.scope(|ui| {
                    ui.set_max_width(ui.available_width() - button_w);
                    ui.add(egui::Label::new(RichText::new(text).color(pal.text)).wrap());
                });
                if let Some(b) = button {
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        clicked = ui.button(b).clicked();
                    });
                }
            });
        });
    clicked
}

fn fmt_time(ts: chrono::DateTime<Local>, clock24: bool) -> String {
    if clock24 { ts.format("%H:%M").to_string() } else { ts.format("%-I:%M %p").to_string() }
}

fn day_divider(ui: &mut egui::Ui, pal: &Palette, ts: chrono::DateTime<Local>) {
    let today = Local::now().date_naive();
    let d = ts.date_naive();
    let label = if d == today {
        "Today".to_string()
    } else if Some(d) == today.pred_opt() {
        "Yesterday".to_string()
    } else if d.year() == today.year() {
        ts.format("%A, %B %-d").to_string()
    } else {
        ts.format("%B %-d, %Y").to_string()
    };
    ui.add_space(6.0);
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let galley = ui.painter().layout_no_wrap(label, theme::bold(11.5), pal.text_dim);
    let c = rect.center();
    let half = galley.size().x / 2.0 + 12.0;
    ui.painter().line_segment([egui::pos2(rect.left() + 12.0, c.y), egui::pos2(c.x - half, c.y)], Stroke::new(1.0, pal.border));
    ui.painter().line_segment([egui::pos2(c.x + half, c.y), egui::pos2(rect.right() - 12.0, c.y)], Stroke::new(1.0, pal.border));
    ui.painter().galley(c - galley.size() / 2.0, galley, pal.text_dim);
    ui.add_space(4.0);
}

fn system_line(ui: &mut egui::Ui, m: &UiMessage, pal: &Palette, clock24: bool, cx: &mut RichCtx) {
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        let (glyph, color) = if m.plain.starts_with("-->") || m.plain.contains("has joined") {
            ("→", pal.good)
        } else if m.plain.starts_with("<--") || m.plain.contains("has left") {
            ("←", pal.bad)
        } else {
            ("•", pal.text_dim)
        };
        ui.add_sized(vec2(28.0, 20.0), egui::Label::new(RichText::new(glyph).color(color).font(theme::bold(14.0))));
        ui.scope(|ui| {
            ui.set_max_width(ui.available_width() - 70.0);
            let mut doc = m.doc.clone();
            strip_arrow(&mut doc);
            let mut small = RichCtx { pal: cx.pal, images: cx.images, client: cx.client, retro: cx.retro, size: 13.0, open_image: cx.open_image, copied: cx.copied };
            ui.vertical(|ui| rich::doc(ui, &doc, &mut small));
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(10.0);
            ui.label(RichText::new(fmt_time(m.ts, clock24)).size(11.0).color(alpha(pal.text_dim, 0.8)));
        });
    });
}

/// Join and leave lines start with a drawn arrow ("-->"); the timeline shows its own glyph instead.
fn strip_arrow(doc: &mut devzat_core::markup::Doc) {
    if let Some(devzat_core::markup::Block::Para(inl)) = doc.blocks.first_mut()
        && let Some(first) = inl.first_mut() {
            let t = first.text.trim_start();
            if let Some(rest) = t.strip_prefix("-->").or_else(|| t.strip_prefix("<--")) {
                first.text = rest.trim_start().to_string();
            }
            if first.text.is_empty() {
                inl.remove(0);
            }
        }
}

#[allow(clippy::too_many_arguments)]
fn message_row(
    ui: &mut egui::Ui,
    m: &UiMessage,
    grouped: bool,
    pal: &Palette,
    clock24: bool,
    cx: &mut RichCtx,
    quote: &mut Option<String>,
    dm: &mut Option<String>,
) {
    let avatar_w = 40.0;
    let top_pad = if grouped { 1.0 } else { 8.0 };
    let full = ui.available_rect_before_wrap();
    let start = ui.cursor().min;
    ui.add_space(top_pad);
    let inner = ui.horizontal_top(|ui| {
        ui.add_space(12.0);
        if grouped {
            let (r, _) = ui.allocate_exact_size(vec2(avatar_w, 18.0), Sense::hover());
            if ui.rect_contains_pointer(full.intersect(egui::Rect::from_min_size(start, vec2(full.width(), 22.0)))) {
                ui.painter().text(r.center(), Align2::CENTER_CENTER, fmt_time(m.ts, true), FontId::proportional(10.0), pal.text_dim);
            }
        } else {
            let seed = if m.kind == MessageKind::Bot { "devbot" } else { m.author.as_str() };
            let r = avatar::show(ui, avatar_w, seed, pal);
            if !m.mine && m.kind != MessageKind::Bot && r.on_hover_text(format!("Message {} privately", m.author)).clicked() {
                *dm = Some(m.author.clone());
            }
        }
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.set_max_width(ui.available_width() - 64.0);
            if !grouped {
                ui.horizontal(|ui| {
                    let job = rich::spans_job(&m.author_spans, theme::bold(14.5), pal.text, pal);
                    ui.label(job);
                    match m.kind {
                        MessageKind::Bot => widgets::tag(ui, "BOT", pal.accent2),
                        MessageKind::DmIn | MessageKind::DmOut => widgets::tag(ui, "PRIVATE", pal.accent),
                        _ => {}
                    }
                    ui.label(RichText::new(fmt_time(m.ts, clock24)).size(11.5).color(pal.text_dim));
                });
            }
            rich::doc(ui, &m.doc, cx);
        });
    });
    let rect = egui::Rect::from_min_max(egui::pos2(full.left(), start.y), egui::pos2(full.right(), inner.response.rect.bottom() + 2.0));
    let hovered = ui.rect_contains_pointer(rect);
    if m.mention {
        let p = ui.painter();
        p.rect_filled(rect, CornerRadius::same(6), alpha(pal.accent, 0.08));
        p.rect_filled(egui::Rect::from_min_size(rect.min, vec2(3.0, rect.height())), CornerRadius::same(2), pal.accent);
    } else if hovered {
        ui.painter().rect_filled(rect, CornerRadius::same(6), alpha(pal.surface_hi, 0.35));
    }
    if hovered {
        let bar = egui::Rect::from_min_size(egui::pos2(rect.right() - 74.0, rect.top() + 2.0), vec2(66.0, 24.0));
        let p = ui.painter();
        p.rect_filled(bar, CornerRadius::same(7), pal.surface);
        p.rect_stroke(bar, CornerRadius::same(7), Stroke::new(1.0, pal.border), StrokeKind::Inside);
        let copy = egui::Rect::from_min_size(bar.min, vec2(33.0, 24.0));
        let reply = egui::Rect::from_min_size(bar.min + vec2(33.0, 0.0), vec2(33.0, 24.0));
        let id = ui.scope_id().with(("msg-actions", m.key));
        let c = ui.interact(copy, id.with("copy"), Sense::click()).on_hover_text("Copy text");
        let q = ui.interact(reply, id.with("quote"), Sense::click()).on_hover_text("Quote in reply");
        for (r, g, resp) in [(copy, "📋", &c), (reply, "↩", &q)] {
            if resp.hovered() {
                ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(5), pal.surface_hi);
            }
            ui.painter().text(r.center(), Align2::CENTER_CENTER, g, FontId::proportional(14.0), pal.text_dim);
        }
        if c.clicked() {
            ui.ctx().copy_text(m.plain.clone());
            *cx.copied = Some("Message copied".into());
        }
        if q.clicked() {
            *quote = Some(m.plain.clone());
        }
    }
}

fn empty_state(ui: &mut egui::Ui, pal: &Palette, key: &ConvKey, t: f32) {
    ui.centered_and_justified(|ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(ui.available_height() * 0.28);
            let bob = (t * 2.0).sin() * 4.0;
            ui.add_space(bob.max(0.0));
            ui.label(RichText::new(match key {
                ConvKey::Room(_) => "( ˘▽˘)っ♨",
                ConvKey::Dm(_) => "✉",
            })
            .size(38.0)
            .color(pal.accent2));
            ui.add_space(10.0);
            let line = match key {
                ConvKey::Room(r) => format!("It's quiet in {r}. Say hi!"),
                ConvKey::Dm(p) => format!("This is the start of your private chat with {p}."),
            };
            ui.label(RichText::new(line).font(theme::bold(17.0)));
            ui.add_space(4.0);
            ui.label(RichText::new("Type / to see commands. Paste or drop a picture to share it.").color(pal.text_dim));
        });
    });
    ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
}
