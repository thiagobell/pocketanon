//! The "About PocketAnon" window, opened from the Options palette.

use egui::{RichText, Ui};

use crate::theme::Tokens;

/// Cargo joins several authors with ':' in `CARGO_PKG_AUTHORS`; show them comma-separated.
pub fn format_authors(raw: &str) -> String {
    raw.split(':').map(str::trim).filter(|a| !a.is_empty()).collect::<Vec<_>>().join(", ")
}

pub fn window(ctx: &egui::Context, open: &mut bool, tk: &Tokens) {
    egui::Window::new("About PocketAnon")
        .open(open)
        .collapsible(false)
        .resizable(false)
        .default_width(360.0)
        .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
        .show(ctx, |ui| body(ui, tk));
}

fn body(ui: &mut Ui, tk: &Tokens) {
    ui.label(RichText::new("PocketAnon").size(20.0).strong().color(tk.text));
    ui.label(RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION"))).color(tk.text_dim));
    ui.add_space(6.0);
    ui.label(RichText::new("Local anonymization of text and documents. Nothing leaves this device.").color(tk.text_dim));
    ui.add_space(8.0);
    egui::Grid::new("about_grid").num_columns(2).spacing([12.0, 4.0]).show(ui, |ui| {
        ui.label(RichText::new("Author").color(tk.text_faint));
        ui.label(format_authors(env!("CARGO_PKG_AUTHORS")));
        ui.end_row();
        ui.label(RichText::new("Homepage").color(tk.text_faint));
        ui.hyperlink(env!("CARGO_PKG_HOMEPAGE"));
        ui.end_row();
        ui.label(RichText::new("License").color(tk.text_faint));
        ui.label("MIT OR Apache-2.0");
        ui.end_row();
    });
    ui.add_space(8.0);
    ui.label(
        RichText::new("Built on GLiNER, gline-rs, ONNX Runtime, Wordcraft and egui; see THIRD-PARTY-LICENSES.md.")
            .size(11.5)
            .color(tk.text_faint),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authors_are_comma_separated() {
        assert_eq!(format_authors("Max Mustermann <max@example.com>"), "Max Mustermann <max@example.com>");
        assert_eq!(
            format_authors("Max Mustermann <max@example.com>:Erika Musterfrau <erika@example.com>"),
            "Max Mustermann <max@example.com>, Erika Musterfrau <erika@example.com>"
        );
        assert_eq!(format_authors(""), "");
    }
}
