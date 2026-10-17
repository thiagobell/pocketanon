//! Design tokens, the egui style, and the per-Type-Group highlight colours.
//!
//! Palette values and the `visuals()` mapping are adapted from WordCraft's `theme.rs`
//! (https://github.com/storytold/wordcraft, MIT OR Apache-2.0), retuned for PocketAnon's
//! flatter, darker ArtCraft look.

use egui::{Color32, CornerRadius, FontId, Stroke, Visuals};
use pocketanon_core::{EntityType, TypeCatalog};
use serde::{Deserialize, Serialize};

/// The interface theme setting. Dark is the default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    #[default]
    Dark,
    Light,
}

/// Colours used by our own painting (cards, rail, chips) on top of egui's visuals.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub dark: bool,
    pub canvas: Color32,
    pub panel: Color32,
    pub rail: Color32,
    pub card: Color32,
    pub card_hover: Color32,
    pub input: Color32,
    pub border: Color32,
    pub border_strong: Color32,
    pub text: Color32,
    pub text_dim: Color32,
    pub text_faint: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub danger: Color32,
    pub warn: Color32,
    pub ok: Color32,
    pub menu: Color32,
}

/// Accent shared by both palettes (ArtCraft violet-blue).
pub const ACCENT: Color32 = Color32::from_rgb(0x7C, 0x6C, 0xF2);

impl Tokens {
    pub fn dark() -> Self {
        Tokens {
            dark: true,
            canvas: Color32::from_rgb(0x12, 0x12, 0x15),
            panel: Color32::from_rgb(0x1A, 0x1A, 0x1E),
            rail: Color32::from_rgb(0x15, 0x15, 0x18),
            card: Color32::from_rgb(0x21, 0x21, 0x26),
            card_hover: Color32::from_rgb(0x28, 0x28, 0x2E),
            input: Color32::from_rgb(0x16, 0x16, 0x1A),
            border: Color32::from_rgb(0x2C, 0x2C, 0x33),
            border_strong: Color32::from_rgb(0x3E, 0x3E, 0x47),
            text: Color32::from_rgb(0xEC, 0xEC, 0xF1),
            text_dim: Color32::from_rgb(0xA6, 0xA6, 0xB2),
            text_faint: Color32::from_rgb(0x6C, 0x6C, 0x78),
            hover: Color32::from_rgb(0x2A, 0x2A, 0x31),
            pressed: Color32::from_rgb(0x34, 0x34, 0x3D),
            accent: ACCENT,
            accent_soft: Color32::from_rgba_unmultiplied(0x7C, 0x6C, 0xF2, 0x33),
            on_accent: Color32::WHITE,
            danger: Color32::from_rgb(0xEE, 0x6A, 0x6A),
            warn: Color32::from_rgb(0xF0, 0xB0, 0x50),
            ok: Color32::from_rgb(0x5B, 0xC0, 0x8A),
            menu: Color32::from_rgb(0x22, 0x22, 0x28),
        }
    }

    pub fn light() -> Self {
        Tokens {
            dark: false,
            canvas: Color32::from_rgb(0xEE, 0xEE, 0xF2),
            panel: Color32::from_rgb(0xF8, 0xF8, 0xFA),
            rail: Color32::from_rgb(0xF0, 0xF0, 0xF4),
            card: Color32::WHITE,
            card_hover: Color32::from_rgb(0xF4, 0xF4, 0xF8),
            input: Color32::WHITE,
            border: Color32::from_rgb(0xE1, 0xE1, 0xE8),
            border_strong: Color32::from_rgb(0xC8, 0xC8, 0xD2),
            text: Color32::from_rgb(0x20, 0x20, 0x26),
            text_dim: Color32::from_rgb(0x5E, 0x5E, 0x6A),
            text_faint: Color32::from_rgb(0x9A, 0x9A, 0xA6),
            hover: Color32::from_rgb(0xE8, 0xE8, 0xEE),
            pressed: Color32::from_rgb(0xDA, 0xDA, 0xE2),
            accent: Color32::from_rgb(0x5B, 0x4B, 0xE0),
            accent_soft: Color32::from_rgba_unmultiplied(0x5B, 0x4B, 0xE0, 0x26),
            on_accent: Color32::WHITE,
            danger: Color32::from_rgb(0xC6, 0x34, 0x34),
            warn: Color32::from_rgb(0xB8, 0x6E, 0x10),
            ok: Color32::from_rgb(0x2E, 0x8B, 0x57),
            menu: Color32::WHITE,
        }
    }

    pub fn for_theme(theme: Theme) -> Self {
        match theme {
            Theme::Dark => Tokens::dark(),
            Theme::Light => Tokens::light(),
        }
    }

    /// The tokens for whatever egui is currently drawing.
    pub fn current(ctx: &egui::Context) -> Self {
        if ctx.global_style().visuals.dark_mode { Tokens::dark() } else { Tokens::light() }
    }
}

