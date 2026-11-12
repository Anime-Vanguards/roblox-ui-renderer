//! Port of runtime/text/richtext.luau: Roblox RichText tag parser producing styled runs (<b>, <i>,
//! <u>, <s>, <font ...>, <stroke ...>, <mark ...>, <uc>/<uppercase>, <sc>/<smallcaps>, <br/>,
//! <!-- -->, entity escapes) plus rich layout. Works on bytes like the Luau string functions (the
//! Lua patterns of the reference are matched by hand with the same semantics).

use std::sync::Arc;

use super::fonts::FontRegistry;
use super::layout::{Glyph, LayoutParams, Prepared, TextLayout, rich_size_for, round_pen, shape_paragraph};
use super::num::{f32r, lmax, tonumber};
use super::utf8::Codepoint;
use super::{KeyHasher, TextEngine};
use crate::cache_counts::{self, Cache};

pub type Color = [f64; 3];

#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    /// None: an unparseable `color` attribute (the Luau ParseColor returned nil).
    pub color: Option<Color>,
    pub thickness: f64,
    pub transparency: f64,
    pub joins: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub color: Option<Color>,
    pub transparency: f64,
}

/// `RichText.Style`. Optional fields are nil in the Luau (inherit the label's value).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikethrough: bool,
    pub uppercase: bool,
    pub small_caps: bool,
    pub color: Option<Color>,
    pub size: Option<f64>,
    pub family: Option<String>,
    pub weight: Option<f64>,
    pub face_weight: Option<f64>,
    pub font_style: Option<String>,
    pub transparency: Option<f64>,
    pub stroke: Option<Stroke>,
    pub mark: Option<Mark>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    /// The run's text after entity unescaping (bytes, like the Luau string).
    pub text: Vec<u8>,
    pub style: Style,
}

impl Style {
    /// The requested weight of a run (`<font weight>`, `<b>` = 700, a `<font face>` weight, else the
    /// label's), as RichText.Layout and the export compute it.
    pub fn run_weight(&self, label_weight: f64) -> f64 {
        self.weight.or(if self.bold { Some(700.0) } else { None }).or(self.face_weight).unwrap_or(label_weight)
    }

    /// The requested style of a run ("italic" for `<i>`, a `<font face>` style, else the label's).
    pub fn run_style<'a>(&'a self, label_style: &'a str) -> &'a str {
        if self.italic { "italic" } else { self.font_style.as_deref().unwrap_or(label_style) }
    }
}

fn entity(name: &[u8]) -> Option<u8> {
    Some(match name {
        b"lt" => b'<',
        b"gt" => b'>',
        b"amp" => b'&',
        b"quot" => b'"',
        b"apos" => b'\'',
        _ => return None,
    })
}

/// `string.gsub(Text, "&(%w+);", Entities)`: known entities replaced, others kept.
fn unescape(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if text[index] == b'&' {
            let start = index + 1;
            let mut end = start;
            while end < text.len() && text[end].is_ascii_alphanumeric() {
                end += 1;
            }
            if end > start && end < text.len() && text[end] == b';' {
                match entity(&text[start..end]) {
                    Some(replacement) => out.push(replacement),
                    None => out.extend_from_slice(&text[index..=end]),
                }
                index = end + 1;
                continue;
            }
        }
        out.push(text[index]);
        index += 1;
    }
    out
}

#[inline]
fn is_space(byte: u8) -> bool {
    super::num::c_isspace(byte)
}

