//! Conversions between egui's character cursors and the byte offsets the core uses.

use pocketanon_core::Span;

/// Byte offset of the `char_idx`-th character of `text` (clamped to `text.len()`).
pub fn char_to_byte(text: &str, char_idx: usize) -> usize {
    text.char_indices().nth(char_idx).map_or(text.len(), |(b, _)| b)
}

/// Character index of byte offset `byte` (rounded down to a char boundary).
#[cfg_attr(not(test), allow(dead_code))]
pub fn byte_to_char(text: &str, byte: usize) -> usize {
    let mut b = byte.min(text.len());
    while !text.is_char_boundary(b) {
        b -= 1;
    }
    text[..b].chars().count()
}

/// The byte span of a selection between two char cursors (in either order). `None` when empty.
pub fn selection_span(text: &str, a: usize, b: usize) -> Option<Span> {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    let span = Span::new(char_to_byte(text, lo), char_to_byte(text, hi));
    (!span.is_empty()).then_some(span)
}

/// The selection with surrounding whitespace removed, so a sloppy double-click still yields a
/// clean entity.
pub fn trim_span(text: &str, span: Span) -> Option<Span> {
    let s = &text[span.range()];
    let lead = s.len() - s.trim_start().len();
    let trail = s.len() - s.trim_end().len();
    let out = Span::new(span.start + lead, span.end - trail);
    (out.start < out.end).then_some(out)
}

/// A short single-line preview of a span for list rows.
pub fn preview(text: &str, max_chars: usize) -> String {
    let one_line: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.chars().count() <= max_chars {
        one_line
    } else {
        let cut: String = one_line.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{cut}…")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_byte_round_trip_with_umlauts_and_emoji() {
        let t = "Grüße 👋🏽 an Jürgen";
        for (ci, (bi, _)) in t.char_indices().enumerate() {
            assert_eq!(char_to_byte(t, ci), bi);
            assert_eq!(byte_to_char(t, bi), ci);
        }
        let n = t.chars().count();
        assert_eq!(char_to_byte(t, n), t.len());
        assert_eq!(char_to_byte(t, n + 5), t.len());
        assert_eq!(byte_to_char(t, t.len()), n);
        // byte inside 'ü' rounds down
        assert_eq!(byte_to_char(t, 3), 2);
    }

    #[test]
    fn selection_maps_to_valid_byte_span() {
        let t = "Grüße 👋 an Jürgen Müller.";
        let start = t.chars().position(|c| c == 'J').unwrap();
        let end = start + "Jürgen Müller".chars().count();
        let span = selection_span(t, end, start).unwrap(); // reversed selection
        assert!(span.is_valid_in(t));
        assert_eq!(&t[span.range()], "Jürgen Müller");
        assert!(selection_span(t, 4, 4).is_none());
        let padded = selection_span(t, start - 1, end).unwrap();
        assert_eq!(&t[trim_span(t, padded).unwrap().range()], "Jürgen Müller");
    }

    #[test]
    fn preview_truncates_on_chars() {
        assert_eq!(preview("ä\nb  c", 10), "ä b c");
        assert_eq!(preview("äöüäöü", 4), "äöü…");
    }
}
