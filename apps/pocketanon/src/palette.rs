//! The Cmd+K command palette: a fuzzy filter over the app's actions.

use egui::{Align2, CornerRadius, Frame, Key, Margin, RichText, Stroke};

use crate::theme::Tokens;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    Run,
    Apply,
    CopyResult,
    OpenFile,
    SaveResult,
    ToggleLegend,
    OpenSettings,
    SwitchWorkspace,
    Clear,
}

impl Action {
    pub const ALL: [Action; 9] = [
        Action::Run,
        Action::Apply,
        Action::CopyResult,
        Action::OpenFile,
        Action::SaveResult,
        Action::ToggleLegend,
        Action::OpenSettings,
        Action::SwitchWorkspace,
        Action::Clear,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Action::Run => "Run detection",
            Action::Apply => "Apply (Indexed Redaction)",
            Action::CopyResult => "Copy result",
            Action::OpenFile => "Open file…",
            Action::SaveResult => "Save result (Apply & Save)",
            Action::ToggleLegend => "Show or hide the Legend",
            Action::OpenSettings => "Open settings",
            Action::SwitchWorkspace => "Switch workspace",
            Action::Clear => "Clear",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Action::Run => "⌘↩",
            Action::Apply => "⌘ Shift ↩",
            Action::OpenFile => "⌘O",
            Action::SaveResult => "⌘S",
            Action::OpenSettings => "⌘,",
            Action::ToggleLegend => "⌘L",
            Action::SwitchWorkspace => "⌘1 / ⌘2",
            _ => "",
        }
    }

    /// Extra words the filter matches on.
    fn keywords(self) -> &'static str {
        match self {
            Action::Run => "detect scan find",
            Action::Apply => "anonymize replace redact",
            Action::CopyResult => "clipboard",
            Action::OpenFile => "load document file anonymization",
            Action::SaveResult => "export write",
            Action::ToggleLegend => "legend entity types languages term rules colors",
            Action::OpenSettings => "preferences settings model language types",
            Action::SwitchWorkspace => "text file anonymization",
            Action::Clear => "reset new",
        }
    }
}

/// Subsequence match score of `query` against `hay` (case-insensitive). Higher is better;
/// `None` when not every query character appears in order. Rewards consecutive runs and
/// matches at word starts.
pub fn fuzzy_score(query: &str, hay: &str) -> Option<i32> {
    let q: Vec<char> = query.to_lowercase().chars().filter(|c| !c.is_whitespace()).collect();
    if q.is_empty() {
        return Some(0);
    }
    let h: Vec<char> = hay.to_lowercase().chars().collect();
    let mut score = 0;
    let mut qi = 0;
    let mut prev_match: Option<usize> = None;
    for (hi, &c) in h.iter().enumerate() {
        if qi < q.len() && c == q[qi] {
            score += 1;
            if prev_match == Some(hi.wrapping_sub(1)) {
                score += 5;
            }
            if hi == 0 || !h[hi - 1].is_alphanumeric() {
                score += 8;
            }
            prev_match = Some(hi);
            qi += 1;
        }
    }
    (qi == q.len()).then(|| score - (h.len() as i32 / 8))
}

/// Actions matching `query`, best first. Label matches beat keyword matches.
pub fn filter(query: &str) -> Vec<Action> {
    let mut scored: Vec<(i32, usize, Action)> = Action::ALL
        .iter()
        .enumerate()
        .filter_map(|(i, &a)| {
            let label = fuzzy_score(query, a.label()).map(|s| s + 100);
            let kw = fuzzy_score(query, &format!("{} {}", a.label(), a.keywords()));
            label.or(kw).map(|s| (s, i, a))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, a)| a).collect()
}

#[derive(Default)]
pub struct Palette {
    pub open: bool,
    query: String,
    selected: usize,
    focus_pending: bool,
}

impl Palette {
    pub fn toggle(&mut self) {
        if self.open {
            self.open = false;
        } else {
            self.open = true;
            self.query.clear();
            self.selected = 0;
            self.focus_pending = true;
        }
    }