/// `string.gmatch(Source, "([%w_]+)%s*=%s*([\"'])(.-)%2")`, keys lowercased, later keys win.
fn parse_attributes(source: &[u8]) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut attributes: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut position = 0;
    while position < source.len() {
        let matched = (|| {
            let mut key_end = position;
            while key_end < source.len() && (source[key_end].is_ascii_alphanumeric() || source[key_end] == b'_') {
                key_end += 1;
            }
            if key_end == position {
                return None;
            }
            let mut cursor = key_end;
            while cursor < source.len() && is_space(source[cursor]) {
                cursor += 1;
            }
            if cursor >= source.len() || source[cursor] != b'=' {
                return None;
            }
            cursor += 1;
            while cursor < source.len() && is_space(source[cursor]) {
                cursor += 1;
            }
            if cursor >= source.len() || (source[cursor] != b'"' && source[cursor] != b'\'') {
                return None;
            }
            let quote = source[cursor];
            let value_start = cursor + 1;
            let value_end = value_start + source[value_start..].iter().position(|&byte| byte == quote)?;
            Some((key_end, value_start, value_end))
        })();
        match matched {
            Some((key_end, value_start, value_end)) => {
                let key = source[position..key_end].to_ascii_lowercase();
                let value = source[value_start..value_end].to_vec();
                match attributes.iter_mut().find(|(existing, _)| *existing == key) {
                    Some(entry) => entry.1 = value,
                    None => attributes.push((key, value)),
                }
                position = value_end + 1;
            }
            None => position += 1,
        }
    }
    attributes
}

fn attribute<'a>(attributes: &'a [(Vec<u8>, Vec<u8>)], name: &[u8]) -> Option<&'a [u8]> {
    attributes.iter().find(|(key, _)| key == name).map(|(_, value)| value.as_slice())
}

fn hex_digit(byte: u8) -> Option<f64> {
    (byte as char).to_digit(16).map(|digit| digit as f64)
}

/// `RichText.ParseColor`: "#rrggbb" / "rrggbb" or "rgb(r, g, b)" (0-255) as 0..1 channels.
pub fn parse_color(value: &[u8]) -> Option<Color> {
    let hex = value.strip_prefix(b"#").unwrap_or(value);
    if hex.len() == 6 && hex.iter().all(|byte| byte.is_ascii_hexdigit()) {
        let channel = |index: usize| (hex_digit(hex[index]).unwrap() * 16.0 + hex_digit(hex[index + 1]).unwrap()) / 255.0;
        return Some([channel(0), channel(2), channel(4)]);
    }
    // "rgb%(%s*(%d+)%s*,%s*(%d+)%s*,%s*(%d+)%s*%)", unanchored: the leftmost match.
    for start in 0..value.len() {
        if let Some(channels) = match_rgb(&value[start..]) {
            return Some(channels);
        }
    }
    None
}

fn match_rgb(text: &[u8]) -> Option<Color> {
    let rest = text.strip_prefix(b"rgb(")?;
    let mut cursor = 0;
    let mut channels = [0.0; 3];
    for (index, channel) in channels.iter_mut().enumerate() {
        while cursor < rest.len() && is_space(rest[cursor]) {
            cursor += 1;
        }
        let digits_start = cursor;
        while cursor < rest.len() && rest[cursor].is_ascii_digit() {
            cursor += 1;
        }
        if cursor == digits_start {
            return None;
        }
        *channel = tonumber(&rest[digits_start..cursor])? / 255.0;
        while cursor < rest.len() && is_space(rest[cursor]) {
            cursor += 1;
        }
        let terminator = if index < 2 { b',' } else { b')' };
        if cursor >= rest.len() || rest[cursor] != terminator {
            return None;
        }
        cursor += 1;
    }
    Some(channels)
}

fn weight_keyword(name: &[u8]) -> Option<f64> {
    Some(match name.to_ascii_lowercase().as_slice() {
        b"thin" => 100.0,
        b"extralight" => 200.0,
        b"light" => 300.0,
        b"regular" => 400.0,
        b"medium" => 500.0,
        b"semibold" => 600.0,
        b"bold" => 700.0,
        b"extrabold" => 800.0,
        b"heavy" => 900.0,
        _ => return None,
    })
}

const KNOWN_TAGS: [&[u8]; 12] = [b"b", b"i", b"u", b"s", b"font", b"stroke", b"mark", b"uc", b"uppercase", b"sc", b"smallcaps", b"br"];

