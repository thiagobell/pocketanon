//! Plain text kept byte-exact (ADR-0001: TXT fidelity is exact).
//!
//! Wordcraft's own TXT import/export normalises line ends to CRLF, drops the BOM and control
//! characters and always writes UTF-8, so it cannot promise an exact round trip. Here the text
//! stays authoritative: lines plus their exact terminators, re-encoded in the encoding they came
//! in. Wordcraft only sees a copy for the page preview.

use encoding_rs::Encoding;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TextEncoding {
    Utf8 { bom: bool },
    Utf16 { le: bool },
    /// A legacy encoding guessed by chardetng (Windows-1252, ISO-8859-x, Shift_JIS, ...).
    Legacy(&'static Encoding),
}

/// A plain-text file as lines (without terminators) and the exact terminator after each.
#[derive(Clone, Debug)]
pub(crate) struct PlainText {
    pub lines: Vec<String>,
    /// `"\n"`, `"\r\n"`, `"\r\r\n"`, ... or `""` for the last line.
    ends: Vec<String>,
    encoding: TextEncoding,
}

impl PlainText {
    pub fn decode(bytes: &[u8]) -> PlainText {
        let (text, encoding) = decode(bytes);
        let mut lines = Vec::new();
        let mut ends = Vec::new();
        let mut rest = text.as_str();
        loop {
            match rest.find('\n') {
                Some(i) => {
                    let line = &rest[..i];
                    let body = line.trim_end_matches('\r');
                    lines.push(body.to_string());
                    ends.push(format!("{}\n", &line[body.len()..]));
                    rest = &rest[i + 1..];
                }
                None => {
                    lines.push(rest.to_string());
                    ends.push(String::new());
                    break;
                }
            }
        }
        PlainText { lines, ends, encoding }
    }

    pub fn text(&self) -> String {
        let mut out = String::new();
        for (l, e) in self.lines.iter().zip(&self.ends) {
            out.push_str(l);
            out.push_str(e);
        }
        out
    }

    pub fn encode(&self) -> Vec<u8> {
        let text = self.text();
        match self.encoding {
            TextEncoding::Utf8 { bom } => {
                let mut out = Vec::with_capacity(text.len() + 3);
                if bom {
                    out.extend_from_slice(&[0xEF, 0xBB, 0xBF]);
                }
                out.extend_from_slice(text.as_bytes());
                out
            }
            TextEncoding::Utf16 { le } => {
                let mut out = Vec::with_capacity(text.len() * 2 + 2);
                out.extend_from_slice(if le { &[0xFF, 0xFE] } else { &[0xFE, 0xFF] });
                for u in text.encode_utf16() {
                    out.extend_from_slice(&if le { u.to_le_bytes() } else { u.to_be_bytes() });
                }
                out
            }
            // Placeholders are ASCII, so everything a replacement adds is encodable; unmappable
            // characters can only come from the source and become numeric references.
            TextEncoding::Legacy(enc) => enc.encode(&text).0.into_owned(),
        }
    }
}

fn decode(b: &[u8]) -> (String, TextEncoding) {
    if let Some(rest) = b.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        return (String::from_utf8_lossy(rest).into_owned(), TextEncoding::Utf8 { bom: true });
    }
    for (bom, le) in [([0xFF, 0xFE], true), ([0xFE, 0xFF], false)] {
        if let Some(rest) = b.strip_prefix(&bom) {
            let units: Vec<u16> =
                rest.chunks_exact(2).map(|c| if le { u16::from_le_bytes([c[0], c[1]]) } else { u16::from_be_bytes([c[0], c[1]]) }).collect();
            return (String::from_utf16_lossy(&units), TextEncoding::Utf16 { le });
        }
    }
    if let Ok(s) = std::str::from_utf8(b) {
        return (s.to_string(), TextEncoding::Utf8 { bom: false });
    }
    let mut det = chardetng::EncodingDetector::new(chardetng::Iso2022JpDetection::Deny);
    det.feed(b, true);
    let enc = det.guess(None, chardetng::Utf8Detection::Deny);
    (enc.decode_without_bom_handling(b).0.into_owned(), TextEncoding::Legacy(enc))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(b: &[u8]) {
        assert_eq!(PlainText::decode(b).encode(), b);
    }

    #[test]
    fn exact_round_trips() {
        roundtrip(b"");
        roundtrip(b"one line");
        roundtrip(b"a\nb\n");
        roundtrip(b"a\r\nb\r\n\r\n");
        roundtrip(b"mixed\r\nends\nand\r\r\nlone\rcr\x0cff");
        roundtrip("\u{FEFF}Grüße".as_bytes());
        roundtrip(b"\xEF\xBB\xBFwith bom\r\n");
        roundtrip(b"\xFF\xFEa\x00\n\x00");
        roundtrip(b"Gr\xfc\xdfe aus K\xf6ln\r\n"); // Windows-1252
    }

    #[test]
    fn lines_and_terminators() {
        let t = PlainText::decode(b"a\r\nb\nc");
        assert_eq!(t.lines, ["a", "b", "c"]);
        let t = PlainText::decode(b"x\n");
        assert_eq!(t.lines, ["x", ""]);
    }
}
