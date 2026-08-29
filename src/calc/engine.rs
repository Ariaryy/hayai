//! Adapter around the general-purpose expression engine.
//!
//! Launcher-specific parsing stays outside this module. This adapter owns
//! calculator syntax, physical units, functions, constants, percentages,
//! comparisons, dates, and number-base conversions behind one small interface.

use super::EvalResult;
use super::currency::FxCache;
use std::borrow::Cow;

#[derive(Clone)]
struct CachedRates(FxCache);

impl fend_core::ExchangeRateFnV2 for CachedRates {
    fn relative_to_base_currency(
        &self,
        currency: &str,
        _options: &fend_core::ExchangeRateFnV2Options,
    ) -> Result<f64, Box<dyn std::error::Error + Send + Sync + 'static>> {
        self.0
            .rate_for(currency)
            .ok_or_else(|| format!("no cached rate for {currency}").into())
    }
}

/// Evaluate a complete calculator expression.
///
/// A fresh context makes each launcher query independent. Context construction
/// does not build a unit registry at runtime; the engine's unit data is static.
pub fn evaluate(input: &str, rates: Option<&FxCache>) -> Option<EvalResult> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(result) = evaluate_aggregate(trimmed, rates) {
        return Some(result);
    }

    if let Some(value) = evaluate_ordering(trimmed, rates) {
        return Some(EvalResult {
            expression: trimmed.to_string(),
            value: value.to_string(),
        });
    }

    let normalized = normalize_syntax(trimmed);
    let mut context = fend_core::Context::new();
    context.disable_rng();
    if let Some(rates) = rates {
        context.set_exchange_rate_handler_v2(CachedRates(rates.clone()));
    }
    let result = fend_core::evaluate(&normalized, &mut context).ok()?;
    if result.output_is_empty() {
        return None;
    }

    Some(EvalResult {
        expression: trimmed.to_string(),
        value: normalize_output(result.get_main_result()),
    })
}

fn evaluate_aggregate(input: &str, rates: Option<&FxCache>) -> Option<EvalResult> {
    let open = input.find('(')?;
    if !input.ends_with(')') {
        return None;
    }
    let name = input[..open].trim().to_ascii_lowercase();
    if !matches!(
        name.as_str(),
        "min" | "max" | "sum" | "avg" | "mean" | "average"
    ) {
        return None;
    }
    let args = split_arguments(&input[open + 1..input.len() - 1])?;
    if args.is_empty() {
        return None;
    }

    let value = match name.as_str() {
        "sum" => {
            evaluate(
                &args
                    .iter()
                    .map(|arg| format!("({arg})"))
                    .collect::<Vec<_>>()
                    .join(" + "),
                rates,
            )?
            .value
        }
        "avg" | "mean" | "average" => {
            evaluate(
                &format!(
                    "({}) / {}",
                    args.iter()
                        .map(|arg| format!("({arg})"))
                        .collect::<Vec<_>>()
                        .join(" + "),
                    args.len()
                ),
                rates,
            )?
            .value
        }
        "min" | "max" => {
            let mut selected = args[0];
            for candidate in &args[1..] {
                let operator = if name == "min" { "<" } else { ">" };
                if evaluate_ordering(&format!("({candidate}) {operator} ({selected})"), rates)? {
                    selected = candidate;
                }
            }
            evaluate(selected, rates)?.value
        }
        _ => unreachable!(),
    };
    Some(EvalResult {
        expression: input.to_string(),
        value,
    })
}

fn split_arguments(input: &str) -> Option<Vec<&str>> {
    let mut args = Vec::new();
    let mut depth = 0_u32;
    let mut start = 0;
    for (index, ch) in input.char_indices() {
        match ch {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1)?,
            ',' if depth == 0 => {
                let arg = input[start..index].trim();
                if arg.is_empty() {
                    return None;
                }
                args.push(arg);
                start = index + 1;
            }
            _ => {}
        }
    }
    if depth != 0 {
        return None;
    }
    let last = input[start..].trim();
    if !last.is_empty() {
        args.push(last);
    }
    Some(args)
}

fn evaluate_ordering(input: &str, rates: Option<&FxCache>) -> Option<bool> {
    let (index, operator) = find_top_level_ordering(input)?;
    let lhs = input[..index].trim();
    let rhs = input[index + operator.len()..].trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    let difference = evaluate(&format!("({lhs}) - ({rhs})"), rates)?;
    let number = parse_leading_number(&difference.value)?;
    Some(match operator {
        ">" => number > 0.0,
        ">=" => number >= 0.0,
        "<" => number < 0.0,
        "<=" => number <= 0.0,
        _ => return None,
    })
}

