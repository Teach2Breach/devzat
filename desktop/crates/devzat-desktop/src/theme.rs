use eframe::egui::{
    self, Color32, CornerRadius, FontData, FontDefinitions, FontFamily, FontId, Margin, Shadow, Stroke, Theme, Vec2,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum ThemeId {
    #[default]
    Neon,
    Phosphor,
    Sunset,
    Paper,
}

#[derive(Clone, Debug)]
pub struct Palette {
    pub name: &'static str,
    pub blurb: &'static str,
    pub dark: bool,
    pub bg: Color32,
    pub panel: Color32,
    pub surface: Color32,
    pub surface_hi: Color32,
    pub border: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub accent: Color32,
    pub accent2: Color32,
    pub on_accent: Color32,
    pub good: Color32,
    pub warn: Color32,
    pub bad: Color32,
    pub code_bg: Color32,
    pub link: Color32,
}

const fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl ThemeId {
    pub const ALL: [ThemeId; 4] = [ThemeId::Neon, ThemeId::Phosphor, ThemeId::Sunset, ThemeId::Paper];

    pub fn from_name(s: &str) -> Option<ThemeId> {
        ThemeId::ALL.into_iter().find(|t| t.palette().name.eq_ignore_ascii_case(s.trim()))
    }

    pub fn palette(self) -> Palette {
        match self {
            ThemeId::Neon => Palette {
                name: "Neon",
                blurb: "Midnight synthwave",
                dark: true,
                bg: rgb(0x0b0d18),
                panel: rgb(0x10132a),
                surface: rgb(0x171b38),
                surface_hi: rgb(0x21264d),
                border: rgb(0x2a3062),
                text: rgb(0xe9eaf6),
                text_dim: rgb(0x8c91bb),
                accent: rgb(0xff4fd8),
                accent2: rgb(0x3de0ff),
                on_accent: rgb(0x14051a),
                good: rgb(0x3ddc97),
                warn: rgb(0xffcc4d),
                bad: rgb(0xff5c7a),
                code_bg: rgb(0x080a14),
                link: rgb(0x6fd3ff),
            },
            ThemeId::Phosphor => Palette {
                name: "Phosphor",
                blurb: "Green-screen terminal",
                dark: true,
                bg: rgb(0x040904),
                panel: rgb(0x071008),
                surface: rgb(0x0c180e),
                surface_hi: rgb(0x132516),
                border: rgb(0x1c3a22),
                text: rgb(0xbaffc9),
                text_dim: rgb(0x5f9f6f),
                accent: rgb(0x39ff6a),
                accent2: rgb(0xc4ff3a),
                on_accent: rgb(0x031205),
                good: rgb(0x39ff6a),
                warn: rgb(0xe5ff5a),
                bad: rgb(0xff6b5a),
                code_bg: rgb(0x020602),
                link: rgb(0x7dffb0),
            },
            ThemeId::Sunset => Palette {
                name: "Sunset",
                blurb: "Warm dusk",
                dark: true,
                bg: rgb(0x170c12),
                panel: rgb(0x1e1018),
                surface: rgb(0x2a1622),
                surface_hi: rgb(0x3a1f2f),
                border: rgb(0x4c2a3c),
                text: rgb(0xffedf2),
                text_dim: rgb(0xc49aa8),
                accent: rgb(0xff8a3d),
                accent2: rgb(0xff3d7f),
                on_accent: rgb(0x220a05),
                good: rgb(0x7ce3a1),
                warn: rgb(0xffd166),
                bad: rgb(0xff5470),
                code_bg: rgb(0x10070b),
                link: rgb(0xffb26b),
            },
            ThemeId::Paper => Palette {
                name: "Paper",
                blurb: "Daylight",
                dark: false,
                bg: rgb(0xf6f3ec),
                panel: rgb(0xebe6db),
                surface: rgb(0xffffff),
                surface_hi: rgb(0xf1ece2),
                border: rgb(0xd8d0c0),
                text: rgb(0x23201b),
                text_dim: rgb(0x786f60),
                accent: rgb(0x6c4cff),
                accent2: rgb(0xff5f9e),
                on_accent: rgb(0xffffff),
                good: rgb(0x1f9d63),
                warn: rgb(0xb7791f),
                bad: rgb(0xd6334f),
                code_bg: rgb(0xf0ebe0),
                link: rgb(0x3355dd),
            },
        }
    }
}

pub fn lerp(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Color32::from_rgba_unmultiplied(m(a.r(), b.r()), m(a.g(), b.g()), m(a.b(), b.b()), m(a.a(), b.a()))
}

pub fn alpha(c: Color32, a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), (a.clamp(0.0, 1.0) * 255.0) as u8)
}

fn luminance(c: Color32) -> f32 {
    let f = |v: u8| {
        let v = v as f32 / 255.0;
        if v <= 0.03928 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
    };
    0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
}

/// Nudges a server-chosen color (made for dark terminals) until it reads well on this theme.
pub fn readable(c: Color32, pal: &Palette) -> Color32 {
    let bg = luminance(pal.bg);
    let mut out = c;
    for _ in 0..8 {
        let l = luminance(out);
        let (hi, lo) = if l > bg { (l, bg) } else { (bg, l) };
        if (hi + 0.05) / (lo + 0.05) >= 3.2 {
            break;
        }
        out = if pal.dark { lerp(out, Color32::WHITE, 0.25) } else { lerp(out, Color32::BLACK, 0.25) };
    }
    out
}

