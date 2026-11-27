use crate::theme::{self, Palette, alpha, lerp};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Painter, Pos2, Rect, Response, Sense, Shape, Stroke, StrokeKind, Ui, Vec2,
    pos2, vec2,
};
use eframe::epaint::Mesh;

/// A rounded rectangle filled with a left-to-right gradient.
pub fn gradient_rect(painter: &Painter, rect: Rect, radius: f32, left: Color32, right: Color32) {
    let r = radius.min(rect.height() / 2.0).min(rect.width() / 2.0);
    let mut pts: Vec<Pos2> = Vec::new();
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -90.0f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ];
    for (c, start) in corners {
        for i in 0..=6 {
            let a = (start + i as f32 * 15.0).to_radians();
            pts.push(c + vec2(a.cos(), a.sin()) * r);
        }
    }
    let mut mesh = Mesh::default();
    let color_at = |x: f32| lerp(left, right, (x - rect.left()) / rect.width().max(1.0));
    mesh.colored_vertex(rect.center(), color_at(rect.center().x));
    for p in &pts {
        mesh.colored_vertex(*p, color_at(p.x));
    }
    let n = pts.len() as u32;
    for i in 0..n {
        mesh.add_triangle(0, 1 + i, 1 + (i + 1) % n);
    }
    painter.add(Shape::mesh(mesh));
}

/// The big call-to-action button with a gradient and a glow on hover.
pub fn gradient_button(ui: &mut Ui, text: &str, size: Vec2, pal: &Palette, enabled: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, if enabled { Sense::click() } else { Sense::hover() });
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let hover = enabled && resp.hovered();
        let lift = if resp.is_pointer_button_down_on() { 1.0 } else if hover { -1.0 } else { 0.0 };
        let rect = rect.translate(vec2(0.0, lift));
        if hover {
            for i in 1..=4 {
                p.rect_filled(rect.expand(i as f32 * 2.5), CornerRadius::same(255), alpha(pal.accent, 0.07));
            }
        }
        let (a, b) = if enabled { (pal.accent, pal.accent2) } else { (pal.surface_hi, pal.surface_hi) };
        gradient_rect(p, rect, rect.height() / 2.0, a, b);
        let text_color = if enabled { pal.on_accent } else { pal.text_dim };
        p.text(rect.center(), Align2::CENTER_CENTER, text, theme::bold(15.0), text_color);
    }
    if enabled {
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    } else {
        resp
    }
}

/// A small frameless button with a hover background, for toolbar glyphs.
pub fn icon_button(ui: &mut Ui, glyph: &str, tip: &str, pal: &Palette) -> Response {
    let size = vec2(30.0, 30.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(rect, CornerRadius::same(8), pal.surface_hi);
        }
        let color = if resp.hovered() { pal.text } else { pal.text_dim };
        p.text(rect.center(), Align2::CENTER_CENTER, glyph, FontId::proportional(16.0), color);
    }
    resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand)
}

pub fn section_label(ui: &mut Ui, text: &str, pal: &Palette) {
    let spaced: String = text.chars().flat_map(|c| [c, '\u{2009}']).collect();
    ui.label(egui::RichText::new(spaced).font(theme::bold(10.5)).color(pal.text_dim));
}

pub fn count_badge(painter: &Painter, right_center: Pos2, n: u32, color: Color32, text: Color32) -> Rect {
    let label = if n > 99 { "99+".to_string() } else { n.to_string() };
    let w = 10.0 + label.len() as f32 * 7.0;
    let rect = Rect::from_center_size(right_center - vec2(w / 2.0, 0.0), vec2(w, 18.0));
    painter.rect_filled(rect, CornerRadius::same(9), color);
    painter.text(rect.center(), Align2::CENTER_CENTER, label, theme::bold(11.0), text);
    rect
}

pub fn tag(ui: &mut Ui, text: &str, color: Color32) {
    let galley = ui.painter().layout_no_wrap(text.to_string(), theme::bold(9.5), color);
    let size = galley.size() + vec2(10.0, 4.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(4), alpha(color, 0.16));
    ui.painter().rect_stroke(rect, CornerRadius::same(4), Stroke::new(1.0, alpha(color, 0.5)), StrokeKind::Inside);
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, color);
}

/// A status dot that breathes while something is in progress.
pub fn status_dot(painter: &Painter, center: Pos2, color: Color32, pulsing: bool, t: f32) {
    if pulsing {
        let k = (t * 2.2).fract();
        painter.circle_filled(center, 4.0 + k * 7.0, alpha(color, 0.35 * (1.0 - k)));
    }
    painter.circle_filled(center, 4.0, color);
}

/// Draws the DEVZAT wordmark in the pixel font, one hue-shifted letter at a time.
pub fn wordmark(ui: &mut Ui, text: &str, size: f32, t: f32, pal: &Palette) -> Response {
    let font = theme::pixel(size);
    let letters: Vec<String> = text.chars().map(String::from).collect();
    let galleys: Vec<_> = letters.iter().map(|l| ui.painter().layout_no_wrap(l.clone(), font.clone(), Color32::WHITE)).collect();
    let spacing = size * 0.18;
    let width: f32 = galleys.iter().map(|g| g.size().x).sum::<f32>() + spacing * (galleys.len().saturating_sub(1)) as f32;
    let (rect, resp) = ui.allocate_exact_size(vec2(width, size * 1.4), Sense::hover());
    let mut x = rect.left();
    for (i, g) in galleys.into_iter().enumerate() {
        let phase = t * 1.6 - i as f32 * 0.45;
        let bob = phase.sin() * size * 0.06;
        let k = (phase.sin() * 0.5 + 0.5).clamp(0.0, 1.0);
        let color = lerp(pal.accent, pal.accent2, k);
        let pos = pos2(x, rect.top() + size * 0.15 + bob);
        let shadow = if pal.dark { Color32::from_black_alpha(170) } else { alpha(pal.accent, 0.25) };
        ui.painter().galley_with_override_text_color(pos + vec2(size * 0.09, size * 0.09), g.clone(), shadow);
        ui.painter().galley_with_override_text_color(pos, g.clone(), color);
        x += g.size().x + spacing;
    }
    resp
}
