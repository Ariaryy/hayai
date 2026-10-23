//! Shared `<amount> <from-unit> (in|to|as) <to-unit>` grammar used by both
//! unit conversion and currency conversion (e.g. "10 km in miles",
//! "100 usd to inr").

/// Parses a leading numeric amount, treating a trailing `k`/`K` as a
/// thousands shorthand (`"5k"` = 5000) — the reason this lives in the
/// conversion grammar rather than the plain arithmetic evaluator, per the
/// user's explicit request that `k` only apply to conversions. A `k` is
/// *not* consumed as a multiplier when a unit token is glued directly onto
/// the number and starts with `k` (e.g. `"5.5kg"` stays `5.5 kg`, not
/// `5500 g`) — only when it's followed by whitespace/end-of-input.
/// Returns the shorthand-expanded value and how many bytes were consumed.
/// A normalized decimal token. Keep its text for the exact expression engine;
/// floating point is only exposed to adapters that need bounded OS values.
pub fn amount_token(input: &str) -> Option<(String, usize)> {
    let bytes = input.as_bytes();
    let mut end = usize::from(bytes.first().is_some_and(|c| matches!(c, b'+' | b'-')));
    let start = end;
    while bytes
        .get(end)
        .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'.' | b',' | b'_'))
    {
        end += 1;
    }
    if end == start {
        return None;
    }
    let mantissa = &input[start..end];
    if mantissa.bytes().filter(|c| *c == b'.').count() > 1
        || !mantissa.bytes().any(|c| c.is_ascii_digit())
    {
        return None;
    }
    for (index, byte) in mantissa.bytes().enumerate() {
        if byte == b'_'
            && (index == 0
                || !mantissa.as_bytes()[index - 1].is_ascii_digit()
                || !mantissa
                    .as_bytes()
                    .get(index + 1)
                    .is_some_and(u8::is_ascii_digit))
        {
            return None;
        }
    }
    if mantissa.contains(',') && mantissa.contains('_') {
        return None;
    }
    if mantissa.contains(',') {
        let integer = mantissa.split('.').next()?;
        let groups: Vec<_> = integer.split(',').collect();
        if groups[0].is_empty()
            || groups[0].len() > 3
            || groups
                .iter()
                .skip(1)
                .any(|g| g.len() != 3 || !g.bytes().all(|c| c.is_ascii_digit()))
            || mantissa
                .split_once('.')
                .is_some_and(|(_, f)| f.contains(','))
        {
            return None;
        }
    }
    if bytes.get(end).is_some_and(|c| matches!(c, b'e' | b'E')) {
        let exponent = end;
        let mut next = end + 1;
        if bytes.get(next).is_some_and(|c| matches!(c, b'+' | b'-')) {
            next += 1;
        }
        let digits = next;
        while bytes.get(next).is_some_and(u8::is_ascii_digit) {
            next += 1;
        }
        if next > digits {
            end = next;
        } else {
            end = exponent;
        }
    }
    let mut text = input[..end].replace([',', '_'], "");
    if bytes.get(end).is_some_and(|c| matches!(c, b'k' | b'K'))
        && bytes.get(end + 1).is_none_or(u8::is_ascii_whitespace)
    {
        text = format!("({text} * 1000)");
        end += 1;
    }
    Some((text, end))
}

pub fn parse_amount(input: &str) -> Option<(f64, usize)> {
    let (text, consumed) = amount_token(input)?;
    let value = if let Some(body) = text
        .strip_prefix('(')
        .and_then(|s| s.strip_suffix(" * 1000)"))
    {
        body.parse::<f64>().ok()? * 1000.0
    } else {
        text.parse().ok()?
    };
    value.is_finite().then_some((value, consumed))
}

/// Shared, case-preserving conversion grammar. Currency names and natural
/// language can fold case later; physical symbols (Mb/MB, m/M) must not.
pub fn conversion_tokens(input: &str) -> Option<(String, String, String)> {
    let input = input.trim();
    let (amount, consumed) = amount_token(input)?;
    let rest = input[consumed..].trim_start();
    let (from, to) = split_conversion(rest)?;
    Some((amount, from.to_string(), to.to_string()))
}

/// Find a complete conversion keyword, including multiword source/target names.
pub fn split_conversion(input: &str) -> Option<(&str, &str)> {
    for (index, _) in input.char_indices() {
        if index == 0 || !input[..index].ends_with(char::is_whitespace) {
            continue;
        }
        let rest = &input[index..];
        for keyword in ["to", "in", "as"] {
            if rest
                .get(..keyword.len())
                .is_some_and(|s| s.eq_ignore_ascii_case(keyword))
                && rest[keyword.len()..].starts_with(char::is_whitespace)
            {
                let from = input[..index].trim();
                let to = rest[keyword.len()..].trim();
                if !from.is_empty() && !to.is_empty() {
                    return Some((from, to));
                }
            }
        }
    }
    None
}

/// Parses "(in|to|as) <target>" off the front of `rest`, requiring the
/// keyword to be its own whitespace-delimited token (so "iron" doesn't get
/// misread as keyword "in" + "ron").
pub fn parse_keyword_then_target(rest: &str) -> (String, bool) {
    for keyword in ["to", "in", "as"] {
        if let Some(tail) = rest.get(..keyword.len())
            && tail.eq_ignore_ascii_case(keyword)
            && rest[keyword.len()..]
                .chars()
                .next()
                .is_some_and(|c| c.is_whitespace())
        {
            let target = rest[keyword.len()..].trim().to_lowercase();
            if target.is_empty() {
                return (String::new(), false);
            }
            return (target, true);
        }
    }
    (String::new(), false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn conversion_tokens_preserve_symbols_and_exact_amounts() {
        for (query, amount, from, to) in [
            ("10 km in miles", "10", "km", "miles"),
            ("100 usd to inr", "100", "usd", "inr"),
            ("5.5kg as lb", "5.5", "kg", "lb"),
            ("5k km to miles", "(5 * 1000)", "km", "miles"),
            ("2.5K usd to inr", "(2.5 * 1000)", "usd", "inr"),
            ("1,000Mb to MB", "1000", "Mb", "MB"),
            ("1e3 MB to Mb", "1e3", "MB", "Mb"),
            (
                "100000000000000001 USD to INR",
                "100000000000000001",
                "USD",
                "INR",
            ),
        ] {
            assert_eq!(
                conversion_tokens(query),
                Some((amount.into(), from.into(), to.into())),
                "{query}"
            );
        }
        for query in [
            "hello world",
            "2+2",
            "1,00 m to km",
            "10 ironman",
            "NaN USD to INR",
            "1__0 USD to INR",
        ] {
            assert!(conversion_tokens(query).is_none(), "{query}");
        }
    }
}
