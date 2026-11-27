#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod avatar;
mod chat;
mod chrome;
mod composer;
mod dialogs;
mod fx;
mod icon;
mod images;
mod launch;
mod model;
mod rich;
mod settings;
mod theme;
mod widgets;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_title("Devzat")
            .with_app_id("devzat-desktop")
            .with_inner_size([1180.0, 760.0])
            .with_min_inner_size([720.0, 480.0])
            .with_decorations(false)
            .with_icon(icon::app_icon()),
        persistence_path: devzat_core::paths::state_file_override(),
        ..Default::default()
    };
    eframe::run_native("Devzat", options, Box::new(|cc| Ok(Box::new(app::DevzatApp::new(cc)))))
}
