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
/// A numeric character reference's byte: `&#66;` and `&#x42;` are "B". Measured (a 0.747 client, fixtures/text-probe-tags
/// .luau): the client keeps the number's low byte only (`&#9731;` is the control character 0x03, not a snowman), and a
/// reference that is not a number (`&#xZZ;`) stays as it is written.
fn numeric_entity(text: &[u8]) -> Option<u8> {
    let (digits, radix) = match text.strip_prefix(b"#")? {
        [b'x', rest @ ..] => (rest, 16),
        rest => (rest, 10),
    };
    if digits.is_empty() || !digits.iter().all(|&byte| (byte as char).is_digit(radix)) {
        return None;
    }
    let value = digits.iter().fold(0u32, |value, &byte| value.wrapping_mul(radix).wrapping_add((byte as char).to_digit(radix).unwrap()));
    Some(value as u8)
}

fn unescape(text: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if text[index] == b'&' {
            if let Some(length) = text[index + 1..].iter().take(12).position(|&byte| byte == b';') {
                if let Some(byte) = numeric_entity(&text[index + 1..index + 1 + length]) {
                    out.push(byte);
                    index += length + 2;
                    continue;
                }
            }
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

/// XML's white space (a vertical tab is none: `<b\v>` makes the source show raw)
fn is_markup_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

/// XML's NameStartChar, bytewise: a letter, `_`, `:` or any non-ASCII byte (`<_x>`, `<a:b>`, `<é>` are tags; `<1>`,
/// `< b>` and `<>` are none)
fn is_name_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b':' || byte >= 0x80
}

/// XML's NameChar, bytewise (`<foo-bar>`, `<foo.bar>`, `<f1>`; `<foo$>` is no tag)
fn is_name_char(byte: u8) -> bool {
    is_name_start(byte) || byte.is_ascii_digit() || byte == b'-' || byte == b'.'
}

/// One tag of rich text markup, read as a 0.747 client reads it: XML's syntax.
struct Tag<'s> {
    closing: bool,
    self_closing: bool,
    name: &'s [u8],
    /// `name="value"` / `name='value'` pairs in source order; a repeated name keeps its first value
    attributes: Vec<(&'s [u8], &'s [u8])>,
    /// The index after the tag's `>`
    end: usize,
}

/// The tag at `source[start]` (a `<`), or None where the `<` starts no well-formed tag, which makes a client show the
/// whole source raw. Measured (fixtures/text-probe-tags.luau): a name follows the `<` or `</` at once; a closing tag
/// holds its name and white space only (`</b x="1">`, `</b/>` show raw); attributes are white-space separated
/// `name="value"` or `name='value'` (no value, no quotes, an open quote or none between two show raw), `=` may have
/// white space around it, and a quoted value may hold `>`; `/>` closes an empty tag (`<br/ >` shows raw).
fn read_tag(source: &[u8], start: usize) -> Option<Tag<'_>> {
    let at = |index: usize| source.get(index).copied();
    let mut index = start + 1;
    let closing = at(index) == Some(b'/');
    if closing {
        index += 1;
    }
    let name_start = index;
    if !at(index).is_some_and(is_name_start) {
        return None;
    }
    while at(index).is_some_and(is_name_char) {
        index += 1;
    }
    let name = &source[name_start..index];
    let mut attributes: Vec<(&[u8], &[u8])> = Vec::new();
    loop {
        let space_start = index;
        while at(index).is_some_and(is_markup_space) {
            index += 1;
        }
        match at(index)? {
            b'>' => return Some(Tag { closing, self_closing: false, name, attributes, end: index + 1 }),
            b'/' if !closing && at(index + 1) == Some(b'>') => return Some(Tag { closing, self_closing: true, name, attributes, end: index + 2 }),
            byte if !closing && index > space_start && is_name_start(byte) => {
                let key_start = index;
                while at(index).is_some_and(is_name_char) {
                    index += 1;
                }
                let key = &source[key_start..index];
                while at(index).is_some_and(is_markup_space) {
                    index += 1;
                }
                if at(index)? != b'=' {
                    return None;
                }
                index += 1;
                while at(index).is_some_and(is_markup_space) {
                    index += 1;
                }
                let quote = at(index).filter(|&byte| byte == b'"' || byte == b'\'')?;
                let value_start = index + 1;
                let value_end = value_start + source[value_start..].iter().position(|&byte| byte == quote)?;
                if !attributes.iter().any(|(existing, _)| *existing == key) {
                    attributes.push((key, &source[value_start..value_end]));
                }
                index = value_end + 1;
            }
            _ => return None,
        }
    }
}

