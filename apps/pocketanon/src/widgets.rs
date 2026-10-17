//! Small styled building blocks: cards, primary buttons, chips, section labels.

use egui::{Align2, Color32, CornerRadius, FontId, Frame, Margin, Response, RichText, Sense, Stroke, Ui, Vec2};

use crate::theme::Tokens;

/// A rounded, bordered card.
pub fn card(tk: &Tokens) -> Frame {
    Frame::new()
        .fill(tk.card)
        .stroke(Stroke::new(1.0, tk.border))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(12))
}

/// Upper-case, dim section caption.
pub fn caption(ui: &mut Ui, tk: &Tokens, text: &str) {
    ui.label(RichText::new(text.to_uppercase()).size(10.5).color(tk.text_faint).strong());
}

/// Card header row: title on the left, extra widgets on the right.
pub fn card_header(ui: &mut Ui, tk: &Tokens, title: &str, right: impl FnOnce(&mut Ui)) {
    ui.horizontal(|ui| {
        ui.label(RichText::new(title).size(14.0).strong().color(tk.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), right);
    });
}

/// The accent-filled main action button.
pub fn primary_button(ui: &mut Ui, tk: &Tokens, text: &str, enabled: bool) -> Response {
    let fill = if enabled { tk.accent } else { tk.pressed };
    let color = if enabled { tk.on_accent } else { tk.text_faint };
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).color(color).strong())
            .fill(fill)
            .stroke(Stroke::NONE)
            .corner_radius(CornerRadius::same(6))
            .min_size(Vec2::new(0.0, 28.0)),
    )
}

/// A flat secondary button.
pub fn ghost_button(ui: &mut Ui, tk: &Tokens, text: &str, enabled: bool) -> Response {
    ui.add_enabled(
        enabled,
        egui::Button::new(RichText::new(text).color(if enabled { tk.text } else { tk.text_faint }))
            .fill(tk.card)
            .stroke(Stroke::new(1.0, tk.border))
            .corner_radius(CornerRadius::same(6))
            .min_size(Vec2::new(0.0, 28.0)),
    )
}

/// A coloured pill with a label, e.g. an Entity Type.
pub fn chip(ui: &mut Ui, text: &str, color: Color32, dark: bool) -> Response {
    let font = FontId::proportional(11.5);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font.clone(), color);
    let size = galley.size() + Vec2::new(14.0, 6.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    let fill = Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), if dark { 0x2E } else { 0x22 });
    ui.painter().rect(rect, CornerRadius::same(255), fill, Stroke::new(1.0, color.gamma_multiply(0.55)), egui::StrokeKind::Inside);
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, text, font, color);
    resp
}

/// A segmented control over `options`; returns true when the value changed.
pub fn segmented<T: PartialEq + Copy>(ui: &mut Ui, tk: &Tokens, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    Frame::new()
        .fill(tk.input)
        .stroke(Stroke::new(1.0, tk.border))
        .corner_radius(CornerRadius::same(7))
        .inner_margin(Margin::same(2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            ui.horizontal(|ui| {
                for (v, label) in options {
                    let on = *value == *v;
                    let b = egui::Button::new(RichText::new(*label).size(12.0).color(if on { tk.text } else { tk.text_dim }))
                        .fill(if on { tk.pressed } else { Color32::TRANSPARENT })
                        .stroke(Stroke::NONE)
                        .corner_radius(CornerRadius::same(5))
                        .min_size(Vec2::new(0.0, 22.0));
                    if ui.add(b).clicked() && !on {
                        *value = *v;
                        changed = true;
                    }
                }
            });
        });
    changed
}

/// A toggleable pill (for Languages and similar multi-selects).
pub fn toggle_pill(ui: &mut Ui, tk: &Tokens, on: &mut bool, text: &str) -> Response {
    let b = egui::Button::new(RichText::new(text).size(12.0).color(if *on { tk.on_accent } else { tk.text_dim }))
        .fill(if *on { tk.accent } else { tk.input })
        .stroke(if *on { Stroke::NONE } else { Stroke::new(1.0, tk.border) })
        .corner_radius(CornerRadius::same(255))
        .min_size(Vec2::new(0.0, 24.0));
    let r = ui.add(b);
    if r.clicked() {
        *on = !*on;
    }
    r
}
