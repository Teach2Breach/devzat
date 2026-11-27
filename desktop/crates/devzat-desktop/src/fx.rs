//! The fun layer: a synthwave backdrop for the launch screen, confetti, and optional CRT scanlines.

use crate::theme::{Palette, alpha, lerp};
use eframe::egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, Vec2, pos2, vec2};
use eframe::epaint::Mesh;

struct Particle {
    pos: Pos2,
    vel: Vec2,
    rot: f32,
    spin: f32,
    size: Vec2,
    color: Color32,
    life: f32,
    ttl: f32,
}

pub struct Fx {
    stars: Vec<[f32; 3]>,
    parts: Vec<Particle>,
    seed: u64,
}

impl Fx {
    pub fn new() -> Self {
        let mut fx = Fx { stars: Vec::new(), parts: Vec::new(), seed: 0x9e3779b97f4a7c15 };
        for _ in 0..220 {
            let s = [fx.rand() * 2.0 - 1.0, fx.rand() * 2.0 - 1.0, fx.rand().max(0.05)];
            fx.stars.push(s);
        }
        fx
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 7;
        self.seed ^= self.seed << 17;
        (self.seed >> 40) as f32 / (1u64 << 24) as f32
    }

    pub fn active(&self) -> bool {
        !self.parts.is_empty()
    }

    /// Starfield, retro sun, and a scrolling perspective grid.
    pub fn backdrop(&mut self, painter: &Painter, rect: Rect, t: f32, dt: f32, pal: &Palette) {
        let horizon = rect.top() + rect.height() * 0.70;
        let sky_top = pal.bg;
        let sky_low = lerp(pal.bg, pal.accent, if pal.dark { 0.20 } else { 0.10 });
        vertical_gradient(painter, Rect::from_min_max(rect.min, pos2(rect.right(), horizon)), sky_top, sky_low);
        vertical_gradient(
            painter,
            Rect::from_min_max(pos2(rect.left(), horizon), rect.max),
            lerp(pal.bg, pal.accent2, if pal.dark { 0.10 } else { 0.06 }),
            pal.bg,
        );

        let center = pos2(rect.center().x, rect.top() + rect.height() * 0.42);
        let scale = rect.width().max(rect.height()) * 0.55;
        let star_color = if pal.dark { Color32::WHITE } else { pal.accent };
        for i in 0..self.stars.len() {
            let mut s = self.stars[i];
            s[2] -= dt * 0.045;
            if s[2] <= 0.04 {
                s = [self.rand() * 2.0 - 1.0, self.rand() * 2.0 - 1.0, 1.0];
            }
            self.stars[i] = s;
            let p = center + vec2(s[0] / s[2], s[1] / s[2]) * scale * 0.18;
            if p.y > horizon - 2.0 || !rect.contains(p) {
                continue;
            }
            let near = 1.0 - s[2];
            painter.circle_filled(p, 0.4 + near * 1.8, alpha(star_color, 0.15 + near * 0.75));
        }

        // The sun: horizontal bars cut from a circle, with gaps growing toward the horizon.
        // It's wider than the launch card so it glows out from behind it.
        let r = (rect.width() * 0.24).min(rect.height() * 0.42);
        let sun_c = pos2(rect.center().x, horizon);
        let bars = 26;
        for i in 0..bars {
            let y0 = sun_c.y - r + (2.0 * r) * i as f32 / bars as f32;
            let y1 = sun_c.y - r + (2.0 * r) * (i + 1) as f32 / bars as f32;
            if y0 >= horizon {
                break;
            }
            let k = i as f32 / bars as f32;
            let gap = if k > 0.45 { (k - 0.45) * 7.0 } else { 0.0 };
            let dy = ((y0 + y1) * 0.5 - sun_c.y) / r;
            let half = r * (1.0 - dy * dy).max(0.0).sqrt();
            let c = lerp(pal.accent2, pal.accent, k);
            painter.rect_filled(
                Rect::from_min_max(pos2(sun_c.x - half, y0), pos2(sun_c.x + half, (y1 - gap).min(horizon).max(y0))),
                0.0,
                alpha(c, if pal.dark { 0.85 } else { 0.55 }),
            );
        }

        let grid = alpha(pal.accent, if pal.dark { 0.55 } else { 0.35 });
        painter.line_segment([pos2(rect.left(), horizon), pos2(rect.right(), horizon)], Stroke::new(1.5, grid));
        let vp = pos2(rect.center().x, horizon);
        for i in -16..=16 {
            let bottom = pos2(rect.center().x + i as f32 * rect.width() * 0.09, rect.bottom());
            painter.line_segment([vp, bottom], Stroke::new(1.0, alpha(pal.accent, 0.25)));
        }
        let phase = (t * 0.45).fract();
        let rows = 14;
        for k in 0..rows {
            let d = (k as f32 + phase) / rows as f32;
            let y = horizon + (rect.bottom() - horizon) * d * d;
            painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, alpha(pal.accent, 0.12 + d * 0.5)));
        }
    }

    pub fn burst(&mut self, at: Pos2, n: usize, colors: &[Color32]) {
        for _ in 0..n {
            let angle = self.rand() * std::f32::consts::TAU;
            let speed = 140.0 + self.rand() * 380.0;
            let color = colors[(self.rand() * colors.len() as f32) as usize % colors.len()];
            let p = Particle {
                pos: at,
                vel: vec2(angle.cos() * speed, angle.sin() * speed - 180.0),
                rot: self.rand() * 6.0,
                spin: (self.rand() - 0.5) * 14.0,
                size: vec2(5.0 + self.rand() * 6.0, 3.0 + self.rand() * 4.0),
                color,
                life: 0.0,
                ttl: 1.6 + self.rand() * 1.4,
            };
            self.parts.push(p);
        }
    }

    pub fn rain(&mut self, rect: Rect, n: usize, colors: &[Color32]) {
        for _ in 0..n {
            let x = rect.left() + self.rand() * rect.width();
            let color = colors[(self.rand() * colors.len() as f32) as usize % colors.len()];
            let p = Particle {
                pos: pos2(x, rect.top() - self.rand() * 160.0),
                vel: vec2((self.rand() - 0.5) * 90.0, 60.0 + self.rand() * 140.0),
                rot: self.rand() * 6.0,
                spin: (self.rand() - 0.5) * 10.0,
                size: vec2(5.0 + self.rand() * 6.0, 3.0 + self.rand() * 4.0),
                color,
                life: 0.0,
                ttl: 3.0 + self.rand() * 1.8,
            };
            self.parts.push(p);
        }
    }

    pub fn paint_particles(&mut self, painter: &Painter, dt: f32) {
        let dt = dt.min(0.05);
        self.parts.retain_mut(|p| {
            p.life += dt;
            p.vel.y += 420.0 * dt;
            p.vel *= 1.0 - 0.9 * dt;
            p.pos += p.vel * dt;
            p.rot += p.spin * dt;
            p.life < p.ttl
        });
        for p in &self.parts {
            let fade = (1.0 - (p.life / p.ttl).powi(3)).clamp(0.0, 1.0);
            let (s, c) = p.rot.sin_cos();
            let wobble = (p.rot * 2.0).cos().abs().max(0.25);
            let hx = vec2(c, s) * p.size.x * 0.5;
            let hy = vec2(-s, c) * p.size.y * 0.5 * wobble;
            let pts = vec![p.pos - hx - hy, p.pos + hx - hy, p.pos + hx + hy, p.pos - hx + hy];
            painter.add(Shape::convex_polygon(pts, alpha(p.color, fade), Stroke::NONE));
        }
    }
}

