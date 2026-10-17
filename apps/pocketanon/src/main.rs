//! PocketAnon desktop app.

mod app;
mod file_ws;
mod legend;
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
use services::real::{GlinerModelManager, WordcraftLoader};

/// Builds the services the app runs with: the Rule Detector is always on, documents go through
/// the Wordcraft adapter, and models come from the GLiNER Registry. The stubs in
/// `services::stubs` remain for UI tests only.
fn services() -> Services {
    let models: Box<dyn services::ModelManager> = match GlinerModelManager::new() {
        Ok(m) => Box::new(m),
        Err(e) => {
            log::error!("model manager unavailable, falling back to demo: {e}");
            Box::new(services::stubs::FakeModelManager)
        }
    };
    Services {
        detectors: vec![Box::new(pocketanon_detect_rules::RuleDetector::new())],
        documents: Box::new(WordcraftLoader),
        models,
    }
}

/// The window and dock icon, rasterised from `assets/icon.svg` by `packaging/make-icons.sh`.
fn window_icon() -> egui::IconData {
    let png = include_bytes!("../../../assets/icon-256.png");
    match image::load_from_memory(png) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            let (width, height) = rgba.dimensions();
            egui::IconData { rgba: rgba.into_raw(), width, height }
        }
        Err(e) => {
            log::warn!("window icon unavailable: {e}");
            egui::IconData::default()
        }
    }
}

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("PocketAnon")
            .with_icon(window_icon())
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
