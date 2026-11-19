//! What the Node CLI's JavaScript did with values, for the commands that moved from it into rur-native (`sweep`,
//! `diff`): JSON whose objects keep their keys in file order and are written as `JSON.stringify` writes them,
//! `Number(text)`, `Number#toFixed`, `String#slice` (UTF-16 units), `String#localeCompare`'s order and JavaScript's
//! truthiness. Their outputs (report.json, summary.md, summary.json, the result on stdout) read as they did.
use std::cmp::Ordering;

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

/// A JSON value whose objects keep their keys in insertion order (serde_json's own map sorts them)
#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    Object(Vec<(String, Json)>),
}

static NULL: Json = Json::Null;

impl Json {
    /// An empty object
    pub fn object() -> Json {
        Json::Object(Vec::new())
    }

    /// `JSON.parse(text)`, keys in file order (a repeated key keeps its first place and its last value)
    pub fn parse(text: &str) -> Result<Json, String> {
        serde_json::from_str::<Json>(text.trim_start_matches('\u{feff}')).map_err(|error| error.to_string())
    }

    /// The file's JSON, or None when it can't be read or parsed (cli/rur.mjs readJson)
    pub fn read(file: &std::path::Path) -> Option<Json> {
        Json::parse(&std::fs::read_to_string(file).ok()?).ok()
    }

    /// `value[key]`: Null for a missing key or a value that isn't an object
    pub fn get(&self, key: &str) -> &Json {
        match self {
            Json::Object(entries) => entries.iter().find(|(name, _)| name == key).map_or(&NULL, |(_, value)| value),
            _ => &NULL,
        }
    }

    /// Whether the object has `key`
    pub fn has(&self, key: &str) -> bool {
        matches!(self, Json::Object(entries) if entries.iter().any(|(name, _)| name == key))
    }

    /// Sets `key` (in its place when the object has it, else at the end), as `object[key] = value`
    pub fn set(&mut self, key: &str, value: impl Into<Json>) {
        if let Json::Object(entries) = self {
            let value = value.into();
            match entries.iter_mut().find(|(name, _)| name == key) {
                Some(entry) => entry.1 = value,
                None => entries.push((key.to_string(), value)),
            }
        }
    }

    /// `object` with `key` set, for building objects in their key order
    pub fn with(mut self, key: &str, value: impl Into<Json>) -> Json {
        self.set(key, value);
        self
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(text) => Some(text),
            _ => None,
        }
    }

    pub fn as_f64(&self) -> Option<f64> {
        match self {
            Json::Number(number) => Some(*number),
            _ => None,
        }
    }

    /// `asArray(value)`: the array's items, none for anything else
    pub fn items(&self) -> &[Json] {
        match self {
            Json::Array(items) => items,
            _ => &[],
        }
    }

    pub fn entries(&self) -> &[(String, Json)] {
        match self {
            Json::Object(entries) => entries,
            _ => &[],
        }
    }

    /// JavaScript's truthiness
    pub fn truthy(&self) -> bool {
        match self {
            Json::Null => false,
            Json::Bool(flag) => *flag,
            Json::Number(number) => *number != 0.0 && !number.is_nan(),
            Json::String(text) => !text.is_empty(),
            Json::Array(_) | Json::Object(_) => true,
        }
    }

    /// `String(value)` for the values a report prints (an object prints as JavaScript prints one)
    pub fn text(&self) -> String {
        match self {
            Json::Null => "null".into(),
            Json::Bool(flag) => flag.to_string(),
            Json::Number(number) => crate::glyphs::js_number_string(*number),
            Json::String(text) => text.clone(),
            Json::Array(items) => items.iter().map(|item| if matches!(item, Json::Null) { String::new() } else { item.text() }).collect::<Vec<_>>().join(","),
            Json::Object(_) => "[object Object]".into(),
        }
    }

    /// `JSON.stringify(value, null, indent)` (indent 0: one line)
    pub fn stringify(&self, indent: usize) -> String {
        let mut out = String::new();
        self.write(&mut out, indent, 0);
        out
    }

    fn write(&self, out: &mut String, indent: usize, depth: usize) {
        let newline = |out: &mut String, depth: usize| {
            if indent > 0 {
                out.push('\n');
                out.extend(std::iter::repeat_n(' ', indent * depth));
            }
        };
        match self {
            Json::Null => out.push_str("null"),
            Json::Bool(flag) => out.push_str(if *flag { "true" } else { "false" }),
            Json::Number(number) if number.is_finite() => out.push_str(&crate::glyphs::js_number_string(*number)),
            Json::Number(_) => out.push_str("null"),
            Json::String(text) => out.push_str(&serde_json::to_string(text).expect("a string")),
            Json::Array(items) if items.is_empty() => out.push_str("[]"),
            Json::Array(items) => {
                out.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    newline(out, depth + 1);
                    item.write(out, indent, depth + 1);
                }
                newline(out, depth);
                out.push(']');
            }
            Json::Object(entries) if entries.is_empty() => out.push_str("{}"),
            Json::Object(entries) => {
                out.push('{');
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    newline(out, depth + 1);
                    out.push_str(&serde_json::to_string(key).expect("a string"));
                    out.push(':');
                    if indent > 0 {
                        out.push(' ');
                    }
                    value.write(out, indent, depth + 1);
                }
                newline(out, depth);
                out.push('}');
            }
        }
    }
}

