//! Splitting long inputs into pieces a detector can handle, and mapping offsets back.
//!
//! A [`ChunkPlan`] is a list of byte ranges that together cover the whole input. Chunks never
//! split a word, prefer paragraph boundaries, then sentence boundaries, and only as a last
//! resort cut at a word boundary. Chunk boundaries are invisible to the rest of the pipeline:
//! candidates found inside a chunk are shifted by the chunk's start.

use unicode_segmentation::UnicodeSegmentation;

use crate::types::Span;

/// One piece of the input, as a byte range.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chunk {
    pub span: Span,
}

impl Chunk {
    pub fn text<'t>(&self, input: &'t str) -> &'t str {
        &input[self.span.range()]
    }
}

/// How an input is cut into chunks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChunkPlan {
    pub chunks: Vec<Chunk>,
}

impl ChunkPlan {
    /// Build a plan where no chunk holds more than `max_words` whitespace-separated words.
    ///
    /// Strategy, in order: paragraphs (split on blank lines or single newlines), then sentences
    /// within an oversized paragraph (Unicode sentence boundaries), then hard word cuts within
    /// an oversized sentence. Consecutive small pieces are packed together greedily so the model
    /// sees as much context as possible.
    pub fn new(input: &str, max_words: usize) -> Self {
        let max_words = max_words.max(1);
        if input.trim().is_empty() {
            return ChunkPlan { chunks: vec![] };
        }
        let mut pieces: Vec<Span> = Vec::new();
        for para in paragraph_spans(input) {
            let words = word_count(&input[para.range()]);
            if words <= max_words {
                pieces.push(para);
                continue;
            }
            for sent in sentence_spans(input, para) {
                let words = word_count(&input[sent.range()]);
                if words <= max_words {
                    pieces.push(sent);
                } else {
                    pieces.extend(word_cuts(input, sent, max_words));
                }
            }
        }
        // Greedy packing: merge adjacent pieces while under the limit. Pieces are contiguous or
        // separated only by whitespace, so a merged span covers everything in between.
        let mut chunks: Vec<Span> = Vec::new();
        for p in pieces {
            if let Some(last) = chunks.last_mut() {
                let merged = Span::new(last.start, p.end);
                if word_count(&input[merged.range()]) <= max_words {
                    *last = merged;
                    continue;
                }
            }
            chunks.push(p);
        }
        ChunkPlan { chunks: chunks.into_iter().map(|span| Chunk { span }).collect() }
    }

    /// A single chunk covering the whole input (for detectors without a limit).
    pub fn whole(input: &str) -> Self {
        if input.is_empty() {
            return ChunkPlan { chunks: vec![] };
        }
        ChunkPlan { chunks: vec![Chunk { span: Span::new(0, input.len()) }] }
    }

    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }
}

/// Number of whitespace-separated words.
pub fn word_count(s: &str) -> usize {
    s.split_whitespace().count()
}

/// Byte spans of non-empty paragraphs. A paragraph ends at any `\n` (Windows `\r\n` included);
/// blank lines simply produce no paragraph. Leading and trailing whitespace is trimmed off each
/// paragraph so chunk text never starts with a newline.
fn paragraph_spans(input: &str) -> Vec<Span> {
    let mut out = Vec::new();
    let mut start = 0usize;
    for (i, b) in input.bytes().enumerate() {
        if b == b'\n' {
            push_trimmed(input, start, i, &mut out);
            start = i + 1;
        }
    }
    push_trimmed(input, start, input.len(), &mut out);
    out
}

fn push_trimmed(input: &str, start: usize, end: usize, out: &mut Vec<Span>) {
    let raw = &input[start..end];
    let lead = raw.len() - raw.trim_start().len();
    let trail = raw.len() - raw.trim_end().len();
    if lead + trail >= raw.len() {
        return;
    }
    out.push(Span::new(start + lead, end - trail));
}

/// Unicode sentence boundaries inside `within`, trimmed, non-empty.
fn sentence_spans(input: &str, within: Span) -> Vec<Span> {
    let text = &input[within.range()];
    let mut out = Vec::new();
    for (off, sent) in text.split_sentence_bound_indices() {
        let abs = within.start + off;
        push_trimmed(input, abs, abs + sent.len(), &mut out);
    }
    out
}

