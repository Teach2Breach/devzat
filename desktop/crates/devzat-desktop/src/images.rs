//! Fetches pictures through the server, decodes them off the UI thread, and keeps two textures each:
//! a chunky pixelated one (the terminal look) and a sharp one for `!hq` posts and the lightbox.

use devzat_core::{ClientHandle, Command};
use eframe::egui::{self, ColorImage, TextureHandle, TextureOptions};
use image::imageops::FilterType;
use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

pub const RETRO_WIDTH: u32 = 72;
const MAX_FULL: u32 = 1600;

pub struct Picture {
    pub full: TextureHandle,
    pub retro: TextureHandle,
    pub size: [usize; 2],
}

pub enum ImgState {
    Loading(Instant),
    Ready(Picture),
    Failed(String),
}

struct Decoded {
    full: ColorImage,
    retro: ColorImage,
}

pub struct Images {
    states: HashMap<String, ImgState>,
    pending: HashMap<u64, String>,
    next_req: u64,
    to_worker: Sender<(String, Vec<u8>)>,
    from_worker: Receiver<(String, Result<Decoded, String>)>,
}

impl Images {
    pub fn new(ctx: egui::Context) -> Self {
        let (to_worker, jobs) = channel::<(String, Vec<u8>)>();
        let (done, from_worker) = channel();
        std::thread::Builder::new()
            .name("devzat-images".into())
            .spawn(move || {
                for (reference, bytes) in jobs {
                    let _ = done.send((reference, decode(&bytes)));
                    ctx.request_repaint();
                }
            })
            .expect("image thread");
        Images { states: HashMap::new(), pending: HashMap::new(), next_req: 1, to_worker, from_worker }
    }

    /// Returns the picture's state, asking the server for it the first time.
    pub fn get(&mut self, reference: &str, client: &ClientHandle) -> &ImgState {
        let stale = matches!(self.states.get(reference), Some(ImgState::Loading(t)) if t.elapsed() > Duration::from_secs(25));
        if stale || !self.states.contains_key(reference) {
            let req = self.next_req;
            self.next_req += 1;
            self.pending.insert(req, reference.to_string());
            client.send(Command::Fetch { req, reference: reference.to_string() });
            self.states.insert(reference.to_string(), ImgState::Loading(Instant::now()));
        }
        &self.states[reference]
    }

    pub fn peek(&self, reference: &str) -> Option<&ImgState> {
        self.states.get(reference)
    }

    pub fn on_blob(&mut self, req: u64, reference: String, result: Result<(String, Vec<u8>), String>) {
        let reference = self.pending.remove(&req).unwrap_or(reference);
        match result {
            Ok((_, bytes)) => {
                let _ = self.to_worker.send((reference, bytes));
            }
            Err(e) => {
                self.states.insert(reference, ImgState::Failed(e));
            }
        }
    }

    pub fn pump(&mut self, ctx: &egui::Context) {
        while let Ok((reference, result)) = self.from_worker.try_recv() {
            let state = match result {
                Ok(d) => {
                    let size = d.full.size;
                    let full = ctx.load_texture(format!("full:{reference}"), d.full, TextureOptions::LINEAR);
                    let retro = ctx.load_texture(format!("retro:{reference}"), d.retro, TextureOptions::NEAREST);
                    ImgState::Ready(Picture { full, retro, size })
                }
                Err(e) => ImgState::Failed(e),
            };
            self.states.insert(reference, state);
        }
    }

    /// After reconnecting, try pictures again that failed or never arrived.
    pub fn retry_unfinished(&mut self) {
        self.states.retain(|_, s| matches!(s, ImgState::Ready(_)));
        self.pending.clear();
    }
}

fn decode(bytes: &[u8]) -> Result<Decoded, String> {
    let img = image::load_from_memory(bytes).map_err(|_| "couldn't decode picture".to_string())?;
    let img = if img.width() > MAX_FULL || img.height() > MAX_FULL { img.resize(MAX_FULL, MAX_FULL, FilterType::Triangle) } else { img };
    let rgba = img.to_rgba8();
    let full = ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
    let retro_w = RETRO_WIDTH.min(img.width().max(1));
    let retro_h = ((img.height() as f32 * retro_w as f32 / img.width().max(1) as f32).round() as u32).clamp(1, 160);
    let small = image::imageops::resize(&rgba, retro_w, retro_h, FilterType::Triangle);
    let retro = ColorImage::from_rgba_unmultiplied([small.width() as usize, small.height() as usize], small.as_raw());
    Ok(Decoded { full, retro })
}

/// Encodes a pasted clipboard image as PNG for upload.
pub fn png_from_color_image(img: &ColorImage) -> Option<Vec<u8>> {
    let mut rgba = Vec::with_capacity(img.pixels.len() * 4);
    for p in &img.pixels {
        let [r, g, b, a] = p.to_srgba_unmultiplied();
        rgba.extend_from_slice(&[r, g, b, a]);
    }
    let buf = image::RgbaImage::from_raw(img.size[0] as u32, img.size[1] as u32, rgba)?;
    let mut out = std::io::Cursor::new(Vec::new());
    buf.write_to(&mut out, image::ImageFormat::Png).ok()?;
    Some(out.into_inner())
}

/// Decodes a file the user picked or dropped, for the "send picture" preview.
pub fn preview(bytes: &[u8]) -> Option<ColorImage> {
    let img = image::load_from_memory(bytes).ok()?;
    let img = img.thumbnail(640, 480).to_rgba8();
    Some(ColorImage::from_rgba_unmultiplied([img.width() as usize, img.height() as usize], img.as_raw()))
}