/// An attribute's value by its exact name (`<font SIZE="30">` sets no size)
fn attribute<'s>(attributes: &[(&'s [u8], &'s [u8])], name: &[u8]) -> Option<&'s [u8]> {
    attributes.iter().find(|(key, _)| *key == name).map(|(_, value)| *value)
}

/// A whole number attribute value as a client takes it (`<font size>`): white space around it and a minus sign, no plus
/// sign, fraction, exponent, hex or unit (" 30", "30 ", "-5"; not "+30", "22.5", "1e1", "0x1E", "30px" or "")
fn whole_value(value: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(value).ok()?.trim_matches(|c: char| c.is_ascii() && is_markup_space(c as u8));
    let digits = text.strip_prefix('-').unwrap_or(text);
    (!digits.is_empty() && digits.bytes().all(|byte| byte.is_ascii_digit())).then(|| text.parse().ok()).flatten()
}

/// A number attribute value (`transparency`, `thickness`, `weight`): as [`whole_value`] with a decimal fraction (".5",
/// "0.5 "); its exponent and sign forms are assumed to follow `size`'s (not measured)
fn number_value(value: &[u8]) -> Option<f64> {
    let text = std::str::from_utf8(value).ok()?.trim_matches(|c: char| c.is_ascii() && is_markup_space(c as u8));
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    let digits = |part: &str| part.bytes().all(|byte| byte.is_ascii_digit());
    (!(whole.is_empty() && fraction.is_empty()) && digits(whole) && digits(fraction) && !(unsigned.ends_with('.') && whole.is_empty())).then(|| text.parse().ok()).flatten()
}

fn hex_digit(byte: u8) -> Option<f64> {
    (byte as char).to_digit(16).map(|digit| digit as f64)
}

/// A colour attribute value as 0..1 channels: "#rrggbb", "#rgb" (each digit doubled: "#F80" draws 255, 136, 0) or
/// "rgb(r, g, b)" (0-255), the whole value. Measured (a 0.747 client, fixtures/text-probe-tags.luau): anything else
/// ("FF0000" without its "#", "red", "x rgb(1,2,3)", "rgb(1,2)", "") makes the source show raw.
pub fn parse_color(value: &[u8]) -> Option<Color> {
    if let Some(hex) = value.strip_prefix(b"#") {
        if !hex.iter().all(|byte| byte.is_ascii_hexdigit()) {
            return None;
        }
        return match hex.len() {
            6 => {
                let channel = |index: usize| (hex_digit(hex[index]).unwrap() * 16.0 + hex_digit(hex[index + 1]).unwrap()) / 255.0;
                Some([channel(0), channel(2), channel(4)])
            }
            3 => {
                let channel = |index: usize| hex_digit(hex[index]).unwrap() * 17.0 / 255.0;
                Some([channel(0), channel(1), channel(2)])
            }
            _ => None,
        };
    }
    match_rgb(value)
}

/// "rgb%(%s*(%d+)%s*,%s*(%d+)%s*,%s*(%d+)%s*%)", the whole text
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
    (cursor == rest.len()).then_some(channels)
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