/// Apply the theme to egui.
pub fn apply(ctx: &egui::Context, theme: Theme) {
    let t = Tokens::for_theme(theme);
    ctx.set_visuals_of(egui::Theme::Dark, visuals(&Tokens::dark()));
    ctx.set_visuals_of(egui::Theme::Light, visuals(&Tokens::light()));
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 4.0);
        s.spacing.interact_size = egui::vec2(24.0, 26.0);
        s.spacing.menu_margin = egui::Margin::same(6);
        s.spacing.slider_width = 160.0;
        s.text_styles.insert(egui::TextStyle::Body, FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Button, FontId::proportional(13.0));
        s.text_styles.insert(egui::TextStyle::Small, FontId::proportional(11.0));
        s.text_styles.insert(egui::TextStyle::Monospace, FontId::monospace(12.5));
        s.text_styles.insert(egui::TextStyle::Heading, FontId::proportional(18.0));
        s.interaction.tooltip_delay = 0.4;
    });
    ctx.options_mut(|o| {
        o.theme_preference = if t.dark { egui::ThemePreference::Dark } else { egui::ThemePreference::Light };
    });
}

fn visuals(t: &Tokens) -> Visuals {
    let mut v = if t.dark { Visuals::dark() } else { Visuals::light() };
    v.panel_fill = t.panel;
    v.window_fill = t.menu;
    v.extreme_bg_color = t.input;
    v.faint_bg_color = t.card;
    v.code_bg_color = t.card;
    v.window_stroke = Stroke::new(1.0, t.border_strong);
    v.window_corner_radius = CornerRadius::same(10);
    v.menu_corner_radius = CornerRadius::same(8);
    v.selection.bg_fill = Color32::from_rgba_unmultiplied(t.accent.r(), t.accent.g(), t.accent.b(), 0x66);
    v.selection.stroke = Stroke::new(1.0, t.text);
    v.hyperlink_color = t.accent;
    v.text_cursor.stroke = Stroke::new(2.0, t.accent);
    let r = CornerRadius::same(6);
    v.widgets.noninteractive.bg_fill = t.panel;
    v.widgets.noninteractive.weak_bg_fill = t.card;
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, t.border);
    v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.noninteractive.corner_radius = r;
    v.widgets.inactive.bg_fill = t.card;
    v.widgets.inactive.weak_bg_fill = t.card;
    v.widgets.inactive.bg_stroke = Stroke::new(1.0, t.border);
    v.widgets.inactive.fg_stroke = Stroke::new(1.0, t.text);
    v.widgets.inactive.corner_radius = r;
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_stroke = Stroke::new(1.0, t.border_strong);
    v.widgets.hovered.fg_stroke = Stroke::new(1.5, t.text);
    v.widgets.hovered.corner_radius = r;
    v.widgets.hovered.expansion = 0.0;
    v.widgets.active.bg_fill = t.pressed;
    v.widgets.active.weak_bg_fill = t.pressed;
    v.widgets.active.bg_stroke = Stroke::new(1.0, t.accent);
    v.widgets.active.fg_stroke = Stroke::new(1.5, t.text);
    v.widgets.active.corner_radius = r;
    v.widgets.active.expansion = 0.0;
    v.widgets.open.bg_fill = t.pressed;
    v.widgets.open.weak_bg_fill = t.pressed;
    v.widgets.open.corner_radius = r;
    v.slider_trailing_fill = true;
    v.striped = false;
    v.popup_shadow = egui::Shadow { offset: [0, 4], blur: 16, spread: 0, color: Color32::from_black_alpha(if t.dark { 120 } else { 40 }) };
    v.window_shadow = egui::Shadow { offset: [0, 8], blur: 28, spread: 0, color: Color32::from_black_alpha(if t.dark { 150 } else { 50 }) };
    v
}

/// Base colour of a Type Group. Custom Types and unknown types share the "custom" colour.
pub fn group_color(group: Option<&str>) -> Color32 {
    match group {
        Some("Identity") => Color32::from_rgb(0x5B, 0x9C, 0xF5),
        Some("Contact") => Color32::from_rgb(0x4C, 0xC3, 0x8A),
        Some("Identifiers") => Color32::from_rgb(0xF2, 0xA1, 0x3B),
        Some("Sensitive") => Color32::from_rgb(0xEF, 0x5F, 0x7A),
        _ => Color32::from_rgb(0xB0, 0x7C, 0xF5),
    }
}

/// Colour for an Entity Type, through its Type Group.
pub fn type_color(catalog: &TypeCatalog, t: &EntityType) -> Color32 {
    group_color(catalog.group_of(t).map(|g| g.name.as_str()))
}

/// Translucent fill for highlighting a span of that colour over text or a page.
pub fn highlight_fill(c: Color32, dark: bool) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), if dark { 0x55 } else { 0x48 })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_group_colours_are_stable_and_distinct() {
        let c = TypeCatalog::builtin();
        // Same type, same colour, every time; types of one group share it.
        assert_eq!(type_color(&c, &EntityType::new("person")), type_color(&c, &EntityType::new("Person")));
        assert_eq!(type_color(&c, &EntityType::new("person")), type_color(&c, &EntityType::new("location")));
        assert_eq!(type_color(&c, &EntityType::new("email")), Color32::from_rgb(0x4C, 0xC3, 0x8A));
        assert_eq!(type_color(&c, &EntityType::new("iban")), Color32::from_rgb(0xF2, 0xA1, 0x3B));
        // Custom types fall back to the custom colour.
        assert_eq!(type_color(&c, &EntityType::new("project code name")), group_color(None));
        // Every built-in group gets its own colour.
        let mut colours: Vec<_> = c.groups.iter().map(|g| group_color(Some(&g.name))).collect();
        colours.push(group_color(None));
        let n = colours.len();
        colours.sort_by_key(|c| c.to_array());
        colours.dedup();
        assert_eq!(colours.len(), n);
    }
}
