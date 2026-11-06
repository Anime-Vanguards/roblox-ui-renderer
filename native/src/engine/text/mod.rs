//! Text engine (phase 1 of the native engine, notes/rust-engine.md "Text API"): a bit-exact port of
//! runtime/text (opentype.luau, fonts.luau, hinted.luau, richtext.luau, layout.luau) and of the text
//! measurement in runtime/layout/init.luau (`LayoutText`: TextScaled, UITextSizeConstraint, automatic
//! width). The Luau runtime's results (85 399 cases, frozen in `out/golden/text-corpus.json` when it
//! was retired) are replayed by `native/tests/text_corpus.rs`.
//!
//! Entry point: [`TextEngine`] (font registry + the Luau caches). One per engine; not thread-safe
//! (`&mut self`), results are shared `Arc`s: equal inputs return the identical `Arc` while cached, as the
//! Luau returns the identical table (incremental layout compares layouts by identity).

pub mod fonts;
pub mod hinted;
pub mod layout;
pub mod measure;
pub mod num;
pub mod opentype;
pub mod reveal;
pub mod richtext;
pub mod utf8;

use std::collections::HashMap;
use std::sync::Arc;

pub use fonts::{Face, FaceId, FontPaths, FontRegistry};
pub use layout::{
    Glyph, LayoutParams, Line, TextLayout, Truncate, XAlign, glyph_advance, is_space, line_pen, ppem, quad_size_for, rich_scaled_pixels, rich_size_for,
    scales, size_for, snap_pixel,
};
pub use opentype::Font;
pub use measure::{SizeConstraint, TextMeasure, TextObject};
pub use num::f32r;
pub use richtext::{Mark, Run, Stroke, Style};

/// FNV-1a style hasher for cache keys (hashes borrowed key parts without allocating).
pub(crate) struct KeyHasher(u64);

impl KeyHasher {
    pub(crate) fn new() -> Self {
        KeyHasher(0xcbf2_9ce4_8422_2325)
    }
    #[inline]
    pub(crate) fn bytes(&mut self, bytes: &[u8]) {
        let mut hash = self.0;
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            hash = (hash ^ u64::from_le_bytes(word)).wrapping_mul(0x0000_0100_0000_01b3).rotate_left(23);
        }
        self.0 = (hash ^ bytes.len() as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    #[inline]
    pub(crate) fn u64(&mut self, value: u64) {
        self.0 = (self.0 ^ value).wrapping_mul(0x0000_0100_0000_01b3).rotate_left(29);
    }
    pub(crate) fn finish(&self) -> u64 {
        self.0 ^ (self.0 >> 31)
    }
}

/// The measure cache key (`MeasureCache` in layout.luau), borrowed for lookups.
pub(crate) struct MeasureKey<'a> {
    pub family: &'a str,
    pub weight: u64,
    pub style: &'a str,
    pub size: u64,
    pub max_width: u64,
    pub max_height: u64,
    pub box_width: u64,
    pub line_factor: u64,
    pub truncate: Truncate,
    pub empty_height: u64,
    pub cache_key: &'a str,
    pub text: &'a [u8],
}

pub(crate) struct MeasureEntry {
    family: Box<str>,
    style: Box<str>,
    cache_key: Box<str>,
    text: Box<[u8]>,
    numbers: [u64; 8],
    truncate: Truncate,
    value: Arc<TextLayout>,
}

pub(crate) struct ShapeEntry {
    pub family: Box<str>,
    pub weight: u64,
    pub style: Box<str>,
    pub size: u64,
    pub text: Box<[u8]>,
    pub value: Arc<Vec<Vec<Glyph>>>,
}

pub(crate) struct RichShapeEntry {
    pub key: (u64, u64, u64, u64),
    pub family: Box<str>,
    pub style: Box<str>,
    pub text: Box<[u8]>,
    pub value: Arc<richtext::RichShaped>,
}

/// The text engine: font registry plus the layout caches of layout.luau / richtext.luau.
pub struct TextEngine {
    pub fonts: FontRegistry,
    measure_cache: HashMap<u64, Vec<MeasureEntry>>,
    measure_count: usize,
    pub(crate) shape_cache: HashMap<u64, Vec<ShapeEntry>>,
    pub(crate) shape_count: usize,
    pub(crate) rich_parse_cache: HashMap<Vec<u8>, Arc<richtext::ParsedEntry>>,
    pub(crate) rich_shape_cache: HashMap<u64, Vec<RichShapeEntry>>,
    pub(crate) rich_count: usize,
    pub(crate) ellipsis_cache: HashMap<(FaceId, u64), Arc<(Vec<Glyph>, f64)>>,
    pub(crate) scratch_codepoints: Vec<utf8::Codepoint>,
}

impl TextEngine {
    /// `Fonts.Preload` plus empty caches.
    pub fn new(paths: FontPaths) -> TextEngine {
        TextEngine::with_registry(FontRegistry::new(paths))
    }

    pub fn with_registry(fonts: FontRegistry) -> TextEngine {
        TextEngine {
            fonts,
            measure_cache: HashMap::new(),
            measure_count: 0,
            shape_cache: HashMap::new(),
            shape_count: 0,
            rich_parse_cache: HashMap::new(),
            rich_shape_cache: HashMap::new(),
            rich_count: 0,
            ellipsis_cache: HashMap::new(),
            scratch_codepoints: Vec::new(),
        }
    }

    /// Lays out plain or rich text (`TextLayout.Layout` / `RichText.Layout`).
    pub fn layout(&mut self, params: &LayoutParams, rich: bool) -> Arc<TextLayout> {
        if rich { self.layout_rich(params) } else { self.layout_plain(params) }
    }

    /// The face a glyph or layout refers to.
    pub fn face(&self, id: FaceId) -> &Arc<Face> {
        self.fonts.face(id)
    }

    fn measure_numbers(key: &MeasureKey) -> [u64; 8] {
        [key.weight, key.size, key.max_width, key.max_height, key.box_width, key.line_factor, key.empty_height, 0]
    }

    pub(crate) fn measure_lookup(&self, hash: u64, key: &MeasureKey) -> Option<Arc<TextLayout>> {
        let bucket = self.measure_cache.get(&hash)?;
        let numbers = Self::measure_numbers(key);
        bucket
            .iter()
            .find(|entry| {
                entry.numbers == numbers
                    && entry.truncate == key.truncate
                    && &*entry.family == key.family
                    && &*entry.style == key.style
                    && &*entry.cache_key == key.cache_key
                    && &*entry.text == key.text
            })
            .map(|entry| entry.value.clone())
    }

    pub(crate) fn measure_insert(&mut self, hash: u64, key: &MeasureKey, value: Arc<TextLayout>) {
        self.measure_count += 1;
        if self.measure_count > 20000 {
            self.measure_cache.clear();
            self.measure_count = 0;
        }
        self.measure_cache.entry(hash).or_default().push(MeasureEntry {
            family: key.family.into(),
            style: key.style.into(),
            cache_key: key.cache_key.into(),
            text: key.text.into(),
            numbers: Self::measure_numbers(key),
            truncate: key.truncate,
            value,
        });
    }
}
