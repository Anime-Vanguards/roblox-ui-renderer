//! Port of runtime/text/opentype.luau: a minimal OpenType reader (head/hhea/OS2/hmtx metrics, cmap
//! formats 0/4/6/12, GDEF glyph classes) and the GSUB/GPOS lookups Roblox's HarfBuzz shaping uses for
//! simple scripts (single + ligature substitution, pair kerning, extension lookups, legacy `kern`).
//!
//! Every read is bounds checked like `buffer.readu8`: an out-of-range read fails the parse (the Luau
//! `pcall` in `Fonts.LoadFile` / `LayoutLookups`).

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::{Mutex, OnceLock};

/// Multiplicative hasher for integer keys (glyph ids, code points).
#[derive(Default, Clone, Copy)]
pub struct IntHasher(u64);

impl Hasher for IntHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.0 = (self.0.rotate_left(5) ^ byte as u64).wrapping_mul(0x51_7c_c1_b7_27_22_0a_95);
        }
    }
    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.0 = (value as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15);
    }
    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.0 = value.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (value >> 29);
    }
}

pub type IntMap<K, V> = HashMap<K, V, BuildHasherDefault<IntHasher>>;

type Read<T> = Result<T, ()>;

#[inline]
fn u8_at(data: &[u8], offset: usize) -> Read<u32> {
    data.get(offset).map(|&byte| byte as u32).ok_or(())
}

#[inline]
fn u16_at(data: &[u8], offset: usize) -> Read<u32> {
    Ok(u8_at(data, offset)? * 256 + u8_at(data, offset + 1)?)
}

#[inline]
fn i16_at(data: &[u8], offset: usize) -> Read<i32> {
    let value = u16_at(data, offset)? as i32;
    Ok(if value >= 32768 { value - 65536 } else { value })
}

#[inline]
fn u32_at(data: &[u8], offset: usize) -> Read<u32> {
    Ok(u16_at(data, offset)? * 65536 + u16_at(data, offset + 2)?)
}

#[inline]
fn tag_at(data: &[u8], offset: usize) -> Read<[u8; 4]> {
    let bytes = data.get(offset..offset + 4).ok_or(())?;
    Ok([bytes[0], bytes[1], bytes[2], bytes[3]])
}

/// A signed offset sum as the Luau code computes it (offsets never go negative in practice).
#[inline]
fn at(base: usize, delta: u32) -> usize {
    base + delta as usize
}

// Coverage / ClassDef

fn read_coverage(data: &[u8], offset: usize) -> Read<IntMap<u32, u32>> {
    let mut result = IntMap::default();
    let format = u16_at(data, offset)?;
    if format == 1 {
        let count = u16_at(data, offset + 2)? as usize;
        for index in 0..count {
            result.insert(u16_at(data, offset + 4 + index * 2)?, index as u32);
        }
    } else if format == 2 {
        let count = u16_at(data, offset + 2)? as usize;
        for index in 0..count {
            let record = offset + 4 + index * 6;
            let start = u16_at(data, record)?;
            let finish = u16_at(data, record + 2)?;
            let start_coverage = u16_at(data, record + 4)?;
            for glyph in start..=finish {
                result.insert(glyph, start_coverage + glyph - start);
            }
        }
    }
    Ok(result)
}

fn read_class_def(data: &[u8], offset: usize) -> Read<IntMap<u32, u32>> {
    let mut result = IntMap::default();
    let format = u16_at(data, offset)?;
    if format == 1 {
        let start_glyph = u16_at(data, offset + 2)?;
        let count = u16_at(data, offset + 4)? as usize;
        for index in 0..count {
            let class = u16_at(data, offset + 6 + index * 2)?;
            if class != 0 {
                result.insert(start_glyph + index as u32, class);
            }
        }
    } else if format == 2 {
        let count = u16_at(data, offset + 2)? as usize;
        for index in 0..count {
            let record = offset + 4 + index * 6;
            let start = u16_at(data, record)?;
            let finish = u16_at(data, record + 2)?;
            let class = u16_at(data, record + 4)?;
            if class != 0 {
                for glyph in start..=finish {
                    result.insert(glyph, class);
                }
            }
        }
    }
    Ok(result)
}

// ValueRecord (GPOS)

fn value_record_size(format: u32) -> usize {
    (0..8).filter(|bit| format & (1 << bit) != 0).count() * 2
}