pub fn vertical_gradient(painter: &Painter, rect: Rect, top: Color32, bottom: Color32) {
    let mut mesh = Mesh::default();
    mesh.colored_vertex(rect.left_top(), top);
    mesh.colored_vertex(rect.right_top(), top);
    mesh.colored_vertex(rect.right_bottom(), bottom);
    mesh.colored_vertex(rect.left_bottom(), bottom);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    painter.add(Shape::mesh(mesh));
}

/// Scanlines and a soft vignette, for people who miss their CRT.
pub fn crt(painter: &Painter, rect: Rect) {
    let mut y = rect.top();
    while y < rect.bottom() {
        painter.line_segment([pos2(rect.left(), y), pos2(rect.right(), y)], Stroke::new(1.0, Color32::from_black_alpha(30)));
        y += 3.0;
    }
    let edge = Color32::from_black_alpha(70);
    let clear = Color32::TRANSPARENT;
    let d = (rect.width().min(rect.height()) * 0.12).max(24.0);
    let strip = |a: Pos2, b: Pos2, c: Pos2, e: Pos2, ca: Color32, cb: Color32| {
        let mut mesh = Mesh::default();
        mesh.colored_vertex(a, ca);
        mesh.colored_vertex(b, ca);
        mesh.colored_vertex(c, cb);
        mesh.colored_vertex(e, cb);
        mesh.add_triangle(0, 1, 2);
        mesh.add_triangle(0, 2, 3);
        painter.add(Shape::mesh(mesh));
    };
    strip(rect.left_top(), rect.right_top(), pos2(rect.right(), rect.top() + d), pos2(rect.left(), rect.top() + d), edge, clear);
    strip(rect.left_bottom(), rect.right_bottom(), pos2(rect.right(), rect.bottom() - d), pos2(rect.left(), rect.bottom() - d), edge, clear);
    strip(rect.left_top(), rect.left_bottom(), pos2(rect.left() + d, rect.bottom()), pos2(rect.left() + d, rect.top()), edge, clear);
    strip(rect.right_top(), rect.right_bottom(), pos2(rect.right() - d, rect.bottom()), pos2(rect.right() - d, rect.top()), edge, clear);
}
