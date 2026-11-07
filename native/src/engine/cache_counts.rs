//! The GUI layer's caches, counted while a profile asks for caches (`--profile caches`) through the emulator's cache
//! counters (its profiler::caches): the layout's records and AutomaticSize sizes, and the text engine's laid-out text,
//! shaped paragraphs and rich text. Off, a count costs one relaxed load; the caches are registered with the profile when
//! one starts ([`register`]) or at their first count.

use std::sync::OnceLock;

use super::profiler::caches;

/// One of the layer's caches
#[derive(Clone, Copy, Debug)]
pub enum Cache {
    /// A node's layout record: kept (the same inputs, or only moved) or laid out again
    Records,
    /// An AutomaticSize object's computed size: kept while its subtree and the inputs are the same
    Sizes,
    /// A text's lines, size and glyphs (TextLayout.Layout) per font, size, box and text
    Texts,
    /// A plain text's shaped paragraphs per font, size and text (a new box only wraps them again)
    Shapes,
    /// A rich text's shaped runs per font, size and text
    RichText,
}

const NAMES: [(&str, &str); 5] = [
    ("layout records", "a node's layout record (hit: kept, its inputs the same or only moved; miss: laid out again)"),
    ("layout sizes", "an AutomaticSize object's computed size, kept while its subtree and inputs are the same"),
    ("text layouts", "a text's lines, size and glyphs per font, size, box and text (eviction: the cache emptied when full)"),
    ("text shaping", "a plain text's shaped paragraphs per font, size and text (eviction: the cache emptied when full)"),
    ("rich text", "a rich text's shaped runs per font, size and text (eviction: the cache emptied when full)"),
];

fn ids() -> &'static [usize; 5] {
    static IDS: OnceLock<[usize; 5]> = OnceLock::new();
    IDS.get_or_init(|| NAMES.map(|(name, about)| caches::named(name, about)))
}

/// Lists the layer's caches with a profile that counts caches, so each has a row even before its first count
pub fn register() {
    ids();
}

#[inline]
pub fn hit(cache: Cache) {
    if caches::on() {
        caches::named_hit(ids()[cache as usize]);
    }
}

#[inline]
pub fn miss(cache: Cache) {
    if caches::on() {
        caches::named_miss(ids()[cache as usize]);
    }
}

#[inline]
pub fn evicted(cache: Cache, count: usize) {
    if caches::on() && count > 0 {
        caches::named_evicted(ids()[cache as usize], count as u64);
    }
}