/// XPlacement and XAdvance of a value record.
fn read_value_record(data: &[u8], offset: usize, format: u32) -> Read<(i32, i32)> {
    let mut placement = 0;
    let mut advance = 0;
    let mut cursor = offset;
    if format & 0x1 != 0 {
        placement = i16_at(data, cursor)?;
        cursor += 2;
    }
    if format & 0x2 != 0 {
        cursor += 2;
    }
    if format & 0x4 != 0 {
        advance = i16_at(data, cursor)?;
    }
    Ok((placement, advance))
}

// Layout tables

struct Subtable {
    kind: u32,
    offset: usize,
}

struct Lookup {
    flag: u32,
    subtables: Vec<Subtable>,
    parsed: OnceLock<Vec<Parsed>>,
}

enum Parsed {
    None,
    Single(SingleSubst),
    Ligature(LigatureSubst),
    Pair(PairPos),
}

struct SingleSubst {
    coverage: IntMap<u32, u32>,
    delta: Option<i32>,
    substitutes: Vec<u32>,
}

struct Ligature {
    glyph: u32,
    components: Vec<u32>,
}

struct LigatureSubst {
    coverage: IntMap<u32, u32>,
    sets: Vec<Vec<Ligature>>,
}

struct PairPos {
    coverage: IntMap<u32, u32>,
    format: u32,
    /// Format 1: per coverage index, second glyph -> (Advance1, Placement1, Advance2, Placement2).
    sets: Vec<IntMap<u32, [i32; 4]>>,
    class_def1: IntMap<u32, u32>,
    class_def2: IntMap<u32, u32>,
    class1_count: u32,
    class2_count: u32,
    records: usize,
    format1: u32,
    format2: u32,
    size1: usize,
    size2: usize,
}

fn read_lookups(data: &[u8], table_offset: usize, is_gpos: bool) -> Read<Vec<Lookup>> {
    let list = at(table_offset, u16_at(data, table_offset + 8)?);
    let count = u16_at(data, list)? as usize;
    let mut lookups = Vec::with_capacity(count);
    for index in 0..count {
        let lookup_offset = at(list, u16_at(data, list + 2 + index * 2)?);
        let lookup_type = u16_at(data, lookup_offset)?;
        let flag = u16_at(data, lookup_offset + 2)?;
        let sub_count = u16_at(data, lookup_offset + 4)? as usize;
        let mut subtables = Vec::with_capacity(sub_count);
        for sub in 0..sub_count {
            let mut sub_offset = at(lookup_offset, u16_at(data, lookup_offset + 6 + sub * 2)?);
            let mut kind = lookup_type;
            let extension = if is_gpos { 9 } else { 7 };
            if kind == extension {
                kind = u16_at(data, sub_offset + 2)?;
                sub_offset = at(sub_offset, u32_at(data, sub_offset + 4)?);
            }
            subtables.push(Subtable { kind, offset: sub_offset });
        }
        if flag & 0x10 != 0 {
            u16_at(data, lookup_offset + 6 + sub_count * 2)?;
        }
        lookups.push(Lookup { flag, subtables, parsed: OnceLock::new() });
    }
    Ok(lookups)
}

/// Sorted lookup indices enabled by `features` for the script (DFLT when the script is absent).
fn read_feature_lookups(data: &[u8], table_offset: usize, features: &[&[u8; 4]], script_tag: &[u8; 4]) -> Read<Vec<u32>> {
    let script_list = at(table_offset, u16_at(data, table_offset + 4)?);
    let feature_list = at(table_offset, u16_at(data, table_offset + 6)?);

    let script_count = u16_at(data, script_list)? as usize;
    let mut chosen = None;
    let mut fallback = None;
    for index in 0..script_count {
        let record = script_list + 2 + index * 6;
        let tag = tag_at(data, record)?;
        let offset = at(script_list, u16_at(data, record + 4)?);
        if &tag == script_tag {
            chosen = Some(offset);
        } else if &tag == b"DFLT" {
            fallback = Some(offset);
        } else if &tag == b"latn" && fallback.is_none() {
            fallback = Some(offset);
        }
    }
    let Some(script) = chosen.or(fallback) else {
        return Ok(Vec::new());
    };

    let default_lang_sys = u16_at(data, script)?;
    if default_lang_sys == 0 {
        return Ok(Vec::new());
    }
    let lang_sys = at(script, default_lang_sys);

    let feature_count = u16_at(data, feature_list)?;
    let mut enabled: Vec<u32> = Vec::new();
    let add_feature = |feature_index: u32, enabled: &mut Vec<u32>| -> Read<()> {
        if feature_index >= feature_count {
            return Ok(());
        }
        let record = feature_list + 2 + feature_index as usize * 6;
        let tag = tag_at(data, record)?;
        if !features.iter().any(|feature| **feature == tag) {
            return Ok(());
        }
        let feature_offset = at(feature_list, u16_at(data, record + 4)?);
        let lookup_count = u16_at(data, feature_offset + 2)? as usize;
        for lookup in 0..lookup_count {
            enabled.push(u16_at(data, feature_offset + 4 + lookup * 2)?);
        }
        Ok(())
    };

    let required = u16_at(data, lang_sys + 2)?;
    if required != 0xFFFF {
        add_feature(required, &mut enabled)?;
    }
    let count = u16_at(data, lang_sys + 4)? as usize;
    for index in 0..count {
        let feature = u16_at(data, lang_sys + 6 + index * 2)?;
        add_feature(feature, &mut enabled)?;
    }
    enabled.sort_unstable();
    enabled.dedup();
    Ok(enabled)
}

