//! Window chrome for the borderless window: a draggable title area, our own minimize, maximize, and close
//! buttons, generous invisible resize grips along every edge, and a hairline outline.

use crate::theme::{Palette, alpha};
use eframe::egui::{
    self, Align, Color32, CornerRadius, CursorIcon, Id, LayerId, Layout, Order, PointerButton, Rect, ResizeDirection,
    Sense, Stroke, StrokeKind, Ui, UiBuilder, Vec2, ViewportCommand, pos2, vec2,
};

/// Height of the title strip on screens without a toolbar.
pub const TITLE_H: f32 = 38.0;
/// How far in from each edge the resize grip reaches.
const EDGE: f32 = 8.0;
/// Corner grips are bigger so diagonal resizing is easy to hit.
const CORNER: f32 = 22.0;

pub fn is_maximized(ctx: &egui::Context) -> bool {
    ctx.input(|i| i.viewport().maximized.unwrap_or(false) || i.viewport().fullscreen.unwrap_or(false))
}

fn grab_flag() -> Id {
    Id::unique("devzat-wm-grab")
}

/// Hands the pointer to the window manager for a move or resize. The window manager keeps the
/// button release for itself, so `release_after_grab` supplies one on the next frame.
fn hand_to_window_manager(ctx: &egui::Context, cmd: ViewportCommand) {
    ctx.send_viewport_cmd(cmd);
    ctx.data_mut(|d| d.insert_temp(grab_flag(), true));
}

/// Call from `raw_input_hook`: without this, egui thinks the button is still down after a move or
/// resize and swallows the next click.
pub fn release_after_grab(ctx: &egui::Context, raw: &mut egui::RawInput) {
    if ctx.data_mut(|d| d.remove_temp::<bool>(grab_flag())).unwrap_or(false) {
        let (pos, modifiers) = ctx.input(|i| (i.pointer.latest_pos().unwrap_or_default(), i.modifiers));
        raw.events.insert(0, egui::Event::PointerButton { pos, button: PointerButton::Primary, pressed: false, modifiers });
    }
}

pub fn toggle_maximized(ctx: &egui::Context) {
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
}

/// Makes `rect` act like a title bar: drag it to move the window, double-click to maximize.
/// Register it before the widgets on top of it so those keep their clicks.
pub fn drag_region(ui: &mut Ui, rect: Rect) {
    let resp = ui.interact(rect, Id::unique("devzat-title-drag"), Sense::click_and_drag());
    if resp.drag_started_by(PointerButton::Primary) {
        hand_to_window_manager(ui.ctx(), ViewportCommand::StartDrag);
    }
    if resp.double_clicked() {
        toggle_maximized(ui.ctx());
    }
}

/// Minimize, maximize/restore, and close, laid out right to left.
pub fn window_buttons(ui: &mut Ui, pal: &Palette) {
    let ctx = ui.ctx().clone();
    let size = vec2(42.0, 30.0);
    let maximized = is_maximized(&ctx);

    let (rect, close) = ui.allocate_exact_size(size, Sense::click());
    paint_button(ui, rect, close.hovered(), Color32::from_rgb(0xe8, 0x11, 0x23));
    let c = rect.center();
    let fg = if close.hovered() { Color32::WHITE } else { pal.text_dim };
    let s = 5.0;
    ui.painter().line_segment([c + vec2(-s, -s), c + vec2(s, s)], Stroke::new(1.4, fg));
    ui.painter().line_segment([c + vec2(-s, s), c + vec2(s, -s)], Stroke::new(1.4, fg));
    if close.on_hover_text("Close").clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Close);
    }

    let (rect, max) = ui.allocate_exact_size(size, Sense::click());
    paint_button(ui, rect, max.hovered(), pal.surface_hi);
    let fg = if max.hovered() { pal.text } else { pal.text_dim };
    let c = rect.center();
    if maximized {
        let back = Rect::from_center_size(c + vec2(1.5, -1.5), Vec2::splat(8.0));
        let front = Rect::from_center_size(c + vec2(-1.5, 1.5), Vec2::splat(8.0));
        ui.painter().rect_stroke(back, CornerRadius::same(1), Stroke::new(1.2, fg), StrokeKind::Inside);
        ui.painter().rect_filled(front, CornerRadius::same(1), button_fill(max.hovered(), pal));
        ui.painter().rect_stroke(front, CornerRadius::same(1), Stroke::new(1.2, fg), StrokeKind::Inside);
    } else {
        ui.painter().rect_stroke(Rect::from_center_size(c, Vec2::splat(10.0)), CornerRadius::same(1), Stroke::new(1.2, fg), StrokeKind::Inside);
    }
    if max.on_hover_text(if maximized { "Restore" } else { "Maximize" }).clicked() {
        toggle_maximized(&ctx);
    }

    let (rect, min) = ui.allocate_exact_size(size, Sense::click());
    paint_button(ui, rect, min.hovered(), pal.surface_hi);
    let fg = if min.hovered() { pal.text } else { pal.text_dim };
    let c = rect.center();
    ui.painter().line_segment([c + vec2(-5.0, 0.5), c + vec2(5.0, 0.5)], Stroke::new(1.4, fg));
    if min.on_hover_text("Minimize").clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Minimized(true));
    }
}

