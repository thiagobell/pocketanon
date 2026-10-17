//! PocketAnon desktop app.

mod app;
mod file_ws;
mod palette;
mod review_ui;
mod services;
mod settings;
mod settings_ui;
mod text_util;
mod text_ws;
mod theme;
mod toast;
mod widgets;
mod worker;

#[cfg(test)]
mod ui_tests;

use services::Services;
use services::stubs::{FakeModelManager, PlainTextLoader};

/// Builds the services the app runs with. STUBS for now: the lead swaps in
/// `pocketanon-documents`, `pocketanon-detect-rules` and `pocketanon-detect-gliner` here.
fn services() -> Services {
    Services { detectors: Vec::new(), documents: Box::new(PlainTextLoader), models: Box::new(FakeModelManager) }
}

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PocketAnon")
            .with_inner_size([1360.0, 840.0])
            .with_min_inner_size([980.0, 600.0])
            .with_drag_and_drop(true),
        ..Default::default()
    };
    eframe::run_native(
        "PocketAnon",
        options,
        Box::new(|cc| {
            Ok(Box::new(app::PocketAnon::new(Some(cc.egui_ctx.clone()), services(), settings::SettingsStore::platform())))
        }),
    )
}