// Font loading

fn parse_cmap(data: &[u8], offset: usize) -> Read<IntMap<u32, u32>> {
    let count = u16_at(data, offset + 2)? as usize;
    let mut best = None;
    let mut best_score = -1;
    for index in 0..count {
        let record = offset + 4 + index * 8;
        let platform = u16_at(data, record)?;
        let encoding = u16_at(data, record + 2)?;
        let sub_offset = at(offset, u32_at(data, record + 4)?);
        let format = u16_at(data, sub_offset)?;
        let score = if platform == 3 && encoding == 10 && format == 12 {
            5
        } else if platform == 0 && (encoding == 4 || encoding == 6) && format == 12 {
            4
        } else if platform == 3 && encoding == 1 && format == 4 {
            3
        } else if platform == 0 && format == 4 {
            2
        } else if platform == 3 && encoding == 0 {
            1
        } else if format == 0 || format == 6 {
            0
        } else {
            -1
        };
        if score > best_score {
            best = Some(sub_offset);
            best_score = score;
        }
    }

    let mut map = IntMap::default();
    let Some(best) = best else {
        return Ok(map);
    };

    let format = u16_at(data, best)?;
    if format == 4 {
        let seg_count = (u16_at(data, best + 6)? / 2) as usize;
        let end_codes = best + 14;
        let start_codes = end_codes + seg_count * 2 + 2;
        let id_deltas = start_codes + seg_count * 2;
        let id_range_offsets = id_deltas + seg_count * 2;
        for segment in 0..seg_count {
            let end_code = u16_at(data, end_codes + segment * 2)?;
            let start_code = u16_at(data, start_codes + segment * 2)?;
            let delta = i16_at(data, id_deltas + segment * 2)?;
            let range_offset_position = id_range_offsets + segment * 2;
            let range_offset = u16_at(data, range_offset_position)?;
            if start_code == 0xFFFF {
                continue;
            }
            for code in start_code..=end_code {
                let glyph = if range_offset == 0 {
                    (code as i32 + delta).rem_euclid(65536) as u32
                } else {
                    let glyph_offset = range_offset_position + range_offset as usize + (code - start_code) as usize * 2;
                    let glyph = u16_at(data, glyph_offset)?;
                    if glyph != 0 { (glyph as i32 + delta).rem_euclid(65536) as u32 } else { 0 }
                };
                if glyph != 0 {
                    map.insert(code, glyph);
                }
            }
        }
    } else if format == 12 {
        let groups = u32_at(data, best + 12)? as usize;
        for group in 0..groups {
            let record = best + 16 + group * 12;
            let start = u32_at(data, record)? as u64;
            let mut finish = u32_at(data, record + 4)? as u64;
            let start_glyph = u32_at(data, record + 8)? as u64;
            if finish > start && finish - start > 200000 {
                finish = start + 200000;
            }
            let mut code = start;
            while code <= finish {
                map.insert(code as u32, (start_glyph + code - start) as u32);
                code += 1;
            }
        }
    } else if format == 6 {
        let first = u16_at(data, best + 6)?;
        let count = u16_at(data, best + 8)? as usize;
        for index in 0..count {
            let glyph = u16_at(data, best + 10 + index * 2)?;
            if glyph != 0 {
                map.insert(first + index as u32, glyph);
            }
        }
    } else if format == 0 {
        for code in 0..=255u32 {
            let glyph = u8_at(data, best + 6 + code as usize)?;
            if glyph != 0 {
                map.insert(code, glyph);
            }
        }
    }
    Ok(map)
}