pub fn bold(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("bold".into()))
}

pub fn pixel(size: f32) -> FontId {
    FontId::new(size, FontFamily::Name("pixel".into()))
}

pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = FontDefinitions::default();
    let add = |fonts: &mut FontDefinitions, name: &str, bytes: &'static [u8]| {
        fonts.font_data.insert(name.to_owned(), Arc::new(FontData::from_static(bytes)));
    };
    add(&mut fonts, "inter", include_bytes!("../assets/fonts/Inter-Regular.ttf"));
    add(&mut fonts, "inter-semibold", include_bytes!("../assets/fonts/Inter-SemiBold.ttf"));
    add(&mut fonts, "jbmono", include_bytes!("../assets/fonts/JetBrainsMono-Regular.ttf"));
    add(&mut fonts, "pixel", include_bytes!("../assets/fonts/PressStart2P-Regular.ttf"));

    let proportional = fonts.families.entry(FontFamily::Proportional).or_default();
    proportional.insert(0, "inter".to_owned());
    let fallbacks: Vec<String> = proportional.iter().skip(1).cloned().collect();
    let mono = fonts.families.entry(FontFamily::Monospace).or_default();
    mono.insert(0, "jbmono".to_owned());

    let mut bold = vec!["inter-semibold".to_owned()];
    bold.extend(fallbacks.iter().cloned());
    fonts.families.insert(FontFamily::Name("bold".into()), bold);
    let mut px = vec!["pixel".to_owned(), "inter".to_owned()];
    px.extend(fallbacks);
    fonts.families.insert(FontFamily::Name("pixel".into()), px);
    ctx.set_fonts(fonts);
}

pub fn apply(ctx: &egui::Context, pal: &Palette) {
    let mut v = if pal.dark { egui::Visuals::dark() } else { egui::Visuals::light() };
    v.panel_fill = pal.panel;
    v.window_fill = pal.surface;
    v.window_stroke = Stroke::new(1.0, pal.border);
    v.window_corner_radius = CornerRadius::same(14);
    v.menu_corner_radius = CornerRadius::same(10);
    v.window_shadow = Shadow { offset: [0, 10], blur: 28, spread: 0, color: Color32::from_black_alpha(if pal.dark { 150 } else { 45 }) };
    v.popup_shadow = Shadow { offset: [0, 6], blur: 18, spread: 0, color: Color32::from_black_alpha(if pal.dark { 130 } else { 35 }) };
    v.extreme_bg_color = pal.code_bg;
    v.text_edit_bg_color = Some(pal.surface);
    v.faint_bg_color = pal.surface_hi;
    v.code_bg_color = pal.code_bg;
    v.hyperlink_color = pal.link;
    v.warn_fg_color = pal.warn;
    v.error_fg_color = pal.bad;
    v.selection.bg_fill = alpha(pal.accent, 0.35);
    v.selection.stroke = Stroke::new(1.0, pal.text);
    v.text_cursor.stroke = Stroke::new(2.0, pal.accent);
    v.override_text_color = Some(pal.text);
    v.weak_text_color = Some(pal.text_dim);

    let radius = CornerRadius::same(8);
    let w = &mut v.widgets;
    w.noninteractive.bg_fill = pal.surface;
    w.noninteractive.weak_bg_fill = pal.surface;
    w.noninteractive.bg_stroke = Stroke::new(1.0, pal.border);
    w.noninteractive.fg_stroke = Stroke::new(1.0, pal.text);
    w.noninteractive.corner_radius = radius;
    w.inactive.bg_fill = pal.surface_hi;
    w.inactive.weak_bg_fill = pal.surface_hi;
    w.inactive.bg_stroke = Stroke::NONE;
    w.inactive.fg_stroke = Stroke::new(1.0, pal.text);
    w.inactive.corner_radius = radius;
    w.hovered.bg_fill = lerp(pal.surface_hi, pal.accent, 0.18);
    w.hovered.weak_bg_fill = lerp(pal.surface_hi, pal.accent, 0.18);
    w.hovered.bg_stroke = Stroke::new(1.0, alpha(pal.accent, 0.6));
    w.hovered.fg_stroke = Stroke::new(1.5, pal.text);
    w.hovered.corner_radius = radius;
    w.active.bg_fill = lerp(pal.surface_hi, pal.accent, 0.32);
    w.active.weak_bg_fill = lerp(pal.surface_hi, pal.accent, 0.32);
    w.active.bg_stroke = Stroke::new(1.0, pal.accent);
    w.active.fg_stroke = Stroke::new(2.0, pal.text);
    w.active.corner_radius = radius;
    w.open = w.active;

    ctx.set_visuals_of(Theme::Dark, v.clone());
    ctx.set_visuals_of(Theme::Light, v);
    ctx.set_theme(if pal.dark { egui::ThemePreference::Dark } else { egui::ThemePreference::Light });
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
        s.spacing.button_padding = Vec2::new(10.0, 5.0);
        s.spacing.window_margin = Margin::same(16);
        s.spacing.menu_margin = Margin::same(8);
        s.spacing.interact_size.y = 26.0;
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(14.5));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(14.0));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.5));
        s.text_styles.insert(egui::TextStyle::Monospace, FontId::monospace(13.0));
        s.text_styles.insert(egui::TextStyle::Heading, bold(20.0));
    });
}
