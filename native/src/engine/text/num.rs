//! Luau number semantics the text engine relies on: every Luau number is an f64, `//` is floor
//! division, `%` is floored modulo, `math.max` / `math.min` / `math.clamp` keep their NaN behaviour,
//! float32 round trips go through `buffer.writef32`, and `tonumber` parses like `luaO_str2d`.

/// `TextLayout.Float32`: rounds to single precision (round to nearest even, like `buffer.writef32`).
#[inline]
pub fn f32r(value: f64) -> f64 {
    value as f32 as f64
}

/// The next float32 above a non-negative float32 value (`NextFloat32` in runtime/layout/init.luau).
#[inline]
pub fn next_f32(value: f64) -> f64 {
    f32::from_bits((value as f32).to_bits().wrapping_add(1)) as f64
}

/// `math.max(a, b)`: the first argument unless the second is greater (NaN stays when first).
#[inline]
pub fn lmax(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

/// `math.min(a, b)`.
#[inline]
pub fn lmin(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// `math.clamp(v, min, max)` (Luau raises when min > max; callers never pass that).
#[inline]
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    let result = if value < min { min } else { value };
    if result > max { max } else { result }
}

/// Luau `a // b`.
#[inline]
pub fn idiv(a: f64, b: f64) -> f64 {
    (a / b).floor()
}

/// Luau `a % b`.
#[inline]
pub fn lmod(a: f64, b: f64) -> f64 {
    a - (a / b).floor() * b
}

#[inline]
fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | 0x0B | 0x0C | b'\r')
}

/// C `isspace` in the C locale (Lua patterns' `%s`).
#[inline]
pub fn c_isspace(byte: u8) -> bool {
    is_space(byte)
}

/// `tonumber(s)` for a string: `luaO_str2d` (strtod, then a hexadecimal integer retry, trailing
/// whitespace allowed). The string ends at its first NUL like the C string it is read as.
pub fn tonumber(source: &[u8]) -> Option<f64> {
    let end = source.iter().position(|&byte| byte == 0).unwrap_or(source.len());
    let text = &source[..end];
    let (mut value, mut consumed) = strtod(text)?;
    if consumed < text.len() && (text[consumed] == b'x' || text[consumed] == b'X') {
        let (retry, retry_end) = strtoul16(text);
        value = retry;
        consumed = retry_end;
    }
    if text[consumed..].iter().all(|&byte| is_space(byte)) {
        Some(value)
    } else {
        None
    }
}

fn strtoul16(text: &[u8]) -> (f64, usize) {
    let mut index = 0;
    while index < text.len() && is_space(text[index]) {
        index += 1;
    }
    let mut negative = false;
    if index < text.len() && (text[index] == b'+' || text[index] == b'-') {
        negative = text[index] == b'-';
        index += 1;
    }
    if index + 1 < text.len() && text[index] == b'0' && (text[index + 1] | 0x20) == b'x' {
        if index + 2 < text.len() && (text[index + 2] as char).is_ascii_hexdigit() {
            index += 2;
        }
    }
    let start = index;
    let mut value: u64 = 0;
    while index < text.len() && (text[index] as char).is_ascii_hexdigit() {
        value = value.saturating_mul(16).saturating_add((text[index] as char).to_digit(16).unwrap() as u64);
        index += 1;
    }
    if index == start {
        return (0.0, 0);
    }
    let value = value.min(u32::MAX as u64) as f64;
    (if negative { -value } else { value }, index)
}