struct LayoutTable {
    enabled: Vec<u32>,
    lookups: Vec<Lookup>,
}

pub struct Font {
    pub data: Vec<u8>,
    tables: HashMap<[u8; 4], (usize, usize)>,
    pub units_per_em: f64,
    pub ascender: f64,
    pub descender: f64,
    pub line_gap: f64,
    pub typo_ascender: f64,
    pub typo_descender: f64,
    pub win_ascent: f64,
    pub win_descent: f64,
    pub num_glyphs: u32,
    pub advances: Vec<i32>,
    pub left_bearings: Vec<i32>,
    pub cmap: IntMap<u32, u32>,
    glyph_classes: Option<IntMap<u32, u32>>,
    pub is_cff: bool,
    pub name: String,
    gsub: OnceLock<LayoutTable>,
    gpos: OnceLock<LayoutTable>,
    gsub_extra: Mutex<HashMap<String, std::sync::Arc<Vec<u32>>>>,
    legacy_kern: OnceLock<Vec<IntMap<u64, i32>>>,
}

impl std::fmt::Debug for Font {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_struct("Font").field("name", &self.name).finish()
    }
}

/// One shaped glyph: advance and x offset in font units.
#[derive(Clone, Copy, Debug)]
pub struct GlyphInfo {
    pub glyph: u32,
    pub cluster: u32,
    pub advance: i64,
    pub x_offset: i64,
    /// A `kern` table pair's value in font units, on the pair's first glyph and on its second: the pair is
    /// scaled whole and then halved (layout.rs `shape_paragraph`), so it is not in `advance` or `x_offset`.
    pub kern: [i64; 2],
}

const DEFAULT_GSUB: [&[u8; 4]; 8] = [b"ccmp", b"locl", b"rlig", b"liga", b"clig", b"calt", b"rclt", b"rvrn"];
const DEFAULT_GPOS: [&[u8; 4]; 4] = [b"kern", b"mark", b"mkmk", b"dist"];

impl Font {
    /// `OpenType.Parse`; Err when a read falls outside the file or a required table is missing.
    pub fn parse(data: Vec<u8>, name: &str) -> Result<Font, String> {
        Self::parse_inner(data, name).map_err(|message| format!("failed to parse font {name}: {message}"))
    }

    fn parse_inner(data: Vec<u8>, name: &str) -> Result<Font, String> {
        let oob = |_| "buffer access out of bounds".to_string();
        let sfnt = tag_at(&data, 0).map_err(oob)?;
        let table_count = u16_at(&data, 4).map_err(oob)? as usize;
        let mut tables = HashMap::new();
        for index in 0..table_count {
            let record = 12 + index * 16;
            let tag = tag_at(&data, record).map_err(oob)?;
            let offset = u32_at(&data, record + 8).map_err(oob)? as usize;
            let length = u32_at(&data, record + 12).map_err(oob)? as usize;
            tables.insert(tag, (offset, length));
        }
        let head = tables.get(b"head").ok_or("font has no head table")?.0;
        let hhea = tables.get(b"hhea").ok_or("font has no hhea table")?.0;
        let maxp = tables.get(b"maxp").ok_or("font has no maxp table")?.0;
        let hmtx = tables.get(b"hmtx").ok_or("font has no hmtx table")?.0;

        let read = || -> Read<Font> {
            let mut font = Font {
                units_per_em: u16_at(&data, head + 18)? as f64,
                ascender: i16_at(&data, hhea + 4)? as f64,
                descender: i16_at(&data, hhea + 6)? as f64,
                line_gap: i16_at(&data, hhea + 8)? as f64,
                typo_ascender: 0.0,
                typo_descender: 0.0,
                win_ascent: 0.0,
                win_descent: 0.0,
                num_glyphs: u16_at(&data, maxp + 4)?,
                advances: Vec::new(),
                left_bearings: Vec::new(),
                cmap: IntMap::default(),
                glyph_classes: None,
                is_cff: &sfnt == b"OTTO",
                name: name.to_string(),
                tables: HashMap::new(),
                data: Vec::new(),
                gsub: OnceLock::new(),
                gpos: OnceLock::new(),
                gsub_extra: Mutex::new(HashMap::new()),
                legacy_kern: OnceLock::new(),
            };
            if let Some(&(os2, _)) = tables.get(b"OS/2") {
                font.typo_ascender = i16_at(&data, os2 + 68)? as f64;
                font.typo_descender = i16_at(&data, os2 + 70)? as f64;
                font.win_ascent = u16_at(&data, os2 + 74)? as f64;
                font.win_descent = u16_at(&data, os2 + 76)? as f64;
            }
            let number_of_h_metrics = u16_at(&data, hhea + 34)? as usize;
            let glyphs = font.num_glyphs as usize;
            font.advances.reserve(glyphs);
            font.left_bearings.reserve(glyphs);
            let mut last_advance = 0;
            for glyph in 0..glyphs {
                if glyph < number_of_h_metrics {
                    last_advance = u16_at(&data, hmtx + glyph * 4)? as i32;
                    font.advances.push(last_advance);
                    font.left_bearings.push(i16_at(&data, hmtx + glyph * 4 + 2)?);
                } else {
                    font.advances.push(last_advance);
                    font.left_bearings.push(i16_at(&data, hmtx + number_of_h_metrics * 4 + (glyph - number_of_h_metrics) * 2)?);
                }
            }
            if let Some(&(cmap, _)) = tables.get(b"cmap") {
                font.cmap = parse_cmap(&data, cmap)?;
            }
            if let Some(&(gdef, _)) = tables.get(b"GDEF") {
                let class_offset = u16_at(&data, gdef + 4)?;
                if class_offset != 0 {
                    font.glyph_classes = Some(read_class_def(&data, at(gdef, class_offset))?);
                }
                let mark_offset = u16_at(&data, gdef + 10)?;
                if mark_offset != 0 {
                    read_class_def(&data, at(gdef, mark_offset))?;
                }
            }
            Ok(font)
        };
        let mut font = read().map_err(oob)?;
        font.tables = tables;
        font.data = data;
        Ok(font)
    }

