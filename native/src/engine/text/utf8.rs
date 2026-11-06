//! `TextLayout.Codepoints`: UTF-8 decoding exactly like Luau's `utf8.codes` (lutf8lib.cpp: overlong
//! forms, surrogates and code points above U+10FFFF are invalid); any invalid sequence makes the whole
//! text fall back to one code point per byte.

/// A decoded code point and its 1-based byte position in the source (the cluster).
pub type Codepoint = (u32, u32);

#[inline]
fn byte_at(text: &[u8], index: usize) -> u32 {
    // Luau strings are NUL terminated: reading past the end sees 0.
    text.get(index).copied().unwrap_or(0) as u32
}

#[inline]
fn is_continuation(text: &[u8], index: usize) -> bool {
    byte_at(text, index) & 0xC0 == 0x80
}

/// utf8_decode: the code point and the index after it, or None for an invalid sequence.
fn decode(text: &[u8], start: usize) -> Option<(u32, usize)> {
    const LIMITS: [u32; 4] = [0xFF, 0x7F, 0x7FF, 0xFFFF];
    let mut c = byte_at(text, start);
    if c < 0x80 {
        return Some((c, start + 1));
    }
    let mut result: u32 = 0;
    let mut count = 0usize;
    while c & 0x40 != 0 {
        count += 1;
        let next = byte_at(text, start + count);
        if next & 0xC0 != 0x80 {
            return None;
        }
        result = (result << 6) | (next & 0x3F);
        c <<= 1;
    }
    if count > 3 {
        return None;
    }
    result |= (c & 0x7F) << (count * 5);
    if result > 0x10FFFF || result <= LIMITS[count] {
        return None;
    }
    if result.wrapping_sub(0xD800) < 0x800 {
        return None;
    }
    Some((result, start + count + 1))
}

/// Decodes into (code, 1-based byte position) pairs, appending to `out`.
pub fn codepoints_into(text: &[u8], out: &mut Vec<Codepoint>) {
    let base = out.len();
    let mut index = 0;
    while index < text.len() {
        match decode(text, index) {
            Some((code, next)) if !(next < text.len() && is_continuation(text, next)) => {
                out.push((code, index as u32 + 1));
                index = next;
            }
            _ => {
                out.truncate(base);
                out.extend(text.iter().enumerate().map(|(position, &byte)| (byte as u32, position as u32 + 1)));
                return;
            }
        }
    }
}

pub fn codepoints(text: &[u8]) -> Vec<Codepoint> {
    let mut out = Vec::with_capacity(text.len());
    codepoints_into(text, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_like_luau() {
        assert_eq!(codepoints("aé".as_bytes()), vec![(97, 1), (233, 2)]);
        // A surrogate is invalid: every byte becomes a code point.
        assert_eq!(codepoints(b"a\xED\xA0\x80"), vec![(97, 1), (0xED, 2), (0xA0, 3), (0x80, 4)]);
        assert_eq!(codepoints(b"\xC0\x80"), vec![(0xC0, 1), (0x80, 2)]);
        assert_eq!(codepoints("😀".as_bytes()), vec![(0x1F600, 1)]);
    }
}
