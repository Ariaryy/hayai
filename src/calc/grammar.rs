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
pub fn parse_amount(input: &str) -> Option<(f64, usize)> {
    let negative = input.starts_with('-');
    let body = &input[usize::from(negative)..];

    let mut end = 0;
    let mut saw_digit = false;
    for (i, c) in body.char_indices() {
        if c.is_ascii_digit() {
            saw_digit = true;
            end = i + c.len_utf8();
        } else if c == '.' {
            end = i + c.len_utf8();
        } else {
            break;
        }
    }
    if !saw_digit {
        return None;
    }
    let mut magnitude: f64 = body[..end].parse().ok()?;
    let mut consumed = end;
    let after = &body[end..];
    if after.starts_with(['k', 'K']) {
        let attaches_to_unit = after[1..].chars().next().is_some_and(|c| c.is_alphabetic());
        if !attaches_to_unit {
            magnitude *= 1000.0;
            consumed += 1;
        }
    }
    let value = if negative { -magnitude } else { magnitude };
    Some((value, consumed + usize::from(negative)))
}

/// Parses the conversion shape and returns `(amount, from, to)` with the
/// unit/currency tokens lowercased. `None` if the input isn't shaped like a
/// conversion at all (caller falls through to plain arithmetic).
pub fn parse_conversion(input: &str) -> Option<(f64, String, String)> {
    let input = input.trim();

    let (amount, consumed) = parse_amount(input)?;
    let rest = input[consumed..].trim_start();

    let from_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    if from_end == 0 {
        return None;
    }
    let from = rest[..from_end].to_lowercase();
    let rest = rest[from_end..].trim_start();

    let (to, matched) = parse_keyword_then_target(rest);
    if !matched {
        return None;
    }
    Some((amount, from, to))
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
    fn parses_basic_conversion() {
        assert_eq!(
            parse_conversion("10 km in miles"),
            Some((10.0, "km".to_string(), "miles".to_string()))
        );
        assert_eq!(
            parse_conversion("100 usd to inr"),
            Some((100.0, "usd".to_string(), "inr".to_string()))
        );
        assert_eq!(
            parse_conversion("5.5kg as lb"),
            Some((5.5, "kg".to_string(), "lb".to_string()))
        );
    }

    #[test]
    fn rejects_non_conversion_shapes() {
        assert_eq!(parse_conversion("2+2"), None);
        assert_eq!(parse_conversion("hello world"), None);
        assert_eq!(parse_conversion("10 ironman"), None);
    }

    #[test]
    fn k_suffix_means_thousands() {
        assert_eq!(
            parse_conversion("5k km to miles"),
            Some((5000.0, "km".to_string(), "miles".to_string()))
        );
        assert_eq!(
            parse_conversion("2.5K usd to inr"),
            Some((2500.0, "usd".to_string(), "inr".to_string()))
        );
    }

    #[test]
    fn k_suffix_does_not_eat_k_prefixed_units() {
        // "kg" is a unit that starts with 'k'; the 'k' must stay part of the
        // unit token, not get consumed as a x1000 multiplier.
        assert_eq!(
            parse_conversion("5.5kg as lb"),
            Some((5.5, "kg".to_string(), "lb".to_string()))
        );
    }
}