    #[inline]
    pub fn has_glyph(&self, code: u32) -> bool {
        self.cmap.contains_key(&code)
    }

    /// `Font.Advances[glyph] or 0`.
    #[inline]
    pub fn advance(&self, glyph: u32) -> i64 {
        self.advances.get(glyph as usize).copied().unwrap_or(0) as i64
    }

    fn table(&self, tag: &[u8; 4]) -> Option<usize> {
        self.tables.get(tag).map(|entry| entry.0)
    }

    #[inline]
    fn skipped(&self, glyph: u32, flag: u32) -> bool {
        let Some(classes) = &self.glyph_classes else {
            return false;
        };
        match classes.get(&glyph) {
            Some(1) => flag & 0x2 != 0,
            Some(2) => flag & 0x4 != 0,
            Some(3) => flag & 0x8 != 0,
            _ => false,
        }
    }

    fn layout_table(&self, gpos: bool) -> &LayoutTable {
        let cell = if gpos { &self.gpos } else { &self.gsub };
        cell.get_or_init(|| {
            let tag = if gpos { b"GPOS" } else { b"GSUB" };
            let Some(offset) = self.table(tag) else {
                return LayoutTable { enabled: Vec::new(), lookups: Vec::new() };
            };
            let features: &[&[u8; 4]] = if gpos { &DEFAULT_GPOS } else { &DEFAULT_GSUB };
            let result = read_lookups(&self.data, offset, gpos)
                .and_then(|lookups| Ok((lookups, read_feature_lookups(&self.data, offset, features, b"latn")?)));
            match result {
                Ok((lookups, enabled)) => LayoutTable { enabled, lookups },
                Err(()) => LayoutTable { enabled: Vec::new(), lookups: Vec::new() },
            }
        })
    }

    fn extra_gsub_lookups(&self, extra: &[&str]) -> std::sync::Arc<Vec<u32>> {
        let mut key: Vec<&str> = extra.to_vec();
        key.sort_unstable();
        key.dedup();
        let cache_key = key.join(",");
        let mut cache = self.gsub_extra.lock().unwrap();
        if let Some(found) = cache.get(&cache_key) {
            return found.clone();
        }
        let mut features: Vec<[u8; 4]> = DEFAULT_GSUB.iter().map(|tag| **tag).collect();
        for tag in &key {
            let bytes = tag.as_bytes();
            if bytes.len() == 4 {
                features.push([bytes[0], bytes[1], bytes[2], bytes[3]]);
            }
        }
        let refs: Vec<&[u8; 4]> = features.iter().collect();
        let enabled = match self.table(b"GSUB") {
            Some(offset) => read_feature_lookups(&self.data, offset, &refs, b"latn").unwrap_or_default(),
            None => Vec::new(),
        };
        let enabled = std::sync::Arc::new(enabled);
        cache.insert(cache_key, enabled.clone());
        enabled
    }