/// strtod: returns the value and the number of bytes consumed (None when nothing converts).
fn strtod(text: &[u8]) -> Option<(f64, usize)> {
    let mut index = 0;
    while index < text.len() && is_space(text[index]) {
        index += 1;
    }
    let mut negative = false;
    if index < text.len() && (text[index] == b'+' || text[index] == b'-') {
        negative = text[index] == b'-';
        index += 1;
    }
    let sign = |value: f64| if negative { -value } else { value };
    let rest = &text[index..];
    let lower = |count: usize| -> Vec<u8> { rest.iter().take(count).map(|byte| byte.to_ascii_lowercase()).collect() };

    // Hexadecimal floating constant.
    if rest.len() >= 2 && rest[0] == b'0' && (rest[1] | 0x20) == b'x' {
        let mut cursor = 2;
        let mut mantissa: f64 = 0.0;
        let mut digits = 0;
        let mut exponent: i64 = 0;
        while cursor < rest.len() && (rest[cursor] as char).is_ascii_hexdigit() {
            mantissa = mantissa * 16.0 + (rest[cursor] as char).to_digit(16).unwrap() as f64;
            digits += 1;
            cursor += 1;
        }
        if cursor < rest.len() && rest[cursor] == b'.' {
            cursor += 1;
            while cursor < rest.len() && (rest[cursor] as char).is_ascii_hexdigit() {
                mantissa = mantissa * 16.0 + (rest[cursor] as char).to_digit(16).unwrap() as f64;
                exponent -= 4;
                digits += 1;
                cursor += 1;
            }
        }
        if digits > 0 {
            if cursor < rest.len() && (rest[cursor] | 0x20) == b'p' {
                let mut probe = cursor + 1;
                let mut exponent_negative = false;
                if probe < rest.len() && (rest[probe] == b'+' || rest[probe] == b'-') {
                    exponent_negative = rest[probe] == b'-';
                    probe += 1;
                }
                let exponent_start = probe;
                let mut value: i64 = 0;
                while probe < rest.len() && rest[probe].is_ascii_digit() {
                    value = value.saturating_mul(10).saturating_add((rest[probe] - b'0') as i64);
                    probe += 1;
                }
                if probe > exponent_start {
                    exponent += if exponent_negative { -value } else { value };
                    cursor = probe;
                }
            }
            let value = mantissa * 2f64.powi(exponent.clamp(-2000, 2000) as i32);
            return Some((sign(value), index + cursor));
        }
        // "0x" without digits: strtod reads the "0".
        return Some((sign(0.0), index + 1));
    }

    if lower(8) == b"infinity" {
        return Some((sign(f64::INFINITY), index + 8));
    }
    if lower(3) == b"inf" {
        return Some((sign(f64::INFINITY), index + 3));
    }
    if lower(3) == b"nan" {
        let mut cursor = 3;
        if cursor < rest.len() && rest[cursor] == b'(' {
            let mut probe = cursor + 1;
            while probe < rest.len() && (rest[probe].is_ascii_alphanumeric() || rest[probe] == b'_') {
                probe += 1;
            }
            if probe < rest.len() && rest[probe] == b')' {
                cursor = probe + 1;
            }
        }
        return Some((f64::NAN, index + cursor));
    }

    let mut cursor = 0;
    let mut digits = 0;
    while cursor < rest.len() && rest[cursor].is_ascii_digit() {
        cursor += 1;
        digits += 1;
    }
    if cursor < rest.len() && rest[cursor] == b'.' {
        cursor += 1;
        while cursor < rest.len() && rest[cursor].is_ascii_digit() {
            cursor += 1;
            digits += 1;
        }
    }
    if digits == 0 {
        return None;
    }
    let mantissa_end = cursor;
    if cursor < rest.len() && (rest[cursor] | 0x20) == b'e' {
        let mut probe = cursor + 1;
        if probe < rest.len() && (rest[probe] == b'+' || rest[probe] == b'-') {
            probe += 1;
        }
        let exponent_start = probe;
        while probe < rest.len() && rest[probe].is_ascii_digit() {
            probe += 1;
        }
        if probe > exponent_start {
            cursor = probe;
        }
    }
    let _ = mantissa_end;
    let literal = std::str::from_utf8(&rest[..cursor]).ok()?;
    let literal = if literal.starts_with('.') { format!("0{literal}") } else { literal.to_string() };
    let literal = if literal.ends_with('.') { format!("{literal}0") } else { literal };
    let literal = literal.replace(".e", ".0e").replace(".E", ".0E");
    let value: f64 = literal.parse().ok()?;
    Some((sign(value), index + cursor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tonumber_matches_luau() {
        assert_eq!(tonumber(b"0x10"), Some(16.0));
        assert_eq!(tonumber(b" 12 "), Some(12.0));
        assert_eq!(tonumber(b"1e2"), Some(100.0));
        assert_eq!(tonumber(b"inf"), Some(f64::INFINITY));
        assert!(tonumber(b"nan").unwrap().is_nan());
        assert_eq!(tonumber(b"0b101"), None);
        assert_eq!(tonumber(b"1_000"), None);
        assert_eq!(tonumber(b"12px"), None);
        assert_eq!(tonumber(b".5"), Some(0.5));
        assert_eq!(tonumber(b"5."), Some(5.0));
        assert_eq!(tonumber(b"0x1p4"), Some(16.0));
        assert_eq!(tonumber(b"+5"), Some(5.0));
        assert_eq!(tonumber(b"- 5"), None);
        assert_eq!(tonumber(b""), None);
        assert_eq!(tonumber(b"5.e1"), Some(50.0));
    }

    #[test]
    fn float32_helpers() {
        assert_eq!(f32r(1.3), 1.2999999523162842);
        assert_eq!(lmod(-3.0, 2.0), 1.0);
        assert_eq!(idiv(-3.0, 2.0), -2.0);
        assert!(lmax(f64::NAN, 1.0).is_nan());
        assert_eq!(lmax(1.0, f64::NAN), 1.0);
    }
}
