//! MaxVisibleGraphemes: which part of each laid-out line is drawn. Measured on
//! fixtures/text-probe-graphemes.luau (notes/text.md "MaxVisibleGraphemes"): Studio draws only the
//! glyphs of the first N graphemes of ContentText (tags do not count; "\n" and the space a line breaks at
//! do), the layout (line breaks, TextScaled size, TextBounds, pens) staying the full text's. A truncated
//! line that is reached (its first grapheme index <= N) draws its ellipsis right after its visible
//! prefix, even at N 0; a line past the cut draws nothing.

use super::TextEngine;
use super::layout::{Line, TextLayout};
use super::utf8::codepoints;

/// How much of one line is drawn when MaxVisibleGraphemes hides part of it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineReveal {
    /// Leading glyphs of `line.glyphs` drawn (spaces included).
    pub visible: usize,
    /// Pen x (glyph x units) where the hidden part starts: decorations end there and a drawn ellipsis
    /// starts there.
    pub cut: f64,
    /// Trailing glyphs of `line.glyphs` that are the truncation ellipsis.
    pub ellipsis: usize,
    /// Whether that ellipsis is drawn (moved to `cut`).
    pub ellipsis_drawn: bool,
}

/// The rule of the emulator's utf8.graphemes (roblox-engine-emulator src/lua/prelude.luau): a code point
/// joins the previous cluster when it extends it (combining marks, variation selectors, skin tones, tags,
/// ZWJ), follows a ZWJ, is the LF of a CRLF, or completes a regional indicator pair.
fn extends(code: u32) -> bool {
    matches!(code, 0x0300..=0x036F | 0x1AB0..=0x1AFF | 0x1DC0..=0x1DFF | 0x20D0..=0x20FF | 0xFE00..=0xFE0F | 0xFE20..=0xFE2F)
        || matches!(code, 0x1F3FB..=0x1F3FF | 0xE0020..=0xE007F | 0xE0100..=0xE01EF)
        || code == 0x200D
}

/// (1-based byte position, 0-based grapheme index) of every code point of `text`, by position.
pub fn grapheme_indices(text: &[u8]) -> Vec<(u32, u32)> {
    let points = codepoints(text);
    let mut out = Vec::with_capacity(points.len());
    let mut index: Option<u32> = None;
    let mut previous: Option<u32> = None;
    let mut joining = false;
    let mut flags = 0u32;
    for (code, position) in points {
        let mut join = previous.is_some() && (extends(code) || joining || (previous == Some(13) && code == 10));
        if (0x1F1E6..=0x1F1FF).contains(&code) {
            join = join || flags % 2 == 1;
            flags += 1;
        } else {
            flags = 0;
        }
        let current = match index {
            Some(current) if join => current,
            Some(current) => current + 1,
            None => 0,
        };
        index = Some(current);
        out.push((position, current));
        joining = code == 0x200D;
        previous = Some(code);
    }
    out
}

fn grapheme_at(indices: &[(u32, u32)], position: u32) -> u32 {
    match indices.binary_search_by_key(&position, |entry| entry.0) {
        Ok(found) => indices[found].1,
        Err(0) => 0,
        Err(after) => indices[after - 1].1,
    }
}

impl TextEngine {
    /// Trailing glyphs of a truncated line that are the layout's ellipsis (TextEngine::ellipsis, appended by
    /// truncation; plain text only: rich truncation appends none).
    fn ellipsis_glyphs(&self, layout: &TextLayout, line: &Line) -> usize {
        if !line.truncated {
            return 0;
        }
        let Some(face) = layout.face else { return 0 };
        let Some(ellipsis) = self.ellipsis_cache.get(&(face, layout.size.to_bits())) else { return 0 };
        let count = ellipsis.0.len();
        if count == 0 || line.glyphs.len() < count {
            return 0;
        }
        let tail = &line.glyphs[line.glyphs.len() - count..];
        let same = tail
            .iter()
            .zip(&ellipsis.0)
            .all(|(a, b)| a.run == 0 && a.glyph == b.glyph && a.face == b.face && a.cluster == b.cluster && a.code == b.code);
        if same { count } else { 0 }
    }