fn text_of(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// Resolves a `<font face="...">` value. Measured: Enum.Font names ("Gotham", "GothamBold", "Arial")
/// and shipped family names ("Montserrat") are valid; anything else ("GothamSSm") invalidates the
/// whole text, which then shows raw.
fn resolve_face(fonts: &mut FontRegistry, name: &[u8]) -> Option<(String, Option<f64>, Option<String>)> {
    let name = text_of(name);
    if let Some((family, weight, style)) = fonts.enum_face(&name) {
        return Some((family, Some(weight), Some(style)));
    }
    let compact: String = name.chars().filter(|&ch| !(ch.is_ascii() && is_space(ch as u8))).collect();
    let uri = format!("rbxasset://fonts/families/{compact}.json");
    if fonts.has_family(&uri) {
        return Some((uri, None, None));
    }
    None
}

/// `RichText.Parse`. Measured: unknown tags are dropped (their text stays); malformed markup
/// (unclosed / mismatched tags, unknown font faces) makes Roblox show the raw source, which callers
/// handle through the returned valid flag.
pub fn parse(fonts: &mut FontRegistry, source: &[u8]) -> (Vec<Run>, bool) {
    let mut runs: Vec<Run> = Vec::new();
    let mut stack: Vec<(Vec<u8>, Style)> = Vec::new();
    let mut current = Style::default();
    let mut valid = true;
    let mut buffer: Vec<u8> = Vec::new();

    fn flush(runs: &mut Vec<Run>, buffer: &mut Vec<u8>, current: &Style) {
        if buffer.is_empty() {
            return;
        }
        runs.push(Run { text: unescape(buffer), style: current.clone() });
        buffer.clear();
    }
    let find = |needle: &[u8], from: usize| -> Option<usize> {
        if from > source.len() {
            return None;
        }
        source[from..].windows(needle.len()).position(|window| window == needle).map(|found| found + from)
    };

    let length = source.len();
    let mut index = 0;
    while index < length {
        let character = source[index];
        if character == b'<' {
            if source[index..].starts_with(b"<!--") {
                if let Some(close) = find(b"-->", index + 4) {
                    index = close + 3;
                    continue;
                }
            }
            let Some(close) = find(b">", index + 1) else {
                buffer.push(character);
                index += 1;
                continue;
            };

            let inner = &source[index + 1..close];
            let closing = inner.first() == Some(&b'/');
            let self_closing = inner.last() == Some(&b'/');
            let body = if closing { &inner[1..] } else { inner };
            let body = body.strip_suffix(b"/").unwrap_or(body);
            let mut name_start = 0;
            while name_start < body.len() && is_space(body[name_start]) {
                name_start += 1;
            }
            let mut name_end = name_start;
            while name_end < body.len() && body[name_end].is_ascii_alphanumeric() {
                name_end += 1;
            }
            let name = body[name_start..name_end].to_ascii_lowercase();

            if name.is_empty() {
                buffer.push(character);
                index += 1;
                continue;
            }

            flush(&mut runs, &mut buffer, &current);
            if !KNOWN_TAGS.contains(&name.as_slice()) {
                index = close + 1;
                continue;
            }
            let attributes = parse_attributes(body);

            if name == b"br" {
                runs.push(Run { text: b"\n".to_vec(), style: current.clone() });
            } else if closing {
                if stack.last().is_some_and(|top| top.0 == name) {
                    let (_, style) = stack.pop().unwrap();
                    current = style;
                } else {
                    valid = false;
                }
            } else if !self_closing {
                stack.push((name.clone(), current.clone()));
                let mut next = current.clone();
                match name.as_slice() {
                    b"b" => next.bold = true,
                    b"i" => next.italic = true,
                    b"u" => next.underline = true,
                    b"s" => next.strikethrough = true,
                    b"uc" | b"uppercase" => next.uppercase = true,
                    b"sc" | b"smallcaps" => next.small_caps = true,
                    b"font" => {
                        if let Some(color) = attribute(&attributes, b"color") {
                            next.color = parse_color(color);
                        }
                        if let Some(size) = attribute(&attributes, b"size") {
                            next.size = tonumber(size);
                        }
                        if let Some(face) = attribute(&attributes, b"face") {
                            match resolve_face(fonts, face) {
                                Some((family, weight, style)) => {
                                    next.family = Some(family);
                                    next.face_weight = weight;
                                    next.font_style = style;
                                }
                                None => valid = false,
                            }
                        }
                        if let Some(family) = attribute(&attributes, b"family") {
                            next.family = Some(text_of(family));
                        }
                        if let Some(weight) = attribute(&attributes, b"weight") {
                            next.weight = tonumber(weight).or_else(|| weight_keyword(weight));
                        }
                        if let Some(transparency) = attribute(&attributes, b"transparency") {
                            next.transparency = tonumber(transparency);
                        }
                    }
                    b"stroke" => {
                        next.stroke = Some(Stroke {
                            color: match attribute(&attributes, b"color") {
                                Some(color) => parse_color(color),
                                None => Some([0.0, 0.0, 0.0]),
                            },
                            thickness: tonumber(attribute(&attributes, b"thickness").unwrap_or(b"1")).unwrap_or(1.0),
                            transparency: tonumber(attribute(&attributes, b"transparency").unwrap_or(b"0")).unwrap_or(0.0),
                            joins: text_of(attribute(&attributes, b"joins").unwrap_or(b"round")),
                        });
                    }
                    b"mark" => {
                        next.mark = Some(Mark {
                            color: match attribute(&attributes, b"color") {
                                Some(color) => parse_color(color),
                                None => Some([1.0, 1.0, 0.0]),
                            },
                            transparency: tonumber(attribute(&attributes, b"transparency").unwrap_or(b"0")).unwrap_or(0.0),
                        });
                    }
                    _ => {}
                }
                current = next;
            }
            index = close + 1;
            continue;
        }

        buffer.push(character);
        index += 1;
    }
    flush(&mut runs, &mut buffer, &current);

    if !stack.is_empty() {
        valid = false;
    }
    (runs, valid)
}

/// Rich text shaped per (text, base size, font, scale): paragraphs of glyphs carrying their run and
/// size, each paragraph's default size, and the key part for the measure cache.
pub(crate) struct RichShaped {
    paragraphs: Vec<Vec<Glyph>>,
    paragraph_sizes: Vec<f64>,
    cache_key: String,
}

pub(crate) struct ParsedEntry {
    pub runs: Arc<Vec<Run>>,
    pub valid: bool,
}

impl TextEngine {
    fn cached_parse(&mut self, source: &[u8]) -> Arc<ParsedEntry> {
        if let Some(found) = self.rich_parse_cache.get(source) {
            return found.clone();
        }
        let (runs, valid) = parse(&mut self.fonts, source);
        let entry = Arc::new(ParsedEntry { runs: Arc::new(runs), valid });
        self.rich_parse_cache.insert(source.to_vec(), entry.clone());
        entry
    }

    /// `RichText.Parse` (uncached).
    pub fn parse_rich(&mut self, source: &[u8]) -> (Vec<Run>, bool) {
        parse(&mut self.fonts, source)
    }

    /// `RichText.ContentText`: the plain text shown for rich text (TextLabel.ContentText). Measured:
    /// <uc>/<sc> do not change it; invalid markup shows the source.
    pub fn content_text(&mut self, source: &[u8]) -> Vec<u8> {
        let (runs, valid) = parse(&mut self.fonts, source);
        if !valid {
            return source.to_vec();
        }
        let mut out = Vec::with_capacity(source.len());
        for run in runs {
            out.extend_from_slice(&run.text);
        }
        out
    }

    /// `RichText.Layout`: every run is shaped on its own (measured: no kerning across run boundaries)
    /// with its face (<b> = weight 700, <i> = italic, <font face/family/weight>) and size (<font size>
    /// = floor(size x UIScale x DPI)); lines take the height of their largest run. <uc> uppercases,
    /// <sc> applies the font's small-caps feature. Glyphs carry `size` and `run` (1-based index into
    /// the result's `runs`). Invalid markup lays out the raw source as plain text (`rich` false).
    pub fn layout_rich(&mut self, params: &LayoutParams) -> Arc<TextLayout> {
        let parsed = self.cached_parse(params.text);
        if !parsed.valid {
            return self.layout_core(params, None, None);
        }

        let (mut base_size, scaled_size) = rich_size_for(params.text_size, params.pixel_scale, Some(params.family), params.scale, params.effective_size);
        if let Some(pixel_size) = params.pixel_size {
            base_size = lmax(pixel_size.floor(), 1.0);
        }
        let scale = f32r(params.scale.unwrap_or(1.0));

        let mut hasher = KeyHasher::new();
        hasher.u64(base_size.to_bits());
        hasher.u64(scale.to_bits());
        hasher.u64(params.pixel_scale.to_bits());
        hasher.bytes(params.family.as_bytes());
        hasher.u64(params.weight.to_bits());
        hasher.bytes(params.style.as_bytes());
        hasher.bytes(params.text);
        let hash = hasher.finish();
        let key = (base_size.to_bits(), scale.to_bits(), params.pixel_scale.to_bits(), params.weight.to_bits());
        let mut shaped = None;
        if let Some(bucket) = self.rich_shape_cache.get(&hash) {
            for entry in bucket {
                if entry.key == key && &*entry.family == params.family && &*entry.style == params.style && &*entry.text == params.text {
                    shaped = Some(entry.value.clone());
                    break;
                }
            }
        }
        let shaped = match shaped {
            Some(shaped) => {
                cache_counts::hit(Cache::RichText);
                shaped
            }
            None => {
                cache_counts::miss(Cache::RichText);
                let value = Arc::new(self.shape_rich(&parsed.runs, params, base_size, scale));
                self.rich_count += 1;
                if self.rich_count > 5000 {
                    cache_counts::evicted(Cache::RichText, self.rich_shape_cache.values().map(Vec::len).sum());
                    self.rich_shape_cache.clear();
                    self.rich_parse_cache.clear();
                    self.rich_count = 0;
                }
                self.rich_shape_cache.entry(hash).or_default().push(super::RichShapeEntry {
                    key,
                    family: params.family.into(),
                    style: params.style.into(),
                    text: params.text.into(),
                    value: value.clone(),
                });
                value
            }
        };

        let mut copy = *params;
        copy.pixel_size = Some(base_size);
        // Measured (layout-probe-richempty): rich text without any character (Text "" or only empty
        // tags) reports its scaled size E unfloored (TextBounds.Y 13.5 px at E = 9); a <br/> or "\n"
        // line is a line at the base size like any other.
        let empty_height = if parsed.runs.is_empty() && params.pixel_size.is_none() { Some(scaled_size * params.pixel_scale) } else { None };
        let prepared = Prepared {
            paragraphs: &shaped.paragraphs,
            paragraph_sizes: &shaped.paragraph_sizes,
            cache_key: &shaped.cache_key,
            empty_height,
        };
        let runs = parsed.runs.clone();
        self.layout_core(&copy, Some(&prepared), Some(&runs))
    }

    fn shape_rich(&mut self, runs: &[Run], params: &LayoutParams, base_size: f64, scale: f64) -> RichShaped {
        let mut paragraphs: Vec<Vec<Glyph>> = vec![Vec::new()];
        let mut paragraph_sizes: Vec<f64> = Vec::new();
        let mut cache_key = String::from("rich:");
        let mut segment: Vec<Codepoint> = Vec::new();
        let mut points: Vec<Codepoint> = Vec::new();
        let mut shaped: Vec<Glyph> = Vec::new();
        for (index, run) in runs.iter().enumerate() {
            let run_index = index as u32 + 1;
            let style = &run.style;
            let family = style.family.as_deref().unwrap_or(params.family);
            let weight = style.run_weight(params.weight);
            let font_style = style.run_style(params.style);
            let mut size = base_size;
            if let Some(run_size) = style.size {
                size = lmax(f32r(f32r(run_size * scale) * params.pixel_scale).floor(), 1.0);
            }
            let face = self.fonts.get_face(family, weight, font_style);
            if index > 0 {
                cache_key.push(';');
            }
            cache_key.push_str(&format!(
                "{family}/{:?}/{font_style}/{:?}/{}{}",
                weight,
                size,
                if style.uppercase { 1 } else { 0 },
                if style.small_caps { 1 } else { 0 }
            ));

            let upper;
            let text: &[u8] = if style.uppercase {
                upper = run.text.to_ascii_uppercase();
                &upper
            } else {
                &run.text
            };
            let extra: Option<&[&str]> = if style.small_caps { Some(&["smcp"]) } else { None };

            let mut flush_segment = |segment: &mut Vec<Codepoint>, paragraphs: &mut Vec<Vec<Glyph>>, fonts: &mut FontRegistry| {
                if let (false, Some(face)) = (segment.is_empty(), face) {
                    shaped.clear();
                    shape_paragraph(fonts, segment, face, size, extra, &mut shaped);
                    let paragraph = paragraphs.last_mut().unwrap();
                    for glyph in shaped.iter_mut() {
                        glyph.run = run_index;
                        paragraph.push(*glyph);
                    }
                }
                segment.clear();
            };
            points.clear();
            super::utf8::codepoints_into(text, &mut points);
            segment.clear();
            for &entry in &points {
                if entry.0 == 10 {
                    flush_segment(&mut segment, &mut paragraphs, &mut self.fonts);
                    paragraph_sizes.push(size);
                    paragraphs.push(Vec::new());
                } else if entry.0 != 13 {
                    segment.push(entry);
                }
            }
            flush_segment(&mut segment, &mut paragraphs, &mut self.fonts);
        }
        paragraph_sizes.push(base_size);
        round_pen(&mut paragraphs, self.subpixel);
        RichShaped { paragraphs, paragraph_sizes, cache_key }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A nominal advance is the font units times the 16.16 scale rounded to 16.16 and again to 26.6, as
    /// HarfBuzz reads FreeType's advance: one 1/64 px above FT_MulFix's single rounding where the first
    /// rounding carries (1108 units at scale 80530: 1362, not 1361).
    #[test]
    fn nominal_advances_round_twice() {
        use super::super::layout::{ft_advance, round_pen, Glyph};
        assert_eq!(ft_advance(1108.0, 80530.0), 1362.0);
        assert_eq!(ft_advance(500.0, 103439.0), 789.0);
        // the exact pens before and after each glyph stay beside the whole-pixel advances
        let glyph = |advance: f64| Glyph { glyph: 1, face: 0, x: 0.0, x_offset: 0.0, advance, cluster: 0, code: 97, size: 0.0, run: 0, pen: 0.0, end: 0.0, offset: 0.0 };
        let mut line = vec![glyph(795.0 / 64.0), glyph(795.0 / 64.0), glyph(316.0 / 64.0)];
        round_pen(std::slice::from_mut(&mut line), true);
        assert_eq!(line.iter().map(|glyph| glyph.pen * 64.0).collect::<Vec<_>>(), [0.0, 795.0, 1590.0]);
        assert_eq!(line[2].end * 64.0, 1906.0);
        assert_eq!(line.iter().map(|glyph| glyph.advance).collect::<Vec<_>>(), [12.0, 13.0, 5.0]);
        // whole-pixel text rounds each advance on its own (half up) and the pens are their sums
        let mut line = vec![glyph(795.0 / 64.0), glyph(795.0 / 64.0), glyph(316.0 / 64.0), glyph(7.5)];
        round_pen(std::slice::from_mut(&mut line), false);
        assert_eq!(line.iter().map(|glyph| glyph.advance).collect::<Vec<_>>(), [12.0, 12.0, 5.0, 8.0]);
        assert_eq!(line.iter().map(|glyph| glyph.pen).collect::<Vec<_>>(), [0.0, 12.0, 24.0, 29.0]);
        assert_eq!(line[3].end, 37.0);
    }

    #[test]
    fn the_text_mode_setting() {
        use super::super::subpixel_value;
        for on in ["on", "ON", " enabled ", "true", "1"] {
            assert!(subpixel_value(Some(on)), "{on}");
        }
        for off in ["off", "", "0", "false", "quarter"] {
            assert!(!subpixel_value(Some(off)), "{off}");
        }
        // unset: what production Roblox draws (FFlagLegacyTextSubpixelRendering off)
        assert!(!subpixel_value(None));
    }

    #[test]
    fn attributes_and_entities() {
        let attributes = parse_attributes(br#"font  color = "red" size='4' x="a'b" y=  ""#);
        assert_eq!(attributes.len(), 3);
        assert_eq!(attribute(&attributes, b"x"), Some(&b"a'b"[..]));
        assert_eq!(unescape(b"&amp;&lt&gt;&#39;&AMP;&quot;"), b"&&lt>&#39;&AMP;\"".to_vec());
        assert_eq!(parse_color(b"#ff8000"), Some([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(parse_color(b"xrgb( 1 ,2,3 )"), Some([1.0 / 255.0, 2.0 / 255.0, 3.0 / 255.0]));
        assert_eq!(parse_color(b"#fff"), None);
    }
}
