//! Shared number formatting for calc result rows.

/// Integers print with thousands separators (`"4,180"`); everything else
/// prints to 6 decimal places with trailing zeros trimmed, grouped the same
/// way (`"1,234.5"`).
pub fn format_number(value: f64) -> String {
    if !value.is_finite() {
        return "Error".to_string();
    }
    let negative = value < 0.0;
    let magnitude = value.abs();

    let body = if magnitude.fract() == 0.0 && magnitude < 1e15 {
        group_thousands(&format!("{}", magnitude as i64))
    } else {
        let mut text = format!("{:.6}", magnitude);
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.pop();
        }
        match text.split_once('.') {
            Some((int_part, frac_part)) => format!("{}.{}", group_thousands(int_part), frac_part),
            None => group_thousands(&text),
        }
    };

    if negative { format!("-{body}") } else { body }
}

/// Currency amounts always show exactly two decimal places (`"8,300.00"`),
/// unlike `format_number`'s variable-precision, trailing-zero-trimmed style.
pub fn format_currency(value: f64) -> String {
    if !value.is_finite() {
        return "Error".to_string();
    }
    let negative = value < 0.0;
    let cents = (value.abs() * 100.0).round() as i64;
    let body = format!(
        "{}.{:02}",
        group_thousands(&(cents / 100).to_string()),
        cents % 100
    );
    if negative { format!("-{body}") } else { body }
}

fn group_thousands(digits: &str) -> String {
    let bytes = digits.as_bytes();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, b) in bytes.iter().enumerate() {
        if i != 0 && (bytes.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(*b as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn integers_print_bare() {
        assert_eq!(format_number(4.0), "4");
        assert_eq!(format_number(-3.0), "-3");
    }

    #[test]
    fn fractions_trim_trailing_zeros() {
        assert_eq!(format_number(2.5), "2.5");
        assert_eq!(format_number(1.0 / 3.0), "0.333333");
    }

    #[test]
    fn groups_thousands() {
        assert_eq!(format_number(4180.0), "4,180");
        assert_eq!(format_number(1234567.0), "1,234,567");
        assert_eq!(format_number(-8300.0), "-8,300");
        assert_eq!(format_number(1234.5), "1,234.5");
    }

    #[test]
    fn currency_always_shows_two_decimals() {
        assert_eq!(format_currency(8300.0), "8,300.00");
        assert_eq!(format_currency(90.0), "90.00");
        assert_eq!(format_currency(1234.5), "1,234.50");
        assert_eq!(format_currency(83.12345), "83.12");
        assert_eq!(format_currency(-8300.0), "-8,300.00");
    }
}