    /// Per line, what MaxVisibleGraphemes `max` leaves drawn (None: the line is drawn whole); None when
    /// nothing is hidden (`max` < 0, or every laid-out grapheme is visible). `source` is the text laid out
    /// (DisplayText; a rich layout indexes its runs' text, i.e. ContentText, instead).
    pub fn reveal(&self, layout: &TextLayout, source: &[u8], max: f64) -> Option<Vec<Option<LineReveal>>> {
        if !(max >= 0.0) {
            return None;
        }
        let rich_content;
        let (content, starts): (&[u8], Vec<u32>) = match &layout.runs {
            Some(runs) => {
                let mut content = Vec::new();
                let mut starts = Vec::with_capacity(runs.len());
                for run in runs.iter() {
                    starts.push(content.len() as u32);
                    content.extend_from_slice(&run.text);
                }
                rich_content = content;
                (&rich_content, starts)
            }
            None => (source, Vec::new()),
        };
        let indices = grapheme_indices(content);
        let max = max.floor();
        let mut out = Vec::with_capacity(layout.lines.len());
        let mut hidden = false;
        // The grapheme after the last one laid out so far: where a line without glyphs of its own starts
        let mut next = 0u32;
        for line in &layout.lines {
            let ellipsis = self.ellipsis_glyphs(layout, line);
            let own = &line.glyphs[..line.glyphs.len() - ellipsis];
            let mut first = None;
            let mut cut = None;
            for (index, glyph) in own.iter().enumerate() {
                let offset = match glyph.run {
                    0 => 0,
                    run => starts.get(run as usize - 1).copied().unwrap_or(0),
                };
                let grapheme = grapheme_at(&indices, offset + glyph.cluster);
                first.get_or_insert(grapheme);
                next = grapheme + 1;
                if cut.is_none() && grapheme as f64 >= max {
                    cut = Some(index);
                }
            }
            let reached = first.unwrap_or(next) as f64 <= max;
            let cut = match cut {
                None if ellipsis > 0 && !reached => Some(own.len()),
                cut => cut,
            };
            match cut {
                None => out.push(None),
                Some(index) => {
                    hidden = true;
                    let at = line.glyphs.get(index).map_or(0.0, |glyph| glyph.x);
                    out.push(Some(LineReveal { visible: index, cut: at, ellipsis, ellipsis_drawn: ellipsis > 0 && reached }));
                }
            }
        }
        hidden.then_some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graphemes_follow_the_emulator_rule() {
        let indices = |text: &str| grapheme_indices(text.as_bytes()).into_iter().map(|entry| entry.1).collect::<Vec<_>>();
        assert_eq!(indices("ab"), vec![0, 1]);
        // e + combining acute is one grapheme; so is CRLF
        assert_eq!(indices("Cafe\u{301} x"), vec![0, 1, 2, 3, 3, 4, 5]);
        assert_eq!(indices("a\r\nb"), vec![0, 1, 1, 2]);
        // ZWJ sequences, skin tones and flag pairs
        assert_eq!(indices("\u{1F468}\u{200D}\u{1F469}!"), vec![0, 0, 0, 1]);
        assert_eq!(indices("\u{1F44D}\u{1F3FD}"), vec![0, 0]);
        assert_eq!(indices("\u{1F1F8}\u{1F1EA}\u{1F1EB}\u{1F1EE}"), vec![0, 0, 1, 1]);
        let positions = grapheme_indices("é!".as_bytes());
        assert_eq!(grapheme_at(&positions, 1), 0);
        assert_eq!(grapheme_at(&positions, 2), 0);
        assert_eq!(grapheme_at(&positions, 3), 1);
    }
}
