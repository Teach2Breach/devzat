//! Lays out a parsed devzat message: colored text, links, code, quotes, lists, tables, and pictures.

use crate::images::{ImgState, Images};
use crate::theme::{self, Palette, alpha, readable};
use devzat_core::ClientHandle;
use devzat_core::ansi::AnsiSpan;
use devzat_core::markup::{Block, Doc, Inline, Style};
use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Rect, RichText, Sense, Stroke, StrokeKind, TextFormat, Ui, Vec2, pos2,
    text::LayoutJob, vec2,
};
use std::ops::Range;

pub struct RichCtx<'a> {
    pub pal: &'a Palette,
    pub images: &'a mut Images,
    pub client: &'a ClientHandle,
    pub retro: bool,
    pub size: f32,
    pub open_image: &'a mut Option<String>,
    pub copied: &'a mut Option<String>,
}

pub fn ansi_color(c: devzat_core::ansi::Rgb, pal: &Palette) -> Color32 {
    readable(Color32::from_rgb(c.0, c.1, c.2), pal)
}

fn format_for(style: &Style, cx: &RichCtx, scale: f32) -> TextFormat {
    let size = cx.size * scale;
    let font_id = if style.code {
        FontId::monospace(size * 0.92)
    } else if style.bold || style.ansi.bold {
        theme::bold(size)
    } else {
        FontId::proportional(size)
    };
    let mut color = style.ansi.fg.map(|c| ansi_color(c, cx.pal)).unwrap_or(cx.pal.text);
    if style.ansi.dim {
        color = color.gamma_multiply(0.65);
    }
    let mut fmt = TextFormat { font_id, color, ..Default::default() };
    if let Some(bg) = style.ansi.bg {
        fmt.background = Color32::from_rgb(bg.0, bg.1, bg.2);
    } else if style.code {
        fmt.background = cx.pal.surface_hi;
        if style.ansi.fg.is_none() {
            fmt.color = theme::lerp(cx.pal.text, cx.pal.accent2, 0.45);
        }
    }
    if style.ansi.inverse {
        let fg = fmt.color;
        fmt.color = if fmt.background == Color32::TRANSPARENT { cx.pal.bg } else { fmt.background };
        fmt.background = fg;
    }
    if style.link.is_some() {
        fmt.color = cx.pal.link;
        fmt.underline = Stroke::new(1.0, alpha(cx.pal.link, 0.55));
    }
    fmt.italics = style.italic || style.ansi.italic;
    if style.ansi.underline {
        fmt.underline = Stroke::new(1.0, fmt.color);
    }
    if style.strike || style.ansi.strike {
        fmt.strikethrough = Stroke::new(1.0, fmt.color);
    }
    fmt
}

/// Builds the layout job for a colored name, or anything else that's only ANSI spans.
pub fn spans_job(spans: &[AnsiSpan], font: FontId, fallback: Color32, pal: &Palette) -> LayoutJob {
    let mut job = LayoutJob::default();
    for s in spans {
        let color = s.style.fg.map(|c| ansi_color(c, pal)).unwrap_or(fallback);
        let mut fmt = TextFormat { font_id: font.clone(), color, ..Default::default() };
        if let Some(bg) = s.style.bg {
            fmt.background = Color32::from_rgb(bg.0, bg.1, bg.2);
        }
        job.append(&s.text, 0.0, fmt);
    }
    if job.sections.is_empty() {
        job.append(" ", 0.0, TextFormat { font_id: font, color: fallback, ..Default::default() });
    }
    job
}

pub fn doc(ui: &mut Ui, doc: &Doc, cx: &mut RichCtx) {
    blocks(ui, &doc.blocks, cx);
}

fn blocks(ui: &mut Ui, list: &[Block], cx: &mut RichCtx) {
    for b in list {
        match b {
            Block::Para(inlines) => paragraph(ui, inlines, cx, 1.0),
            Block::Heading { level, inlines } => {
                let scale = match level {
                    1 => 1.55,
                    2 => 1.35,
                    3 => 1.18,
                    _ => 1.06,
                };
                paragraph(ui, inlines, cx, scale);
            }
            Block::Code { lang, inlines } => code_block(ui, lang, inlines, cx),
            Block::Quote(inner) => {
                ui.horizontal_top(|ui| {
                    let (bar, _) = ui.allocate_exact_size(vec2(3.0, 18.0), Sense::hover());
                    let top = bar.top();
                    ui.vertical(|ui| blocks(ui, inner, cx));
                    let bottom = ui.min_rect().bottom();
                    ui.painter().line_segment(
                        [pos2(bar.center().x, top), pos2(bar.center().x, bottom)],
                        Stroke::new(3.0, alpha(cx.pal.accent, 0.7)),
                    );
                });
            }
            Block::List { start, items } => {
                for (i, item) in items.iter().enumerate() {
                    ui.horizontal_top(|ui| {
                        let marker = match start {
                            Some(n) => format!("{}.", n + i as u64),
                            None => "•".into(),
                        };
                        ui.add_sized(vec2(18.0, cx.size * 1.3), egui::Label::new(egui::RichText::new(marker).color(cx.pal.accent)));
                        ui.vertical(|ui| blocks(ui, item, cx));
                    });
                }
            }
            Block::Image { alt, src, hq } => image(ui, alt, src, *hq, cx),
            Block::Table { rows, header_rows } => table(ui, rows, *header_rows, cx),
            Block::Rule => {
                ui.add_space(4.0);
                let w = ui.available_width();
                let (r, _) = ui.allocate_exact_size(vec2(w, 1.0), Sense::hover());
                ui.painter().rect_filled(r, 0.0, cx.pal.border);
                ui.add_space(4.0);
            }
        }
    }
}

