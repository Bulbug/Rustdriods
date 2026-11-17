// Hide the console window in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod adb;
mod app;
mod icons;
mod settings;
mod theme;
mod widgets;
mod devices;
mod download;
mod hook;
mod inject;
mod images;
mod java;
mod keymap;
mod launcher;
mod optimize;
mod paths;
mod runner;
mod sdk;
mod virt;

/// One error type for everything that can fail (keeps the code simple).
pub type Res<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() -> eframe::Result<()> {
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_icon(app_icon())
            .with_inner_size([1000.0, 680.0])
            .with_min_inner_size([860.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        "Android Emulator Manager",
        options,
        Box::new(|cc| Ok(Box::new(app::App::new(cc)))),
    )
}

/// The 128x128 window/taskbar icon (raw RGBA bytes, so no image decoder is needed).
fn app_icon() -> eframe::egui::IconData {
    eframe::egui::IconData {
        rgba: include_bytes!("../assets/logo-128.rgba").to_vec(),
        width: 128,
        height: 128,
    }
}