/// Cut `within` into runs of at most `max_words` words at word boundaries.
fn word_cuts(input: &str, within: Span, max_words: usize) -> Vec<Span> {
    let text = &input[within.range()];
    let mut out = Vec::new();
    let mut words_in_piece = 0usize;
    let mut piece_start: Option<usize> = None;
    let mut piece_end = 0usize;
    let mut i = 0usize;
    let bytes = text.as_bytes();
    while i < bytes.len() {
        // skip whitespace
        let mut j = i;
        while j < bytes.len() && (bytes[j] as char).is_ascii_whitespace() {
            j += 1;
        }
        // Non-ASCII whitespace is rare in documents; treat via str API for correctness.
        let rest = &text[j..];
        let skip = rest.len() - rest.trim_start().len();
        let j = j + skip;
        if j >= bytes.len() {
            break;
        }
        // word
        let rest = &text[j..];
        let wlen = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let (ws, we) = (within.start + j, within.start + j + wlen);
        if piece_start.is_none() {
            piece_start = Some(ws);
        }
        words_in_piece += 1;
        piece_end = we;
        if words_in_piece >= max_words {
            out.push(Span::new(piece_start.take().unwrap_or(ws), piece_end));
            words_in_piece = 0;
        }
        i = j + wlen;
    }
    if let Some(s) = piece_start {
        out.push(Span::new(s, piece_end));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::{prop, prop_assert, prop_assert_eq, proptest};

    fn plan_texts(input: &str, max: usize) -> Vec<&str> {
        ChunkPlan::new(input, max).chunks.iter().map(|c| c.text(input)).collect()
    }

    #[test]
    fn empty_input_has_no_chunks() {
        assert!(ChunkPlan::new("", 10).is_empty());
        assert!(ChunkPlan::new("  \n\n ", 10).is_empty());
        assert!(ChunkPlan::whole("").is_empty());
    }

    #[test]
    fn short_input_is_one_chunk() {
        let t = "Max Mustermann wohnt in Bonn.";
        assert_eq!(plan_texts(t, 384), vec![t]);
    }

    #[test]
    fn paragraphs_pack_until_limit() {
        let t = "one two three\n\nfour five\n\nsix seven eight nine";
        // limit 5: "one two three" + "four five" = 5 words → packed; then the 4-word para alone
        assert_eq!(plan_texts(t, 5), vec!["one two three\n\nfour five", "six seven eight nine"]);
    }

    #[test]
    fn long_paragraph_splits_on_sentences() {
        let t = "Alpha beta gamma. Delta epsilon zeta. Eta theta.";
        assert_eq!(plan_texts(t, 4), vec!["Alpha beta gamma.", "Delta epsilon zeta.", "Eta theta."]);
    }

    #[test]
    fn long_sentence_splits_on_words() {
        let t = "a b c d e f g";
        assert_eq!(plan_texts(t, 3), vec!["a b c", "d e f", "g"]);
    }

    #[test]
    fn chunks_never_start_or_end_with_whitespace_and_are_ordered() {
        let t = "  Hallo Welt.  \r\nZweite Zeile hier.\n\n\n Dritte.";
        let plan = ChunkPlan::new(t, 2);
        let mut prev_end = 0;
        for c in &plan.chunks {
            let s = c.text(t);
            assert_eq!(s, s.trim(), "chunk {s:?} not trimmed");
            assert!(c.span.start >= prev_end);
            prev_end = c.span.end;
        }
    }

    #[test]
    fn multibyte_text_keeps_char_boundaries() {
        let t = "Größe straße café. Ärger über Öl. 日本語 の テキスト です。";
        for max in 1..6 {
            for c in ChunkPlan::new(t, max).chunks {
                assert!(c.span.is_valid_in(t), "{:?} invalid for max {max}", c.span);
            }
        }
    }

    proptest! {
        #[test]
        fn every_word_is_covered_exactly_once(
            words in prop::collection::vec("[a-zäöüß]{1,8}|[A-Z][a-z]{1,5}\\.", 0..60),
            seps in prop::collection::vec(prop::sample::select(vec![" ", "  ", "\n", "\n\n", "\r\n"]), 0..60),
            max in 1usize..20,
        ) {
            let mut t = String::new();
            for (i, w) in words.iter().enumerate() {
                t.push_str(w);
                t.push_str(seps.get(i).map(|s| s.as_ref()).unwrap_or(" "));
            }
            let plan = ChunkPlan::new(&t, max);
            let mut covered: Vec<&str> = Vec::new();
            let mut prev_end = 0;
            for c in &plan.chunks {
                prop_assert!(c.span.is_valid_in(&t));
                prop_assert!(c.span.start >= prev_end, "chunks overlap or out of order");
                prev_end = c.span.end;
                prop_assert!(word_count(c.text(&t)) <= max, "chunk over limit: {:?}", c.text(&t));
                covered.extend(c.text(&t).split_whitespace());
            }
            let expected: Vec<&str> = t.split_whitespace().collect();
            prop_assert_eq!(covered, expected);
        }
    }
}