fn link_at(galley: &egui::Galley, local: Vec2, links: &[(Range<usize>, String)]) -> Option<String> {
    if links.is_empty() || !galley.rect.expand(2.0).contains(local.to_pos2()) {
        return None;
    }
    let idx = galley.cursor_from_pos(local).index.0;
    links.iter().find(|(r, _)| r.contains(&idx)).map(|(_, u)| u.clone())
}

fn paragraph(ui: &mut Ui, inlines: &[Inline], cx: &mut RichCtx, scale: f32) {
    let mut job = LayoutJob::default();
    job.wrap.max_width = ui.available_width();
    let mut links: Vec<(Range<usize>, String)> = Vec::new();
    let mut chars = 0;
    for inl in inlines {
        let fmt = format_for(&inl.style, cx, scale);
        job.append(&inl.text, 0.0, fmt);
        let n = inl.text.chars().count();
        if let Some(url) = &inl.style.link {
            links.push((chars..chars + n, url.clone()));
        }
        chars += n;
    }
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let resp = ui.add(egui::Label::new(galley.clone()).sense(Sense::click()));
    if links.is_empty() {
        return;
    }
    let hovered = resp.hover_pos().and_then(|p| link_at(&galley, p - resp.rect.min, &links));
    if let Some(url) = &hovered {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        resp.clone().on_hover_text(url);
    }
    if resp.clicked()
        && let Some(url) = resp.interact_pointer_pos().and_then(|p| link_at(&galley, p - resp.rect.min, &links)) {
            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
        }
}

fn code_block(ui: &mut Ui, lang: &str, inlines: &[Inline], cx: &mut RichCtx) {
    let frame = egui::Frame::new()
        .fill(cx.pal.code_bg)
        .stroke(Stroke::new(1.0, cx.pal.border))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(12, 10));
    let resp = frame
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            let mut job = LayoutJob::default();
            for inl in inlines {
                let mut style = inl.style.clone();
                style.code = true;
                let mut fmt = format_for(&style, cx, 1.0);
                if inl.style.ansi.bg.is_none() {
                    fmt.background = Color32::TRANSPARENT;
                }
                if inl.style.ansi.fg.is_none() {
                    fmt.color = theme::lerp(cx.pal.text, cx.pal.accent2, 0.15);
                }
                job.append(&inl.text, 0.0, fmt);
            }
            job.wrap.max_width = f32::INFINITY;
            egui::ScrollArea::horizontal().id_salt(ui.next_auto_id()).auto_shrink([false, true]).show(ui, |ui| {
                ui.add(egui::Label::new(job).extend());
            });
        })
        .response;
    let hovered = ui.rect_contains_pointer(resp.rect);
    if !lang.is_empty() || hovered {
        let painter = ui.painter_at(resp.rect);
        let mut x = resp.rect.right() - 8.0;
        if hovered {
            let r = Rect::from_min_size(pos2(x - 46.0, resp.rect.top() + 6.0), vec2(46.0, 20.0));
            let click = ui.interact(r, ui.scope_id().with(("copy", resp.rect.min.x as i32, resp.rect.min.y as i32)), Sense::click());
            painter.rect_filled(r, CornerRadius::same(5), if click.hovered() { cx.pal.surface_hi } else { cx.pal.surface });
            painter.text(r.center(), Align2::CENTER_CENTER, "copy", FontId::proportional(11.0), cx.pal.text_dim);
            if click.clicked() {
                let text: String = inlines.iter().map(|i| i.text.as_str()).collect();
                ui.ctx().copy_text(text);
                *cx.copied = Some("Code copied".into());
            }
            x -= 52.0;
        }
        if !lang.is_empty() {
            painter.text(pos2(x, resp.rect.top() + 16.0), Align2::RIGHT_CENTER, lang, FontId::monospace(10.5), cx.pal.text_dim);
        }
    }
}