fn find_top_level_ordering(input: &str) -> Option<(usize, &'static str)> {
    let bytes = input.as_bytes();
    let mut depth = 0_u32;
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'(' => depth += 1,
            b')' => depth = depth.saturating_sub(1),
            b'>' | b'<' if depth == 0 => {
                let operator = match (bytes[index], bytes.get(index + 1)) {
                    (b'>', Some(b'=')) => ">=",
                    (b'<', Some(b'=')) => "<=",
                    (b'>', _) => ">",
                    (b'<', _) => "<",
                    _ => unreachable!(),
                };
                // Shift operators belong to arithmetic.
                if bytes.get(index + 1) != Some(&bytes[index]) {
                    return Some((index, operator));
                }
                index += 1;
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn parse_leading_number(output: &str) -> Option<f64> {
    let token = output
        .trim_start()
        .split_once(char::is_whitespace)
        .map_or(output.trim(), |(first, _)| first)
        .replace(',', "");
    token.parse().ok()
}

/// Translate familiar calculator words that the engine deliberately leaves
/// to front ends. Replacements only apply to complete ASCII words, so unit
/// names and function identifiers are unaffected.
fn normalize_syntax(input: &str) -> Cow<'_, str> {
    let needs_rewrite = input.contains("**")
        || input.split(|c: char| !c.is_ascii_alphabetic()).any(|word| {
            matches!(
                word.to_ascii_lowercase().as_str(),
                "add" | "plus" | "minus" | "mul" | "div" | "mod" | "modulo" | "pow" | "power"
            )
        });
    if !needs_rewrite {
        return Cow::Borrowed(input);
    }

    let mut output = String::with_capacity(input.len());
    let mut word = String::new();
    let flush_word = |word: &mut String, output: &mut String| {
        if word.is_empty() {
            return;
        }
        let replacement = match word.to_ascii_lowercase().as_str() {
            "add" | "plus" => "+",
            "minus" => "-",
            "mul" => "*",
            "div" => "/",
            "mod" | "modulo" => "%",
            "pow" | "power" => "^",
            _ => word.as_str(),
        };
        output.push_str(replacement);
        word.clear();
    };

    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch.is_ascii_alphabetic() {
            word.push(ch);
            continue;
        }
        flush_word(&mut word, &mut output);
        if ch == '*' && chars.peek() == Some(&'*') {
            chars.next();
            output.push('^');
        } else {
            output.push(ch);
        }
    }
    flush_word(&mut word, &mut output);
    Cow::Owned(output)
}

fn normalize_output(output: &str) -> String {
    output
        .strip_prefix("approximately ")
        .or_else(|| output.strip_prefix("approx. "))
        .unwrap_or(output)
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evaluates_arithmetic_and_functions() {
        assert_eq!(evaluate("sqrt(16)", None).unwrap().value, "4");
        assert_eq!(evaluate("20% of 100", None).unwrap().value, "20");
        assert_eq!(evaluate("2^8", None).unwrap().value, "256");
    }

    #[test]
    fn evaluates_units_and_compound_dimensions() {
        assert_eq!(evaluate("1 km to m", None).unwrap().value, "1000 m");
        assert_eq!(
            evaluate("5 kg * 9.8 m/s^2 to N", None).unwrap().value,
            "49 N"
        );
    }

    #[test]
    fn rejects_non_expressions() {
        assert!(evaluate("spotify", None).is_none());
        assert!(evaluate("1password", None).is_none());
    }

    #[test]
    fn accepts_calculator_language() {
        for input in [
            "6 mod 2",
            "5 mul 5",
            "5 div 5",
            "2 pow 8",
            "2 plus 3",
            "10 minus 4",
            "2(1 + 5)",
            "1 << 8",
            "42 >= 21",
            "5!",
            "min(1, 2, 3)",
            "sin(pi / 2)",
            "1_000_000 + 1_000",
        ] {
            assert!(evaluate(input, None).is_some(), "did not evaluate {input}");
        }
    }

    #[test]
    fn accepts_broad_unit_and_date_operations() {
        for input in [
            "1 km + 100 m",
            "1 km * 1 km",
            "1 km / 100 m",
            "2 cups to ml",
            "1 acre to m^2",
            "100 mph to kmh",
            "1 GB to MB",
            "1 GiB to MiB",
            "1 kWh to J",
            "2 bar to psi",
            "@2020-05-04 + 5 days",
        ] {
            assert!(evaluate(input, None).is_some(), "did not evaluate {input}");
        }
    }
}
