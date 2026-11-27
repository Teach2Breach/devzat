//! Pixel identicons: a symmetric 5x5 sprite and color derived from a name, so everyone gets a face.

use crate::theme::{Palette, lerp};
use eframe::egui::{Color32, CornerRadius, Painter, Rect, Response, Sense, Ui, Vec2, pos2, vec2};

pub fn hash64(s: &str) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51afd7ed558ccd);
    h ^ (h >> 33)
}

pub fn hsv(h: f32, s: f32, v: f32) -> Color32 {
    let h = (h.rem_euclid(360.0)) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    Color32::from_rgb(((r + m) * 255.0) as u8, ((g + m) * 255.0) as u8, ((b + m) * 255.0) as u8)
}

pub fn color_for(seed: &str) -> Color32 {
    hsv((hash64(seed) % 360) as f32, 0.62, 0.98)
}

pub fn paint(painter: &Painter, rect: Rect, seed: &str, pal: &Palette) {
    let h = hash64(&seed.to_lowercase());
    let fg = color_for(&seed.to_lowercase());
    let bg = lerp(pal.surface_hi, fg, if pal.dark { 0.18 } else { 0.14 });
    painter.rect_filled(rect, CornerRadius::same((rect.width() * 0.3).min(255.0) as u8), bg);
    let inner = rect.shrink(rect.width() * 0.18);
    let cell = inner.width() / 5.0;
    let mut filled = 0;
    for y in 0..5 {
        for x in 0..3 {
            if (h >> (y * 3 + x)) & 1 == 1 || (filled == 0 && y == 2 && x == 2) {
                filled += 1;
                for cx in [x, 4 - x] {
                    let min = pos2(inner.min.x + cx as f32 * cell, inner.min.y + y as f32 * cell);
                    painter.rect_filled(Rect::from_min_size(min, vec2(cell, cell)).shrink(cell * 0.06), CornerRadius::same(1), fg);
                }
            }
        }
    }
}

pub fn show(ui: &mut Ui, size: f32, seed: &str, pal: &Palette) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::click());
    if ui.is_rect_visible(rect) {
        paint(ui.painter(), rect, seed, pal);
    }
    resp
}