fn table(ui: &mut Ui, rows: &[Vec<Vec<Inline>>], header_rows: usize, cx: &mut RichCtx) {
    egui::Frame::new()
        .stroke(Stroke::new(1.0, cx.pal.border))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(egui::Margin::same(6))
        .show(ui, |ui| {
            egui::Grid::new(ui.next_auto_id()).striped(true).spacing(vec2(16.0, 4.0)).show(ui, |ui| {
                for (r, row) in rows.iter().enumerate() {
                    for cell in row {
                        let mut job = LayoutJob::default();
                        for inl in cell {
                            let mut style = inl.style.clone();
                            style.bold |= r < header_rows;
                            job.append(&inl.text, 0.0, format_for(&style, cx, 0.95));
                        }
                        ui.label(job);
                    }
                    ui.end_row();
                }
            });
        });
}

fn image(ui: &mut Ui, alt: &str, src: &str, hq: bool, cx: &mut RichCtx) {
    let pal = cx.pal;
    let state = cx.images.get(src, cx.client);
    match state {
        ImgState::Loading(started) => {
            let (rect, _) = ui.allocate_exact_size(vec2(260.0, 150.0), Sense::hover());
            let t = started.elapsed().as_secs_f32();
            ui.painter().rect_filled(rect, CornerRadius::same(10), pal.surface_hi);
            let band_x = rect.left() + ((t * 0.8).fract() * 1.6 - 0.3) * rect.width();
            let band = Rect::from_min_max(pos2(band_x.max(rect.left()), rect.top()), pos2((band_x + 60.0).min(rect.right()), rect.bottom()));
            if band.width() > 0.0 {
                ui.painter().rect_filled(band, 0.0, alpha(pal.text, 0.05));
            }
            ui.painter().text(rect.center(), Align2::CENTER_CENTER, "loading picture…", FontId::proportional(12.5), pal.text_dim);
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
        }
        ImgState::Failed(err) => {
            let label = if alt.is_empty() { "picture".to_string() } else { alt.to_string() };
            let text = format!("🖼  couldn't load {label}: {err}");
            let resp = ui.add(egui::Label::new(egui::RichText::new(text).color(pal.text_dim).italics()).sense(Sense::click()));
            if src.starts_with("http") {
                if resp.clicked() {
                    ui.ctx().open_url(egui::OpenUrl::new_tab(src));
                }
                resp.on_hover_text(format!("Open {src} in your browser")).on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        }
        ImgState::Ready(pic) => {
            let retro = cx.retro && !hq;
            let aspect = pic.size[1] as f32 / pic.size[0].max(1) as f32;
            let avail = ui.available_width().max(80.0);
            let mut w = if retro { avail.min(340.0) } else { avail.min(480.0).min(pic.size[0] as f32) };
            let mut h = w * aspect;
            let max_h = if retro { 300.0 } else { 380.0 };
            if h > max_h {
                w *= max_h / h;
                h = max_h;
            }
            let tex = if retro { &pic.retro } else { &pic.full };
            let resp = ui.add(
                egui::Image::new((tex.id(), vec2(w, h)))
                    .corner_radius(CornerRadius::same(if retro { 4 } else { 10 }))
                    .sense(Sense::click()),
            );
            let painter = ui.painter_at(resp.rect);
            if retro {
                let mut y = resp.rect.top();
                while y < resp.rect.bottom() {
                    painter.line_segment([pos2(resp.rect.left(), y), pos2(resp.rect.right(), y)], Stroke::new(1.0, Color32::from_black_alpha(28)));
                    y += 3.0;
                }
            }
            if hq {
                let r = Rect::from_min_size(resp.rect.min + vec2(8.0, 8.0), vec2(30.0, 17.0));
                painter.rect_filled(r, CornerRadius::same(4), alpha(Color32::BLACK, 0.55));
                painter.text(r.center(), Align2::CENTER_CENTER, "HQ", theme::bold(10.0), pal.accent2);
            }
            if resp.hovered() {
                painter.rect_stroke(resp.rect, CornerRadius::same(if retro { 4 } else { 10 }), Stroke::new(2.0, alpha(pal.accent, 0.8)), StrokeKind::Inside);
            }
            let resp = resp.on_hover_text("Click to view full size").on_hover_cursor(egui::CursorIcon::ZoomIn);
            if resp.clicked() {
                *cx.open_image = Some(src.to_string());
            }
            if !alt.is_empty() {
                ui.label(RichText::new(alt).italics().size(cx.size * 0.88).color(pal.text_dim));
            }
        }
    }
    ui.add_space(2.0);
}
