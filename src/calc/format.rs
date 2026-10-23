//! Decimal presentation never casts amounts to fixed-width integer cents.
//! Clipboard values come directly from the expression engine instead.

pub fn currency(value: &str, places: usize) -> Option<String> {
    let value = value.trim();
    let negative = value.starts_with('-');
    let value = value.trim_start_matches(['+', '-']);
    let (mantissa, exponent) = value.split_once(['e', 'E']).unwrap_or((value, "0"));
    let exponent: i32 = exponent.parse().ok()?;
    if !(-4096..=4096).contains(&exponent) {
        return None;
    }
    let (whole, fraction) = mantissa.split_once('.').unwrap_or((mantissa, ""));
    let mut digits: Vec<u8> = whole.bytes().chain(fraction.bytes()).collect();
    if digits.is_empty() || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    let point = whole.len() as i32 + exponent;
    let point = if point < 0 {
        let mut padded = vec![b'0'; (-point) as usize];
        padded.extend(digits);
        digits = padded;
        0
    } else {
        point as usize
    };
    if digits.len() < point {
        digits.resize(point, b'0');
    }
    // Nonzero sub-minor-unit amounts retain the engine's precision rather
    // than being presented as zero, irrespective of the currency's scale.
    let small = digits[..point.min(digits.len())].iter().all(|d| *d == b'0')
        && digits
            .get(point..point + places)
            .unwrap_or(&[])
            .iter()
            .all(|d| *d == b'0')
        && digits.iter().any(|d| *d != b'0');
    let keep = if small {
        digits.len().saturating_sub(point).max(places)
    } else {
        places
    };
    let round_at = point + keep;
    let round_up = digits.get(round_at).is_some_and(|d| *d >= b'5');
    digits.resize(round_at, b'0');
    let mut point = point;
    if round_up {
        let mut carry = true;
        for digit in digits.iter_mut().rev() {
            if *digit == b'9' {
                *digit = b'0';
            } else {
                *digit += 1;
                carry = false;
                break;
            }
        }
        if carry {
            digits.insert(0, b'1');
            point += 1;
        }
    }
    let whole = std::str::from_utf8(&digits[..point])
        .ok()?
        .trim_start_matches('0');
    let whole = if whole.is_empty() { "0" } else { whole };
    let mut out = String::new();
    if negative && digits.iter().any(|d| *d != b'0') {
        out.push('-');
    }
    for (index, digit) in whole.chars().enumerate() {
        if index != 0 && (whole.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(digit);
    }
    if keep != 0 {
        out.push('.');
        out.push_str(std::str::from_utf8(&digits[point..]).ok()?);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn decimal_formatting_is_not_integer_limited() {
        for (input, places, expected) in [
            ("100000000000000001", 2, "100,000,000,000,000,001.00"),
            ("1234.505", 2, "1,234.51"),
            ("-0.000123", 2, "-0.000123"),
            ("9.999", 2, "10.00"),
            ("1e-8", 2, "0.00000001"),
            ("1e17", 2, "100,000,000,000,000,000.00"),
            ("150.3", 0, "150"),
            ("1.2345", 3, "1.235"),
        ] {
            assert_eq!(
                currency(input, places).as_deref(),
                Some(expected),
                "{input}"
            );
        }
        assert!(currency("NaN", 2).is_none());
    }
}
