//! What `rur render --placeholders` draws for an image the render could not get: a magenta and black
//! checkerboard (the usual "missing texture") with the asset id written across a black band, so a missing
//! icon reads as missing rather than as a gap in the layout. Studio draws nothing there, so this is off by
//! default. The file goes into the render directory's assets/, never the asset cache (cli/assets.mjs: no
//! stand-ins there), and the image's manifest entry keeps its error, so the summary still lists it missing.
use std::path::Path;

use anyhow::Result;

const SIZE: u32 = 128;
const SQUARE: u32 = 16;
const SCALE: u32 = 3;
const DIGITS_PER_ROW: usize = 8;
const MAGENTA: [u8; 4] = [255, 0, 255, 255];
const BLACK: [u8; 4] = [0, 0, 0, 255];
const WHITE: [u8; 4] = [255, 255, 255, 255];

/// 3 x 5 digits, a row per entry, its three bits left to right
const DIGITS: [[u8; 5]; 10] = [
    [0b111, 0b101, 0b101, 0b101, 0b111],
    [0b010, 0b110, 0b010, 0b010, 0b111],
    [0b111, 0b001, 0b111, 0b100, 0b111],
    [0b111, 0b001, 0b111, 0b001, 0b111],
    [0b101, 0b101, 0b111, 0b001, 0b001],
    [0b111, 0b100, 0b111, 0b001, 0b111],
    [0b111, 0b100, 0b111, 0b101, 0b111],
    [0b111, 0b001, 0b010, 0b010, 0b010],
    [0b111, 0b101, 0b111, 0b101, 0b111],
    [0b111, 0b101, 0b111, 0b001, 0b111],
];

/// The longest run of digits in a content URI (an asset id), or "" when it has none
pub fn label(uri: &str) -> String {
    uri.split(|character: char| !character.is_ascii_digit()).max_by_key(|run| run.len()).unwrap_or_default().to_string()
}

/// The placeholder's straight RGBA pixels, SIZE x SIZE, `label`'s digits on up to three rows of eight
pub fn pixels(label: &str) -> Vec<u8> {
    let mut rgba = vec![0u8; (SIZE * SIZE * 4) as usize];
    let mut put = |x: u32, y: u32, colour: [u8; 4]| {
        let at = ((y * SIZE + x) * 4) as usize;
        rgba[at..at + 4].copy_from_slice(&colour);
    };
    for y in 0..SIZE {
        for x in 0..SIZE {
            put(x, y, if (x / SQUARE + y / SQUARE) % 2 == 0 { MAGENTA } else { BLACK });
        }
    }
    let digits: Vec<usize> = label.bytes().filter(u8::is_ascii_digit).map(|byte| (byte - b'0') as usize).collect();
    let rows: Vec<&[usize]> = digits.chunks(DIGITS_PER_ROW).take(3).collect();
    if rows.is_empty() {
        return rgba;
    }
    let (glyph_width, glyph_height, gap) = (3 * SCALE, 5 * SCALE, 2 * SCALE);
    let band_height = 2 * gap + rows.len() as u32 * glyph_height + (rows.len() as u32 - 1) * gap;
    let band_top = SIZE - band_height;
    for y in band_top..SIZE {
        for x in 0..SIZE {
            put(x, y, BLACK);
        }
    }
    for (row, digits) in rows.iter().enumerate() {
        let width = digits.len() as u32 * glyph_width + (digits.len() as u32 - 1) * SCALE;
        let left = (SIZE - width) / 2;
        let top = band_top + gap + row as u32 * (glyph_height + gap);
        for (index, &digit) in digits.iter().enumerate() {
            let x0 = left + index as u32 * (glyph_width + SCALE);
            for (line, bits) in DIGITS[digit].iter().enumerate() {
                for column in 0..3 {
                    if bits & (0b100 >> column) == 0 {
                        continue;
                    }
                    for dy in 0..SCALE {
                        for dx in 0..SCALE {
                            put(x0 + column * SCALE + dx, top + line as u32 * SCALE + dy, WHITE);
                        }
                    }
                }
            }
        }
    }
    rgba
}

/// Writes the placeholder for `uri` into `assets` (once per label) and returns its file name
pub fn write(assets: &Path, uri: &str) -> Result<String> {
    let label = label(uri);
    let name = format!("missing-{}.png", if label.is_empty() { "image" } else { &label });
    let file = assets.join(&name);
    if !file.is_file() {
        crate::story_render::write_png(&file, &pixels(&label), SIZE, SIZE)?;
    }
    Ok(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(rgba: &[u8], x: u32, y: u32) -> [u8; 4] {
        let index = ((y * SIZE + x) * 4) as usize;
        rgba[index..index + 4].try_into().unwrap()
    }

    #[test]
    fn checkerboard_with_the_id_on_a_band() {
        assert_eq!(label("rbxassetid://117011682359398"), "117011682359398");
        assert_eq!(label("rbxthumb://type=Asset&id=42&w=150&h=150"), "150");
        let rgba = pixels("117011682359398");
        assert_eq!(rgba.len(), (SIZE * SIZE * 4) as usize);
        assert_eq!(at(&rgba, 0, 0), MAGENTA);
        assert_eq!(at(&rgba, SQUARE, 0), BLACK);
        // Two rows of digits: a band of 2 gaps, 2 rows and the gap between them, holding white
        let band = 2 * (2 * SCALE) + 2 * (5 * SCALE) + 2 * SCALE;
        assert_eq!(at(&rgba, 0, SIZE - band), BLACK);
        assert_eq!(at(&rgba, 0, SIZE - band - 1), if ((SIZE - band - 1) / SQUARE) % 2 == 0 { MAGENTA } else { BLACK });
        let white = (SIZE - band..SIZE).flat_map(|y| (0..SIZE).map(move |x| (x, y))).filter(|&(x, y)| at(&rgba, x, y) == WHITE).count();
        assert!(white > 0);
        // No digits: the checkerboard alone
        assert!(pixels("").chunks(4).all(|pixel| pixel == MAGENTA || pixel == BLACK));
    }
}