    /// Draw the palette; returns the chosen action.
    pub fn show(&mut self, ctx: &egui::Context) -> Option<Action> {
        if !self.open {
            return None;
        }
        let tk = Tokens::current(ctx);
        let matches = filter(&self.query);
        self.selected = self.selected.min(matches.len().saturating_sub(1));
        let (up, down, enter, esc) = ctx.input_mut(|i| {
            (
                i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                i.consume_key(egui::Modifiers::NONE, Key::Enter),
                i.consume_key(egui::Modifiers::NONE, Key::Escape),
            )
        });
        if esc {
            self.open = false;
            return None;
        }
        if up {
            self.selected = self.selected.saturating_sub(1);
        }
        if down && self.selected + 1 < matches.len() {
            self.selected += 1;
        }
        let mut chosen = enter.then(|| matches.get(self.selected).copied()).flatten();

        // Dim the app behind the palette; clicking outside closes it.
        let screen = ctx.content_rect();
        let backdrop = egui::Area::new(egui::Id::new("palette_backdrop"))
            .order(egui::Order::Middle)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let r = ui.allocate_rect(screen, egui::Sense::click());
                ui.painter().rect_filled(screen, 0.0, egui::Color32::from_black_alpha(110));
                r
            })
            .inner;
        if backdrop.clicked() {
            self.open = false;
        }

        egui::Area::new(egui::Id::new("palette"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_TOP, egui::vec2(0.0, 96.0))
            .show(ctx, |ui| {
                Frame::new()
                    .fill(tk.menu)
                    .stroke(Stroke::new(1.0, tk.border_strong))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(Margin::same(8))
                    .shadow(ctx.global_style().visuals.window_shadow)
                    .show(ui, |ui| {
                        ui.set_width(520.0);
                        ui.horizontal(|ui| {
                            ui.add_space(8.0);
                            ui.label(RichText::new("Options (Cmd K)").size(11.0).strong().color(tk.text_faint));
                        });
                        let edit = egui::TextEdit::singleline(&mut self.query)
                            .hint_text("Type a command…")
                            .frame(egui::Frame::NONE)
                            .margin(Margin::symmetric(8, 8))
                            .font(egui::FontId::proportional(16.0))
                            .desired_width(f32::INFINITY);
                        let r = ui.add(edit);
                        if self.focus_pending {
                            r.request_focus();
                            self.focus_pending = false;
                        }
                        if r.changed() {
                            self.selected = 0;
                        }
                        ui.separator();
                        if matches.is_empty() {
                            ui.add_space(6.0);
                            ui.label(RichText::new("  No matching command").color(tk.text_faint));
                            ui.add_space(6.0);
                        }
                        for (i, a) in matches.iter().enumerate() {
                            let sel = i == self.selected;
                            let (rect, resp) = ui.allocate_exact_size(egui::vec2(ui.available_width(), 32.0), egui::Sense::click());
                            if resp.hovered() {
                                self.selected = i;
                            }
                            if sel || resp.hovered() {
                                ui.painter().rect_filled(rect, CornerRadius::same(6), tk.accent_soft);
                            }
                            ui.painter().text(
                                rect.left_center() + egui::vec2(10.0, 0.0),
                                Align2::LEFT_CENTER,
                                a.label(),
                                egui::FontId::proportional(14.0),
                                tk.text,
                            );
                            ui.painter().text(
                                rect.right_center() - egui::vec2(10.0, 0.0),
                                Align2::RIGHT_CENTER,
                                a.hint(),
                                egui::FontId::proportional(12.0),
                                tk.text_faint,
                            );
                            if resp.clicked() {
                                chosen = Some(*a);
                            }
                        }
                    });
            });
        if chosen.is_some() {
            self.open = false;
        }
        chosen
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_lists_everything_in_order() {
        assert_eq!(filter(""), Action::ALL.to_vec());
    }

    #[test]
    fn subsequence_and_ranking() {
        assert_eq!(filter("copy")[0], Action::CopyResult);
        assert_eq!(filter("opfi")[0], Action::OpenFile);
        assert_eq!(filter("settings")[0], Action::OpenSettings);
        assert_eq!(filter("legend")[0], Action::ToggleLegend);
        // keyword match
        assert!(filter("detect").contains(&Action::Run));
        assert!(filter("clipboard").contains(&Action::CopyResult));
        assert!(filter("zzzz").is_empty());
        assert!(fuzzy_score("ab", "a-b").is_some());
        assert!(fuzzy_score("ba", "ab").is_none());
        assert!(fuzzy_score("ap", "Apply").unwrap() > fuzzy_score("ap", "Clear app").unwrap());
    }
}
