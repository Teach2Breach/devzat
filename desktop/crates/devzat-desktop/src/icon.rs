//! The window icon: a pixel-art chat bubble with a shell prompt, drawn in code so there's no asset to ship.

use eframe::egui::IconData;

const ART: [&str; 16] = [
    "................",
    "................",
    "..wwwwwwwwwwww..",
    ".wwwwwwwwwwwwww.",
    ".wwkwwwwwwwwwww.",
    ".wwwkwwwwwwwwww.",
    ".wwwwkwwwwwwwww.",
    ".wwwkwwwwwwwwww.",
    ".wwkwwwkkkkkwww.",
    ".wwwwwwwwwwwwww.",
    "..wwwwwwwwwwww..",
    "....wwww........",
    "....www.........",
    "....ww..........",
    "................",
    "................",
];

pub fn pixels(scale: usize) -> (Vec<u8>, usize) {
    let size = 16 * scale;
    let mut rgba = vec![0u8; size * size * 4];
    let from = [0xff_u8, 0x4f, 0xd8];
    let to = [0x3d_u8, 0xe0, 0xff];
    for (py, row) in ART.iter().enumerate() {
        for (px, c) in row.bytes().enumerate() {
            let corner = (px == 0 || px == 15) && (py == 0 || py == 15);
            let t = (px + py) as f32 / 30.0;
            let bg = [
                (from[0] as f32 + (to[0] as f32 - from[0] as f32) * t) as u8,
                (from[1] as f32 + (to[1] as f32 - from[1] as f32) * t) as u8,
                (from[2] as f32 + (to[2] as f32 - from[2] as f32) * t) as u8,
            ];
            let color = match c {
                b'w' => [0xfa, 0xf8, 0xff, 0xff],
                b'k' => [0x16, 0x12, 0x2e, 0xff],
                _ if corner => [0, 0, 0, 0],
                _ => [bg[0], bg[1], bg[2], 0xff],
            };
            for dy in 0..scale {
                for dx in 0..scale {
                    let i = ((py * scale + dy) * size + px * scale + dx) * 4;
                    rgba[i..i + 4].copy_from_slice(&color);
                }
            }
        }
    }
    (rgba, size)
}

pub fn app_icon() -> IconData {
    let (rgba, size) = pixels(4);
    IconData { rgba, width: size as u32, height: size as u32 }
}