    fn parsed<'a>(&self, lookup: &'a Lookup, gpos: bool) -> &'a [Parsed] {
        lookup.parsed.get_or_init(|| {
            lookup
                .subtables
                .iter()
                .map(|sub| {
                    let parsed = match (gpos, sub.kind) {
                        (false, 1) => parse_single_subst(&self.data, sub.offset).map(Parsed::Single),
                        (false, 4) => parse_ligature_subst(&self.data, sub.offset).map(Parsed::Ligature),
                        (true, 2) => parse_pair_pos(&self.data, sub.offset).map(Parsed::Pair),
                        _ => Ok(Parsed::None),
                    };
                    parsed.unwrap_or(Parsed::None)
                })
                .collect()
        })
    }

    fn apply_gsub(&self, glyphs: &mut Vec<GlyphInfo>, extra: Option<&[&str]>) {
        let table = self.layout_table(false);
        let extra_enabled;
        let enabled: &[u32] = match extra {
            Some(extra) => {
                extra_enabled = self.extra_gsub_lookups(extra);
                &extra_enabled
            }
            None => &table.enabled,
        };
        for &lookup_index in enabled {
            let Some(lookup) = table.lookups.get(lookup_index as usize) else {
                continue;
            };
            let parsed = self.parsed(lookup, false);
            for sub in parsed {
                match sub {
                    Parsed::Single(single) => {
                        for info in glyphs.iter_mut() {
                            if let Some(&coverage_index) = single.coverage.get(&info.glyph) {
                                if !self.skipped(info.glyph, lookup.flag) {
                                    info.glyph = match single.delta {
                                        Some(delta) => (info.glyph as i64 + delta as i64).rem_euclid(65536) as u32,
                                        None => single.substitutes.get(coverage_index as usize).copied().unwrap_or(info.glyph),
                                    };
                                }
                            }
                        }
                    }
                    Parsed::Ligature(ligatures) => {
                        let mut index = 0;
                        let mut positions: Vec<usize> = Vec::new();
                        while index < glyphs.len() {
                            let first = glyphs[index].glyph;
                            if let Some(&coverage_index) = ligatures.coverage.get(&first) {
                                if !self.skipped(first, lookup.flag) {
                                    if let Some(set) = ligatures.sets.get(coverage_index as usize) {
                                        for ligature in set {
                                            let mut matches = true;
                                            positions.clear();
                                            positions.push(index);
                                            let mut cursor = index;
                                            for &component in &ligature.components {
                                                cursor += 1;
                                                while cursor < glyphs.len() && self.skipped(glyphs[cursor].glyph, lookup.flag) {
                                                    cursor += 1;
                                                }
                                                if cursor >= glyphs.len() || glyphs[cursor].glyph != component {
                                                    matches = false;
                                                    break;
                                                }
                                                positions.push(cursor);
                                            }
                                            if matches {
                                                glyphs[index].glyph = ligature.glyph;
                                                for &remove in positions[1..].iter().rev() {
                                                    glyphs.remove(remove);
                                                }
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            index += 1;
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    fn apply_gpos(&self, glyphs: &mut [GlyphInfo]) {
        let table = self.layout_table(true);
        for &lookup_index in &table.enabled {
            let Some(lookup) = table.lookups.get(lookup_index as usize) else {
                continue;
            };
            let parsed = self.parsed(lookup, true);
            let mut index = 0;
            while index + 1 < glyphs.len() {
                if self.skipped(glyphs[index].glyph, lookup.flag) {
                    index += 1;
                    continue;
                }
                let mut next = index + 1;
                while next < glyphs.len() && self.skipped(glyphs[next].glyph, lookup.flag) {
                    next += 1;
                }
                if next >= glyphs.len() {
                    break;
                }
                let mut advanced = false;
                for (sub_index, sub) in lookup.subtables.iter().enumerate() {
                    if sub.kind != 2 {
                        continue;
                    }
                    let Parsed::Pair(pair) = &parsed[sub_index] else {
                        continue;
                    };
                    if let Some([advance1, placement1, advance2, placement2]) = self.pair_value(pair, glyphs[index].glyph, glyphs[next].glyph) {
                        glyphs[index].advance += advance1 as i64;
                        glyphs[index].x_offset += placement1 as i64;
                        glyphs[next].advance += advance2 as i64;
                        glyphs[next].x_offset += placement2 as i64;
                        index = if pair.size2 > 0 { next + 1 } else { next };
                        advanced = true;
                        break;
                    }
                }
                if !advanced {
                    index += 1;
                }
            }
        }
    }

    fn pair_value(&self, sub: &PairPos, first: u32, second: u32) -> Option<[i32; 4]> {
        let coverage = *sub.coverage.get(&first)?;
        if sub.format == 1 {
            return sub.sets.get(coverage as usize)?.get(&second).copied();
        }
        let class1 = sub.class_def1.get(&first).copied().unwrap_or(0);
        let class2 = sub.class_def2.get(&second).copied().unwrap_or(0);
        if class1 >= sub.class1_count || class2 >= sub.class2_count {
            return Some([0, 0, 0, 0]);
        }
        let record_size = sub.size1 + sub.size2;
        let record = sub.records + (class1 as usize * sub.class2_count as usize + class2 as usize) * record_size;
        let (placement1, advance1) = read_value_record(&self.data, record, sub.format1).unwrap_or((0, 0));
        let (placement2, advance2) = read_value_record(&self.data, record + sub.size1, sub.format2).unwrap_or((0, 0));
        Some([advance1, placement1, advance2, placement2])
    }

    fn has_gpos_kern(&self) -> Read<bool> {
        let Some(offset) = self.table(b"GPOS") else {
            return Ok(false);
        };
        let feature_list = at(offset, u16_at(&self.data, offset + 6)?);
        for index in 0..u16_at(&self.data, feature_list)? as usize {
            if &tag_at(&self.data, feature_list + 2 + index * 6)? == b"kern" {
                return Ok(true);
            }
        }
        Ok(false)
    }

    fn read_legacy_kern(&self) -> Read<Vec<IntMap<u64, i32>>> {
        let mut subtables = Vec::new();
        let Some(offset) = self.table(b"kern") else {
            return Ok(subtables);
        };
        if self.has_gpos_kern()? {
            return Ok(subtables);
        }
        let data = &self.data;
        if u16_at(data, offset)? != 0 {
            return Ok(subtables);
        }
        let mut cursor = offset + 4;
        for _ in 0..u16_at(data, offset + 2)? {
            let length = u16_at(data, cursor + 2)? as usize;
            let coverage = u16_at(data, cursor + 4)?;
            let horizontal = coverage & 0x1 != 0;
            let minimum = coverage & 0x2 != 0;
            let cross_stream = coverage & 0x4 != 0;
            if coverage >> 8 == 0 && horizontal && !minimum && !cross_stream {
                let mut pairs = IntMap::default();
                for index in 0..u16_at(data, cursor + 6)? as usize {
                    let record = cursor + 14 + index * 6;
                    let key = u16_at(data, record)? as u64 * 65536 + u16_at(data, record + 2)? as u64;
                    pairs.insert(key, i16_at(data, record + 4)?);
                }
                subtables.push(pairs);
            }
            cursor += length;
        }
        Ok(subtables)
    }

    fn apply_legacy_kern(&self, glyphs: &mut [GlyphInfo]) {
        // A malformed kern table raises in the Luau reader; nothing is kerned here then.
        let subtables = self.legacy_kern.get_or_init(|| self.read_legacy_kern().unwrap_or_default());
        let classes = self.glyph_classes.as_ref();
        for pairs in subtables {
            let mut index = 0;
            while index + 1 < glyphs.len() {
                let mut next = index + 1;
                while next < glyphs.len() && classes.is_some_and(|classes| classes.get(&glyphs[next].glyph) == Some(&3)) {
                    next += 1;
                }
                if next >= glyphs.len() {
                    break;
                }
                let key = glyphs[index].glyph as u64 * 65536 + glyphs[next].glyph as u64;
                if let Some(&value) = pairs.get(&key) {
                    if value != 0 {
                        glyphs[index].kern[0] += value as i64;
                        glyphs[next].kern[1] += value as i64;
                    }
                }
                index = next;
            }
        }
    }

    /// `OpenType.Shape`: shapes (code, cluster) pairs with the default features (plus `extra` GSUB
    /// features); advances and offsets in font units.
    pub fn shape(&self, codepoints: &[(u32, u32)], extra: Option<&[&str]>) -> Vec<GlyphInfo> {
        let mut glyphs: Vec<GlyphInfo> = codepoints
            .iter()
            .map(|&(code, cluster)| GlyphInfo { glyph: self.cmap.get(&code).copied().unwrap_or(0), cluster, advance: 0, x_offset: 0, kern: [0; 2] })
            .collect();
        self.apply_gsub(&mut glyphs, extra);
        for info in glyphs.iter_mut() {
            info.advance = self.advance(info.glyph);
        }
        self.apply_gpos(&mut glyphs);
        self.apply_legacy_kern(&mut glyphs);
        glyphs
    }
}

fn parse_single_subst(data: &[u8], offset: usize) -> Read<SingleSubst> {
    let format = u16_at(data, offset)?;
    let coverage = read_coverage(data, at(offset, u16_at(data, offset + 2)?))?;
    if format == 1 {
        return Ok(SingleSubst { coverage, delta: Some(i16_at(data, offset + 4)?), substitutes: Vec::new() });
    }
    let count = u16_at(data, offset + 4)? as usize;
    let mut substitutes = Vec::with_capacity(count);
    for index in 0..count {
        substitutes.push(u16_at(data, offset + 6 + index * 2)?);
    }
    Ok(SingleSubst { coverage, delta: None, substitutes })
}

fn parse_ligature_subst(data: &[u8], offset: usize) -> Read<LigatureSubst> {
    let coverage = read_coverage(data, at(offset, u16_at(data, offset + 2)?))?;
    let set_count = u16_at(data, offset + 4)? as usize;
    let mut sets = Vec::with_capacity(set_count);
    for set_index in 0..set_count {
        let set_offset = at(offset, u16_at(data, offset + 6 + set_index * 2)?);
        let ligature_count = u16_at(data, set_offset)? as usize;
        let mut ligatures = Vec::with_capacity(ligature_count);
        for ligature_index in 0..ligature_count {
            let ligature_offset = at(set_offset, u16_at(data, set_offset + 2 + ligature_index * 2)?);
            let glyph = u16_at(data, ligature_offset)?;
            let component_count = u16_at(data, ligature_offset + 2)? as usize;
            let mut components = Vec::with_capacity(component_count.saturating_sub(1));
            for component in 1..component_count {
                components.push(u16_at(data, ligature_offset + 2 + component * 2)?);
            }
            ligatures.push(Ligature { glyph, components });
        }
        sets.push(ligatures);
    }
    Ok(LigatureSubst { coverage, sets })
}

fn parse_pair_pos(data: &[u8], offset: usize) -> Read<PairPos> {
    let format = u16_at(data, offset)?;
    let coverage = read_coverage(data, at(offset, u16_at(data, offset + 2)?))?;
    let format1 = u16_at(data, offset + 4)?;
    let format2 = u16_at(data, offset + 6)?;
    let size1 = value_record_size(format1);
    let size2 = value_record_size(format2);
    let mut result = PairPos {
        coverage,
        format,
        sets: Vec::new(),
        class_def1: IntMap::default(),
        class_def2: IntMap::default(),
        class1_count: 0,
        class2_count: 0,
        records: 0,
        format1,
        format2,
        size1,
        size2,
    };
    if format == 1 {
        let set_count = u16_at(data, offset + 8)? as usize;
        for set_index in 0..set_count {
            let set_offset = at(offset, u16_at(data, offset + 10 + set_index * 2)?);
            let pair_count = u16_at(data, set_offset)? as usize;
            let mut pairs = IntMap::default();
            pairs.reserve(pair_count);
            for pair in 0..pair_count {
                let record = set_offset + 2 + pair * (2 + size1 + size2);
                let second = u16_at(data, record)?;
                let (placement1, advance1) = read_value_record(data, record + 2, format1)?;
                let (placement2, advance2) = read_value_record(data, record + 2 + size1, format2)?;
                pairs.insert(second, [advance1, placement1, advance2, placement2]);
            }
            result.sets.push(pairs);
        }
        return Ok(result);
    }
    result.class_def1 = read_class_def(data, at(offset, u16_at(data, offset + 8)?))?;
    result.class_def2 = read_class_def(data, at(offset, u16_at(data, offset + 10)?))?;
    result.class1_count = u16_at(data, offset + 12)?;
    result.class2_count = u16_at(data, offset + 14)?;
    result.records = offset + 16;
    Ok(result)
}