/// `RichText.Parse`. Markup a client rejects makes it show the raw source, which callers handle through the returned
/// valid flag. Measured (a 0.747 client, 138 cases in fixtures/text-probe-tags.luau): the markup is read as XML. Every
/// tag, known or not, needs its closing tag of exactly the same name (case and all: `<B>AW</b>` shows raw), or `/>`;
/// an unclosed, stray or crossed tag shows raw (`A<foo>V`, `A</foo>V`, `<b>A<foo>V</b>A</foo>`). Only the lowercase
/// names format: other names (`<foo>`, `<B>`, `<UC>`) are dropped with their text kept. `<br>` without its slash is a
/// tag like any other (`A<br>V` shows raw, `A<br>V</br>` breaks the line once). A `<` that starts no tag (`A < V`,
/// `<1>`, `<>`, a `<` at the end) and an unclosed comment show raw; a lone `>` and an unknown entity are text. A known
/// attribute's value must parse (`<font size="30px">`, `color="red"`, `transparency="x"`, stroke `joins="zzz"`
/// show raw); attributes are matched by exact name, a repeated one keeps its first value, unknown ones are ignored,
/// and so is an unknown `<font family>` (fonts fall back) where an unknown `<font face>` shows raw.
pub fn parse(fonts: &mut FontRegistry, source: &[u8]) -> (Vec<Run>, bool) {
    let mut runs: Vec<Run> = Vec::new();
    let mut stack: Vec<(Vec<u8>, Style)> = Vec::new();
    let mut current = Style::default();
    let mut buffer: Vec<u8> = Vec::new();

    // Measured (a 0.747 client, 1.5x: four faces, three texts, each character in a tag of its own): runs next to
    // each other with the same formatting are one run, laid out (kerned) as one: every glyph in the same
    // `<font color>`, `<b>`, `<font size>` or `<font transparency="0">`, or in `#F0F0F0` and `rgb(240,240,240)`
    // in turn, is as wide as the untagged text; alternating colours are not (48 of 108 TextBounds off by
    // 1 to 15 px before)
    fn push(runs: &mut Vec<Run>, text: Vec<u8>, style: &Style) {
        match runs.last_mut() {
            Some(last) if last.style == *style => last.text.extend_from_slice(&text),
            _ => runs.push(Run { text, style: style.clone() }),
        }
    }
    fn flush(runs: &mut Vec<Run>, buffer: &mut Vec<u8>, current: &Style) {
        if buffer.is_empty() {
            return;
        }
        push(runs, unescape(buffer), current);
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
        if character != b'<' {
            buffer.push(character);
            index += 1;
            continue;
        }
        if source[index..].starts_with(b"<!--") {
            match find(b"-->", index + 4) {
                Some(close) => {
                    index = close + 3;
                    continue;
                }
                None => return (runs, false),
            }
        }
        let Some(tag) = read_tag(source, index) else {
            return (runs, false);
        };
        flush(&mut runs, &mut buffer, &current);
        index = tag.end;
        if tag.closing {
            match stack.pop() {
                Some((name, style)) if name.as_slice() == tag.name => current = style,
                _ => return (runs, false),
            }
            continue;
        }
        let known = KNOWN_TAGS.contains(&tag.name);
        if tag.name == b"br" {
            push(&mut runs, b"\n".to_vec(), &current);
        }
        if tag.self_closing {
            continue;
        }
        let mut next = current.clone();
        if known {
            let attributes = &tag.attributes;
            let number = |name: &[u8], parse: fn(&[u8]) -> Option<f64>| -> Result<Option<f64>, ()> {
                attribute(attributes, name).map(|value| parse(value).ok_or(())).transpose()
            };
            let color = |name: &[u8]| -> Result<Option<Color>, ()> { attribute(attributes, name).map(|value| parse_color(value).ok_or(())).transpose() };
            let applied = (|| -> Result<(), ()> {
                match tag.name {
                    b"b" => next.bold = true,
                    b"i" => next.italic = true,
                    b"u" => next.underline = true,
                    b"s" => next.strikethrough = true,
                    b"uc" | b"uppercase" => next.uppercase = true,
                    b"sc" | b"smallcaps" => next.small_caps = true,
                    b"font" => {
                        if let Some(value) = color(b"color")? {
                            next.color = Some(value);
                        }
                        if let Some(size) = number(b"size", whole_value)? {
                            next.size = Some(size);
                        }
                        if let Some(face) = attribute(attributes, b"face") {
                            let (family, weight, style) = resolve_face(fonts, face).ok_or(())?;
                            next.family = Some(family);
                            next.face_weight = weight;
                            next.font_style = style;
                        }
                        if let Some(family) = attribute(attributes, b"family") {
                            next.family = Some(text_of(family));
                        }
                        if let Some(weight) = attribute(attributes, b"weight") {
                            next.weight = Some(number_value(weight).or_else(|| weight_keyword(weight)).ok_or(())?);
                        }
                        if let Some(transparency) = number(b"transparency", number_value)? {
                            next.transparency = Some(transparency);
                        }
                    }
                    b"stroke" => {
                        let joins = attribute(attributes, b"joins").unwrap_or(b"round");
                        if !matches!(joins.to_ascii_lowercase().as_slice(), b"round" | b"bevel" | b"miter") {
                            return Err(());
                        }
                        next.stroke = Some(Stroke {
                            color: Some(color(b"color")?.unwrap_or([0.0, 0.0, 0.0])),
                            thickness: number(b"thickness", number_value)?.unwrap_or(1.0),
                            transparency: number(b"transparency", number_value)?.unwrap_or(0.0),
                            joins: text_of(joins),
                        });
                    }
                    b"mark" => {
                        next.mark = Some(Mark {
                            color: Some(color(b"color")?.unwrap_or([1.0, 1.0, 0.0])),
                            transparency: number(b"transparency", number_value)?.unwrap_or(0.0),
                        });
                    }
                    _ => {}
                }
                Ok(())
            })();
            if applied.is_err() {
                return (runs, false);
            }
        }
        stack.push((tag.name.to_vec(), std::mem::replace(&mut current, next)));
    }
    flush(&mut runs, &mut buffer, &current);

    let valid = stack.is_empty();
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
            // Measured (Roblox 0.747 client, 1.5x: TextSize 1, sizes UIScale floors to 1 px, a <font size="1"> line,
            // LineHeight 2; 24 of 24 values): a rich line of 1 px glyphs is 2 px tall (plain text: 1 px); a size
            // TextScaled picks keeps its 1 px line (rich TextScaled in 1 .. 1.5 px boxes)
            min_line: if params.pixel_size.is_none() { 2.0 } else { 0.0 },
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
        let tag = read_tag(br#"<font  color = "red" size='4' x="a'b" size="5">"#, 0).unwrap();
        assert_eq!((tag.name, tag.closing, tag.self_closing), (&b"font"[..], false, false));
        assert_eq!(tag.attributes.len(), 3);
        assert_eq!(attribute(&tag.attributes, b"x"), Some(&b"a'b"[..]));
        assert_eq!(attribute(&tag.attributes, b"size"), Some(&b"4"[..]));
        assert_eq!(unescape(b"&amp;&lt&gt;&#39;&AMP;&quot;&#x42;&#9731;&#xZZ;"), b"&&lt>'&AMP;\"B\x03&#xZZ;".to_vec());
        assert_eq!(parse_color(b"#ff8000"), Some([1.0, 128.0 / 255.0, 0.0]));
        assert_eq!(parse_color(b"#F80"), Some([1.0, 136.0 / 255.0, 0.0]));
        assert_eq!(parse_color(b"rgb( 1 ,2,3 )"), Some([1.0 / 255.0, 2.0 / 255.0, 3.0 / 255.0]));
        for bad in [&b"xrgb(1,2,3)"[..], b"ff8000", b"red", b"", b"rgb(1,2)", b"#ff80"] {
            assert_eq!(parse_color(bad), None, "{}", String::from_utf8_lossy(bad));
        }
        assert_eq!([whole_value(b" 30"), whole_value(b"30 "), whole_value(b"-5")], [Some(30.0), Some(30.0), Some(-5.0)]);
        assert!([&b"+30"[..], b"22.5", b"1e1", b"0x1E", b"30px", b""].iter().all(|value| whole_value(value).is_none()));
        assert_eq!([number_value(b".5"), number_value(b"0.5 "), number_value(b"700")], [Some(0.5), Some(0.5), Some(700.0)]);
        assert!([&b"x"[..], b".", b"-", b"1e1", b""].iter().all(|value| number_value(value).is_none()));
    }

    /// What a 0.747 client shows of rich markup (fixtures/text-probe-tags.luau, read back as each label's ContentText):
    /// the raw source where it rejects the markup, else the text without its tags.
    #[test]
    fn markup_a_client_rejects_shows_its_source() {
        let dir = std::env::temp_dir().to_string_lossy().replace('\\', "/");
        let mut fonts = FontRegistry::new(super::super::fonts::FontPaths { root: dir.clone(), cache: dir, content_root: Some(String::new()), fonts: None });
        let cases: &[(&str, &str)] = &[
            ("A<foo>W<foo>A", "A<foo>W<foo>A"),
            ("A<foo>W</foo>A", "AWA"),
            ("A<foo/>W", "AW"),
            ("A</foo>W", "A</foo>W"),
            ("<b>A<foo>W</b>A</foo>", "<b>A<foo>W</b>A</foo>"),
            ("<foo x=\"1\">AW</foo>", "AW"),
            ("<FOO>AW</foo>", "<FOO>AW</foo>"),
            ("<b>A<foo>W</foo></b>", "AW"),
            ("<foo>A<bar>W</bar></foo>", "AW"),
            ("A<foo>W", "A<foo>W"),
            ("<foo>", "<foo>"),
            ("A<b>W<foo></b>", "A<b>W<foo></b>"),
            ("A<foo>W</bar>", "A<foo>W</bar>"),
            ("A<foo-bar>W</foo-bar>", "AW"),
            ("A<foo>W</foo >", "AW"),
            ("A<foo>W<foo>A<foo>W<foo>A<foo>T<foo>o<foo>", "A<foo>W<foo>A<foo>W<foo>A<foo>T<foo>o<foo>"),
            ("A<foo x=\"1\"/>W", "AW"),
            ("<foo><foo>A</foo></foo>", "A"),
            ("<b>A</b></foo>", "<b>A</b></foo>"),
            ("<foo x=\">\">A</foo>", "A"),
            ("A<foo />W", "AW"),
            ("<b>A<i>W</b></i>", "<b>A<i>W</b></i>"),
            ("<b>AW", "<b>AW"),
            ("A</b>W", "A</b>W"),
            ("<B>AW</b>", "<B>AW</b>"),
            ("A<b>W</B>", "A<b>W</B>"),
            ("A<b/>W", "AW"),
            ("A<i/>W</i>", "A<i/>W</i>"),
            ("A<b>W</b></b>", "A<b>W</b></b>"),
            ("<B>AW</B>", "AW"),
            ("<FONT size=\"30\">AW</FONT>", "AW"),
            ("<UC>av</UC>", "av"),
            ("<uc>av</uc>", "av"),
            ("<sc>Av</SC>", "<sc>Av</SC>"),
            ("<font>AW</font>", "AW"),
            ("A<b >W</b >", "AW"),
            ("<font size=\"30\">A</font >", "A"),
            ("<font size=\"30\">A", "<font size=\"30\">A"),
            ("<b>AW</b>", "AW"),
            ("AW", "AW"),
            ("A<br/>W", "A\nW"),
            ("A<br>W", "A<br>W"),
            ("A<br>W</br>", "A\nW"),
            ("A<br />W", "A\nW"),
            ("A<br/ >W", "A<br/ >W"),
            ("A<br x=\"1\"/>W", "A\nW"),
            ("<b>A<br>W</b>", "<b>A<br>W</b>"),
            ("A < W", "A < W"),
            ("<b>A</b><1>", "<b>A</b><1>"),
            ("<b>A</b><1>W</1>", "<b>A</b><1>W</1>"),
            ("<b>A</b>< b>", "<b>A</b>< b>"),
            ("<b>A</b><>", "<b>A</b><>"),
            ("<b>A</b> < W", "<b>A</b> < W"),
            ("<b>A</b><", "<b>A</b><"),
            ("<b>A</b>>", "A>"),
            ("<b>A</b><foo", "<b>A</b><foo"),
            ("<b>A</b></>", "<b>A</b></>"),
            ("<b>A</b>1<2", "<b>A</b>1<2"),
            ("A<>W", "A<>W"),
            ("A<1>W</1>", "A<1>W</1>"),
            ("A< b>W</ b>", "A< b>W</ b>"),
            ("<b>A</b>&foo;", "A&foo;"),
            ("&foo;AW", "&foo;AW"),
            ("A<!--x-->W", "AW"),
            ("A<!--x W", "A<!--x W"),
            ("<b>A</b><!--x", "<b>A</b><!--x"),
            ("<foo>A<!--x-->W</foo>", "AW"),
            ("<foo-bar>W</foo>", "<foo-bar>W</foo>"),
            ("<foo.bar>W</foo.bar>", "W"),
            ("<foo_bar>W</foo_bar>", "W"),
            ("<f1>W</f1>", "W"),
            ("<b>A</b><_x>W</_x>", "AW"),
            ("<b>A</b><a:b>W</a:b>", "AW"),
            ("<b>A</b><foo$>W</foo$>", "<b>A</b><foo$>W</foo$>"),
            ("<b>A</b><x->W</x->", "AW"),
            ("<b\t>A</b>W", "AW"),
            ("<b\n>A</b>W", "AW"),
            ("<b>A< /b>W", "<b>A< /b>W"),
            ("<b>A</ b>W", "<b>A</ b>W"),
            ("<b>A</b x=\"1\">W", "<b>A</b x=\"1\">W"),
            ("<b>A</b/>W", "<b>A</b/>W"),
            ("<font size=30>A</font>W", "<font size=30>A</font>W"),
            ("<font size='30'>A</font>W", "AW"),
            ("<b>A</b><foo x='>'>W</foo>", "AW"),
            ("<font size=\"30>A</font>W", "<font size=\"30>A</font>W"),
            ("<b>A</b><foo bar>W</foo>", "<b>A</b><foo bar>W</foo>"),
            ("<b x>A</b>W", "<b x>A</b>W"),
            ("<font size=\"x\">A</font>W", "<font size=\"x\">A</font>W"),
            ("<font size=\"30\"color=\"#FF0000\">A</font>W", "<font size=\"30\"color=\"#FF0000\">A</font>W"),
            ("<font size = \"30\">A</font>W", "AW"),
            ("<b>A</b><foo a-b=\"1\">W</foo>", "AW"),
            ("<font SIZE=\"30\">A</font>W", "AW"),
            ("<font foo=\"1\">A</font>W", "AW"),
            ("<b x=\"1\">A</b>W", "AW"),
            ("<font color=\"zzz\">A</font>W", "<font color=\"zzz\">A</font>W"),
            ("<font color=\"\">A</font>W", "<font color=\"\">A</font>W"),
            ("<font color=\"rgb(1,2)\">A</font>W", "<font color=\"rgb(1,2)\">A</font>W"),
            ("<font color=\"rgb(255, 0, 0)\">A</font>W", "AW"),
            ("<font color=\"FF0000\">A</font>W", "<font color=\"FF0000\">A</font>W"),
            ("<font color=\"#F80\">A</font>W", "AW"),
            ("<font color=\"red\">A</font>W", "<font color=\"red\">A</font>W"),
            ("<font color=\"x rgb(1,2,3)\">A</font>W", "<font color=\"x rgb(1,2,3)\">A</font>W"),
            ("<font transparency=\"x\">A</font>W", "<font transparency=\"x\">A</font>W"),
            ("<font transparency=\".5\">A</font>W", "AW"),
            ("<font transparency=\"0.5 \">A</font>W", "AW"),
            ("<font weight=\"zzz\">A</font>W", "<font weight=\"zzz\">A</font>W"),
            ("<font weight=\"bold\">A</font>W", "AW"),
            ("<font weight=\"Bold\">A</font>W", "AW"),
            ("<font weight=\"700\">A</font>W", "AW"),
            ("<font face=\"Nope\">A</font>W", "<font face=\"Nope\">A</font>W"),
            ("<font family=\"rbxasset://fonts/families/Nope.json\">A</font>W", "AW"),
            ("<font size=\"30px\">A</font>W", "<font size=\"30px\">A</font>W"),
            ("<font size=\" 30\">A</font>W", "AW"),
            ("<font size=\"30 \">A</font>W", "AW"),
            ("<font size=\"\">A</font>W", "<font size=\"\">A</font>W"),
            ("<font size=\"1e1\">A</font>W", "<font size=\"1e1\">A</font>W"),
            ("<font size=\"22.5\">A</font>W", "<font size=\"22.5\">A</font>W"),
            ("<font size=\"+30\">A</font>W", "<font size=\"+30\">A</font>W"),
            ("<font size=\"0x1E\">A</font>W", "<font size=\"0x1E\">A</font>W"),
            ("<font size=\"30\" size=\"20\">A</font>W", "AW"),
            ("<stroke thickness=\"x\">A</stroke>W", "<stroke thickness=\"x\">A</stroke>W"),
            ("<stroke color=\"zzz\">A</stroke>W", "<stroke color=\"zzz\">A</stroke>W"),
            ("<stroke transparency=\"x\">A</stroke>W", "<stroke transparency=\"x\">A</stroke>W"),
            ("<stroke joins=\"zzz\">A</stroke>W", "<stroke joins=\"zzz\">A</stroke>W"),
            ("<mark color=\"zzz\">A</mark>W", "<mark color=\"zzz\">A</mark>W"),
            ("<mark transparency=\"x\">A</mark>W", "<mark transparency=\"x\">A</mark>W"),
            ("<b>A</b><é>W</é>", "AW"),
            ("<b>A</b><:x>W</:x>", "AW"),
            ("<b\u{b}>A</b>W", "<b\u{b}>A</b>W"),
            ("<b>A</b\t>W", "AW"),
            ("<b>A</b> & W", "A & W"),
            ("<b>A</b>&amp;", "A&"),
            ("<b>A</b>&#66;", "AB"),
            ("<b>A</b>&#x42;", "AB"),
            ("<b>A</b>&#9731;", "A\u{3}"),
            ("<b>A</b>&#xZZ;", "A&#xZZ;"),
            ("<b>A</b>&lt", "A&lt"),
            ("<b>A</b>&LT;", "A&LT;"),
        ];
        for &(source, content) in cases {
            let (runs, valid) = parse(&mut fonts, source.as_bytes());
            let shown: Vec<u8> = if valid { runs.iter().flat_map(|run| run.text.iter().copied()).collect() } else { source.as_bytes().to_vec() };
            assert_eq!(String::from_utf8_lossy(&shown), content, "{source:?}");
        }
        // Only lowercase names format; attributes are matched by exact name, the first of a repeated one counts
        let style = |fonts: &mut FontRegistry, source: &str| parse(fonts, source.as_bytes()).0.first().unwrap().style.clone();
        assert!(!style(&mut fonts, "<B>AW</B>").bold);
        assert_eq!(style(&mut fonts, "<font SIZE=\"30\">A</font>V").size, None);
        assert_eq!(style(&mut fonts, "<font size=\"30\" size=\"20\">A</font>V").size, Some(30.0));
        assert_eq!(parse(&mut fonts, b"A<br>V</br>").0.iter().map(|run| String::from_utf8_lossy(&run.text).into_owned()).collect::<Vec<_>>(), ["A\nV"]);
    }

    #[test]
    fn runs_of_the_same_formatting_next_to_each_other_are_one_run() {
        let dir = std::env::temp_dir().to_string_lossy().replace('\\', "/");
        let mut fonts = FontRegistry::new(super::super::fonts::FontPaths { root: dir.clone(), cache: dir, content_root: Some(String::new()), fonts: None });
        let mut texts = |source: &str| -> Vec<String> {
            let (runs, valid) = parse(&mut fonts, source.as_bytes());
            assert!(valid, "{source}");
            runs.iter().map(|run| String::from_utf8_lossy(&run.text).into_owned()).collect()
        };
        // The same tags in turn, and the same colour written two ways: one run, laid out (kerned) as one
        assert_eq!(texts("<b>T</b><b>o</b><b>T</b>"), ["ToT"]);
        assert_eq!(texts("<font color=\"#F0F0F0\">T</font><font color=\"rgb(240,240,240)\">o</font>"), ["To"]);
        assert_eq!(texts("<font transparency=\"0\">T</font><font transparency=\"0\">o<br/>W</font>"), ["To\nW"]);
        assert_eq!(texts("<i>T</i><br/><i>o</i>"), ["T", "\n", "o"]);
        assert_eq!(texts("T<b></b>o"), ["To"]);
        // Other formatting, or a tag next to untagged text that sets the label's own value, stays apart
        assert_eq!(texts("<font color=\"#FF2020\">T</font><font color=\"#20FF20\">o</font>"), ["T", "o"]);
        assert_eq!(texts("T<font transparency=\"0\">o</font>"), ["T", "o"]);
    }
}