fn button_fill(hovered: bool, pal: &Palette) -> Color32 {
    if hovered { pal.surface_hi } else { pal.panel }
}

fn paint_button(ui: &Ui, rect: Rect, hovered: bool, hover_fill: Color32) {
    if hovered {
        ui.painter().rect_filled(rect, CornerRadius::same(6), hover_fill);
    }
}

/// The title strip for screens that have no toolbar of their own (the launch screen).
pub fn title_strip(ui: &mut Ui, rect: Rect, pal: &Palette) {
    drag_region(ui, rect);
    ui.scope_builder(UiBuilder::new().max_rect(rect.shrink2(vec2(10.0, 4.0))).layout(Layout::right_to_left(Align::Center)), |ui| {
        window_buttons(ui, pal);
    });
}

/// Resize grips on every edge and corner, plus a hairline outline. Call once per frame, after everything else.
pub fn window_frame(ctx: &egui::Context, pal: &Palette) {
    if is_maximized(ctx) {
        return;
    }
    let r = ctx.content_rect();
    let border = ctx.layer_painter(LayerId::new(Order::Tooltip, Id::unique("devzat-window-outline")));
    border.rect_stroke(r, CornerRadius::ZERO, Stroke::new(1.0, alpha(pal.accent, 0.45)), StrokeKind::Inside);

    let zones = [
        (ResizeDirection::NorthWest, Rect::from_min_size(r.left_top(), Vec2::splat(CORNER)), CursorIcon::ResizeNorthWest),
        (ResizeDirection::NorthEast, Rect::from_min_size(pos2(r.right() - CORNER, r.top()), Vec2::splat(CORNER)), CursorIcon::ResizeNorthEast),
        (ResizeDirection::SouthWest, Rect::from_min_size(pos2(r.left(), r.bottom() - CORNER), Vec2::splat(CORNER)), CursorIcon::ResizeSouthWest),
        (ResizeDirection::SouthEast, Rect::from_min_size(r.right_bottom() - Vec2::splat(CORNER), Vec2::splat(CORNER)), CursorIcon::ResizeSouthEast),
        (ResizeDirection::North, Rect::from_min_max(pos2(r.left() + CORNER, r.top()), pos2(r.right() - CORNER, r.top() + EDGE)), CursorIcon::ResizeNorth),
        (ResizeDirection::South, Rect::from_min_max(pos2(r.left() + CORNER, r.bottom() - EDGE), pos2(r.right() - CORNER, r.bottom())), CursorIcon::ResizeSouth),
        (ResizeDirection::West, Rect::from_min_max(pos2(r.left(), r.top() + CORNER), pos2(r.left() + EDGE, r.bottom() - CORNER)), CursorIcon::ResizeWest),
        (ResizeDirection::East, Rect::from_min_max(pos2(r.right() - EDGE, r.top() + CORNER), pos2(r.right(), r.bottom() - CORNER)), CursorIcon::ResizeEast),
    ];
    for (i, (direction, zone, cursor)) in zones.into_iter().enumerate() {
        egui::Area::new(Id::unique(("devzat-resize", i)))
            .order(Order::Tooltip)
            .fixed_pos(zone.min)
            .movable(false)
            .constrain(false)
            .show(ctx, |ui| {
                let (_, resp) = ui.allocate_exact_size(zone.size(), Sense::drag());
                if resp.hovered() || resp.dragged() {
                    ctx.set_cursor_icon(cursor);
                }
                if resp.drag_started_by(PointerButton::Primary) {
                    hand_to_window_manager(ctx, ViewportCommand::BeginResize(direction));
                }
            });
    }
}
