//! Number-base conversion: hex/decimal/octal/binary, e.g. "0x1A to decimal",
//! "26 to hex", "0b1010 as octal". Numbers use `0x`/`0o`/`0b` prefixes
//! (case-insensitive) or plain decimal digits; output is always prefixed and
//! round-trippable.

use super::EvalResult;
use super::grammar;

#[derive(Clone, Copy)]
enum Base {
    Hex,
    Dec,
    Oct,
    Bin,
}

impl Base {
    fn radix(self) -> u32 {
        match self {
            Base::Hex => 16,
            Base::Dec => 10,
            Base::Oct => 8,
            Base::Bin => 2,
        }
    }

    fn prefix(self) -> &'static str {
        match self {
            Base::Hex => "0x",
            Base::Dec => "",
            Base::Oct => "0o",
            Base::Bin => "0b",
        }
    }

    fn label(self) -> &'static str {
        match self {
            Base::Hex => "hex",
            Base::Dec => "decimal",
            Base::Oct => "octal",
            Base::Bin => "binary",
        }
    }

    fn format(self, value: i64) -> String {
        let negative = value < 0;
        let magnitude = value.unsigned_abs();
        let digits = match self {
            Base::Hex => format!("{magnitude:X}"),
            Base::Dec => format!("{magnitude}"),
            Base::Oct => format!("{magnitude:o}"),
            Base::Bin => format!("{magnitude:b}"),
        };
        let body = format!("{}{digits}", self.prefix());
        if negative { format!("-{body}") } else { body }
    }
}

fn find_base_name(token: &str) -> Option<Base> {
    match token {
        "hex" | "hexadecimal" => Some(Base::Hex),
        "dec" | "decimal" => Some(Base::Dec),
        "oct" | "octal" => Some(Base::Oct),
        "bin" | "binary" => Some(Base::Bin),
        _ => None,
    }
}

/// Parses a base-prefixed (`0x`/`0o`/`0b`, case-insensitive) or plain
/// decimal integer token off the front of `input`. Returns the value, the
/// base it was written in, and how many bytes were consumed.
fn parse_number(input: &str) -> Option<(i64, Base, usize)> {
    let negative = input.starts_with('-');
    let body = if negative { &input[1..] } else { input };

    let (base, digits_start) = if body.len() >= 2 && body.as_bytes()[0] == b'0' {
        match body.as_bytes()[1] {
            b'x' | b'X' => (Base::Hex, 2),
            b'o' | b'O' => (Base::Oct, 2),
            b'b' | b'B' => (Base::Bin, 2),
            _ => (Base::Dec, 0),
        }
    } else {
        (Base::Dec, 0)
    };

    let digits_body = &body[digits_start..];
    let end = digits_body
        .find(|c: char| !c.is_digit(base.radix()))
        .unwrap_or(digits_body.len());
    if end == 0 {
        return None;
    }
    let magnitude = i64::from_str_radix(&digits_body[..end], base.radix()).ok()?;
    let value = if negative { -magnitude } else { magnitude };
    let consumed = usize::from(negative) + digits_start + end;
    Some((value, base, consumed))
}

/// Attempts a base conversion, e.g. "0x1A to decimal" or "26 to hex". `None`
/// if the input isn't shaped like a number, or the target isn't a
/// recognized base name — declining leaves arithmetic/units/currency to try
/// next.
pub fn convert(input: &str) -> Option<EvalResult> {
    let trimmed = input.trim();
    let (value, from_base, consumed) = parse_number(trimmed)?;
    let rest = trimmed[consumed..].trim_start();
    let (target, matched) = grammar::parse_keyword_then_target(rest);
    if !matched {
        return None;
    }
    let to_base = find_base_name(&target)?;

    Some(EvalResult {
        expression: format!("{} {}", from_base.format(value), from_base.label()),
        value: to_base.format(value),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_to_decimal() {
        let r = convert("0x1A to decimal").unwrap();
        assert_eq!(r.value, "26");
    }

    #[test]
    fn decimal_to_hex() {
        let r = convert("26 to hex").unwrap();
        assert_eq!(r.value, "0x1A");
    }

    #[test]
    fn decimal_to_binary() {
        let r = convert("10 to binary").unwrap();
        assert_eq!(r.value, "0b1010");
    }

    #[test]
    fn binary_to_octal() {
        let r = convert("0b1010 as octal").unwrap();
        assert_eq!(r.value, "0o12");
    }

    #[test]
    fn declines_unknown_target() {
        assert!(convert("26 to miles").is_none());
    }

    #[test]
    fn declines_non_number() {
        assert!(convert("hello to hex").is_none());
    }
}