impl From<&str> for Json {
    fn from(text: &str) -> Json {
        Json::String(text.to_string())
    }
}

impl From<String> for Json {
    fn from(text: String) -> Json {
        Json::String(text)
    }
}

impl From<bool> for Json {
    fn from(flag: bool) -> Json {
        Json::Bool(flag)
    }
}

impl From<f64> for Json {
    fn from(number: f64) -> Json {
        Json::Number(number)
    }
}

impl From<usize> for Json {
    fn from(number: usize) -> Json {
        Json::Number(number as f64)
    }
}

impl From<Vec<Json>> for Json {
    fn from(items: Vec<Json>) -> Json {
        Json::Array(items)
    }
}

impl From<Vec<String>> for Json {
    fn from(items: Vec<String>) -> Json {
        Json::Array(items.into_iter().map(Json::String).collect())
    }
}

impl<T: Into<Json>> From<Option<T>> for Json {
    fn from(value: Option<T>) -> Json {
        value.map_or(Json::Null, Into::into)
    }
}

impl From<&std::path::Path> for Json {
    fn from(path: &std::path::Path) -> Json {
        Json::String(path.display().to_string())
    }
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Json, D::Error> {
        struct JsonVisitor;
        impl<'de> Visitor<'de> for JsonVisitor {
            type Value = Json;
            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("JSON")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Json, A::Error> {
                let mut entries: Vec<(String, Json)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, Json>()? {
                    match entries.iter_mut().find(|(name, _)| *name == key) {
                        Some(entry) => entry.1 = value,
                        None => entries.push((key, value)),
                    }
                }
                Ok(Json::Object(entries))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Json, A::Error> {
                let mut items = Vec::new();
                while let Some(item) = seq.next_element::<Json>()? {
                    items.push(item);
                }
                Ok(Json::Array(items))
            }
            fn visit_str<E>(self, value: &str) -> Result<Json, E> {
                Ok(Json::String(value.to_string()))
            }
            fn visit_string<E>(self, value: String) -> Result<Json, E> {
                Ok(Json::String(value))
            }
            fn visit_bool<E>(self, value: bool) -> Result<Json, E> {
                Ok(Json::Bool(value))
            }
            fn visit_i64<E>(self, value: i64) -> Result<Json, E> {
                Ok(Json::Number(value as f64))
            }
            fn visit_u64<E>(self, value: u64) -> Result<Json, E> {
                Ok(Json::Number(value as f64))
            }
            fn visit_f64<E>(self, value: f64) -> Result<Json, E> {
                Ok(Json::Number(value))
            }
            fn visit_unit<E>(self) -> Result<Json, E> {
                Ok(Json::Null)
            }
            fn visit_none<E>(self) -> Result<Json, E> {
                Ok(Json::Null)
            }
        }
        deserializer.deserialize_any(JsonVisitor)
    }
}

/// `Number(text)`: decimal (with an exponent), `0x` / `0o` / `0b` integers and `Infinity`; blank is 0, anything else
/// NaN
pub fn number(text: &str) -> f64 {
    let text = text.trim();
    if text.is_empty() {
        return 0.0;
    }
    let radix = |prefix: &str, radix: u32| {
        text.get(..2)
            .filter(|start| start.eq_ignore_ascii_case(prefix))
            .map(|_| u64::from_str_radix(&text[2..], radix).map_or(f64::NAN, |value| value as f64))
    };
    if let Some(value) = radix("0x", 16).or_else(|| radix("0o", 8)).or_else(|| radix("0b", 2)) {
        return value;
    }
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    if unsigned == "Infinity" {
        return if text.starts_with('-') { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    // (Rust also reads "inf" and "nan", which JavaScript does not)
    if !unsigned.starts_with(|c: char| c.is_ascii_digit() || c == '.') {
        return f64::NAN;
    }
    text.parse().unwrap_or(f64::NAN)
}

/// `value.toFixed(digits)`: the exact value rounded half away from zero (Rust's `{:.N}` rounds a tie to even)
pub fn to_fixed(value: f64, digits: usize) -> String {
    if !value.is_finite() {
        return crate::glyphs::js_number_string(value);
    }
    if value.abs() >= 1e21 {
        return crate::glyphs::js_number_string(value);
    }
    // Every digit of the double: the expansion is exact well before 1100 places
    let exact = format!("{:.1100}", value.abs());
    let (whole, fraction) = exact.split_once('.').unwrap();
    let mut kept: Vec<u8> = whole.bytes().chain(fraction.bytes().take(digits)).collect();
    if fraction.as_bytes()[digits] >= b'5' {
        let mut index = kept.len();
        loop {
            if index == 0 {
                kept.insert(0, b'1');
                break;
            }
            index -= 1;
            if kept[index] == b'9' {
                kept[index] = b'0';
            } else {
                kept[index] += 1;
                break;
            }
        }
    }
    let split = kept.len() - digits;
    let mut text = String::from_utf8(kept[..split].to_vec()).unwrap();
    if digits > 0 {
        text.push('.');
        text.push_str(std::str::from_utf8(&kept[split..]).unwrap());
    }
    // (a negative value keeps its sign even when it rounds to zero; -0 has none)
    if value < 0.0 { format!("-{text}") } else { text }
}

/// The byte index of the char boundary at `units` UTF-16 units into `text` (a char the cut would split is left out)
fn utf16_index(text: &str, units: usize) -> usize {
    let mut counted = 0;
    for (index, c) in text.char_indices() {
        if counted + c.len_utf16() > units {
            return index;
        }
        counted += c.len_utf16();
    }
    text.len()
}

/// `text.slice(0, units)` (UTF-16 units)
pub fn slice_start(text: &str, units: usize) -> &str {
    &text[..utf16_index(text, units)]
}

/// `text.slice(-units)` (UTF-16 units)
pub fn slice_end(text: &str, units: usize) -> &str {
    let length: usize = text.chars().map(char::len_utf16).sum();
    if length <= units {
        return text;
    }
    let skip = length - units;
    let mut counted = 0;
    for (index, c) in text.char_indices() {
        if counted >= skip {
            return &text[index..];
        }
        counted += c.len_utf16();
    }
    ""
}

/// JavaScript's `\s`
pub fn is_space(c: char) -> bool {
    matches!(c, '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}' | '\u{205f}' | '\u{3000}' | '\u{feff}')
}

/// JavaScript's `\w`
pub fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// The ASCII symbols in the order `localeCompare` sorts them (the CLDR root collation Node's ICU uses), before the
/// digits and the letters
const SYMBOL_ORDER: &str = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$";

/// A char's primary weight: symbols, digits, then letters with case ignored; other chars after them by code point
fn primary(c: char) -> u32 {
    if let Some(index) = SYMBOL_ORDER.find(c) {
        return 1 + index as u32;
    }
    match c {
        '0'..='9' => 100 + (c as u32 - '0' as u32),
        'a'..='z' => 200 + (c as u32 - 'a' as u32),
        'A'..='Z' => 200 + (c as u32 - 'A' as u32),
        c if (c as u32) < 32 => 0,
        c => 1000 + c as u32,
    }
}

/// `a.localeCompare(b)` for the strings rur sorts (paths and story keys): letters before case, then lowercase first
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    let primaries = |text: &str| text.chars().map(primary).collect::<Vec<_>>();
    primaries(a).cmp(&primaries(b)).then_with(|| {
        let cases = |text: &str| text.chars().map(|c| c.is_ascii_uppercase()).collect::<Vec<_>>();
        cases(a).cmp(&cases(b))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_keeps_its_key_order_and_is_written_as_json_stringify_writes_it() {
        let value = Json::parse(r#"{"b": 1, "a": [1.5, "x", null, true, {}], "c": {"z": 1e21, "y": 0.1}, "b": 2, "d": []}"#).unwrap();
        assert_eq!(value.stringify(0), r#"{"b":2,"a":[1.5,"x",null,true,{}],"c":{"z":1e+21,"y":0.1},"d":[]}"#);
        assert_eq!(value.stringify(1), "{\n \"b\": 2,\n \"a\": [\n  1.5,\n  \"x\",\n  null,\n  true,\n  {}\n ],\n \"c\": {\n  \"z\": 1e+21,\n  \"y\": 0.1\n },\n \"d\": []\n}");
        assert_eq!(Json::from("tab\t\"q\"\u{1}").stringify(0), r#""tab\t\"q\"\u0001""#);
        assert_eq!(Json::Number(f64::NAN).stringify(0), "null");
        let mut object = Json::object().with("x", 1.0).with("y", "two");
        object.set("x", 3.0);
        object.set("z", Json::Null);
        assert_eq!(object.stringify(0), r#"{"x":3,"y":"two","z":null}"#);
    }

    #[test]
    fn numbers_read_and_print_as_javascript_does() {
        assert_eq!(number(" 12 "), 12.0);
        assert_eq!(number(""), 0.0);
        assert_eq!(number("0x10"), 16.0);
        assert_eq!(number("1e3"), 1000.0);
        assert_eq!(number("-Infinity"), f64::NEG_INFINITY);
        assert!(number("inf").is_nan() && number("12px").is_nan() && number("nan").is_nan());
        assert_eq!(to_fixed(1.25, 1), "1.3");
        assert_eq!(to_fixed(1.35, 1), "1.4");
        assert_eq!(to_fixed(0.045, 2), "0.04");
        assert_eq!(to_fixed(99.96, 1), "100.0");
        assert_eq!(to_fixed(2.5, 0), "3");
        assert_eq!(to_fixed(-0.04, 1), "-0.0");
        assert_eq!(to_fixed(-0.0, 1), "0.0");
        assert_eq!(to_fixed(0.5, 0), "1");
        assert_eq!(to_fixed(-1.25, 1), "-1.3");
        assert_eq!(to_fixed(12.0, 3), "12.000");
    }

    #[test]
    fn strings_slice_by_utf16_units_and_sort_as_locale_compare() {
        assert_eq!(slice_start("ab😀c", 3), "ab");
        assert_eq!(slice_start("ab😀c", 4), "ab😀");
        assert_eq!(slice_end("ab😀c", 3), "😀c");
        assert_eq!(slice_end("abc", 9), "abc");
        let mut keys = vec!["b/c", "bc", "B", "a10", "b_c", "a9", "b.c", "Ab", "aB", "a-b", "ab"];
        keys.sort_by(|a, b| locale_compare(a, b));
        // (node -e: the same list sorted with localeCompare)
        assert_eq!(keys, ["a-b", "a10", "a9", "ab", "aB", "Ab", "B", "b_c", "b.c", "b/c", "bc"]);
    }
}
