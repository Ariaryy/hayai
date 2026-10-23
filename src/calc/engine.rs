//! Adapter around the general-purpose expression engine.
//!
//! Launcher-specific parsing stays outside this module. This adapter owns
//! calculator syntax, physical units, functions, constants, percentages,
//! comparisons, dates, and number-base conversions behind one small interface.

use super::EvalResult;
use super::currency::FxCache;
use std::borrow::Cow;
use std::time::{Duration, Instant};

const MAX_INPUT_BYTES: usize = 4096;
const MAX_DEPTH: usize = 48;
struct Budget {
    deadline: Instant,
}
impl fend_core::Interrupt for Budget {
    fn should_interrupt(&self) -> bool {
        Instant::now() >= self.deadline
    }
}

type RateTable = std::sync::Arc<std::collections::HashMap<String, f64>>;
#[derive(Clone)]
struct CachedRates(Option<RateTable>);

impl fend_core::ExchangeRateFnV2 for CachedRates {
    fn relative_to_base_currency(
        &self,
        currency: &str,
        _options: &fend_core::ExchangeRateFnV2Options,
    ) -> Result<f64, Box<dyn std::error::Error + Send + Sync + 'static>> {
        self.0
            .as_ref()
            .and_then(|rates| rates.get(&currency.to_ascii_lowercase()).copied())
            .ok_or_else(|| format!("no cached rate for {currency}").into())
    }
}

/// Evaluate a complete calculator expression.
///
/// A fresh context makes each launcher query independent. Context construction
/// does not build a unit registry at runtime; the engine's unit data is static.
pub fn evaluate(input: &str, rates: Option<&FxCache>) -> Option<EvalResult> {
    if input.len() > MAX_INPUT_BYTES {
        return None;
    }
    let budget = Budget {
        deadline: Instant::now() + Duration::from_millis(12),
    };
    let snapshot = rates.and_then(FxCache::snapshot);
    evaluate_inner(input, snapshot.as_ref(), &budget, 0)
}

fn evaluate_inner(
    input: &str,
    rates: Option<&RateTable>,
    budget: &Budget,
    depth: usize,
) -> Option<EvalResult> {
    if depth >= MAX_DEPTH || fend_core::Interrupt::should_interrupt(budget) {
        return None;
    }
    let mut nesting = 0usize;
    for c in input.chars() {
        if c == '(' {
            nesting += 1;
        }
        if nesting >= MAX_DEPTH {
            return None;
        }
        if c == ')' {
            nesting = nesting.checked_sub(1)?;
        }
    }
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return None;
    }

    if let Some(result) = evaluate_aggregate(trimmed, rates, budget, depth + 1) {
        return Some(result);
    }

    if let Some(value) = evaluate_ordering(trimmed, rates, budget, depth + 1) {
        return Some(EvalResult {
            copy_value: None,
            expression: trimmed.to_string(),
            value: value.to_string(),
        });
    }

    let temperature = super::units::temperature_expression(trimmed);
    let normalized = normalize_syntax(temperature.as_deref().unwrap_or(trimmed));
    let mut context = fend_core::Context::new();
    context.disable_rng();
    if let Some(rates) = rates {
        context.set_exchange_rate_handler_v2(CachedRates(Some(rates.clone())));
    }
    let result = fend_core::evaluate_with_interrupt(&normalized, &mut context, budget).ok()?;
    if result.output_is_empty() || result.get_main_result().len() > 8192 {
        return None;
    }

    Some(EvalResult {
        copy_value: None,
        expression: trimmed.to_string(),
        value: normalize_output(result.get_main_result()),
    })
}

fn evaluate_aggregate(
    input: &str,
    rates: Option<&RateTable>,
    budget: &Budget,
    depth: usize,
) -> Option<EvalResult> {
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
            evaluate_inner(
                &args
                    .iter()
                    .map(|arg| format!("({arg})"))
                    .collect::<Vec<_>>()
                    .join(" + "),
                rates,
                budget,
                depth + 1,
            )?
            .value
        }
        "avg" | "mean" | "average" => {
            evaluate_inner(
                &format!(
                    "({}) / {}",
                    args.iter()
                        .map(|arg| format!("({arg})"))
                        .collect::<Vec<_>>()
                        .join(" + "),
                    args.len()
                ),
                rates,
                budget,
                depth + 1,
            )?
            .value
        }
        "min" | "max" => {
            let mut selected = args[0];
            for candidate in &args[1..] {
                let operator = if name == "min" { "<" } else { ">" };
                if evaluate_ordering(
                    &format!("({candidate}) {operator} ({selected})"),
                    rates,
                    budget,
                    depth + 1,
                )? {
                    selected = candidate;
                }
            }
            evaluate_inner(selected, rates, budget, depth + 1)?.value
        }
        _ => unreachable!(),
    };
    Some(EvalResult {
        copy_value: None,
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
    if last.is_empty() {
        return None;
    }
    args.push(last);
    Some(args)
}

fn evaluate_ordering(
    input: &str,
    rates: Option<&RateTable>,
    budget: &Budget,
    depth: usize,
) -> Option<bool> {
    let (index, operator) = find_top_level_ordering(input)?;
    let lhs = input[..index].trim();
    let rhs = input[index + operator.len()..].trim();
    if lhs.is_empty() || rhs.is_empty() {
        return None;
    }
    let difference = evaluate_inner(&format!("({lhs}) - ({rhs})"), rates, budget, depth + 1)?;
    let sign = parse_number_sign(&difference.value)?;
    Some(match operator {
        ">" => sign.is_gt(),
        ">=" => !sign.is_lt(),
        "<" => sign.is_lt(),
        "<=" => !sign.is_gt(),
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

fn parse_number_sign(output: &str) -> Option<std::cmp::Ordering> {
    let token = output.split_whitespace().next()?.replace(',', "");
    let (mantissa, _) = token.split_once(['e', 'E']).unwrap_or((&token, ""));
    let digits = mantissa.trim_start_matches(['+', '-']);
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit() || c == '.') {
        return None;
    }
    Some(if digits.chars().all(|c| matches!(c, '0' | '.')) {
        std::cmp::Ordering::Equal
    } else if mantissa.starts_with('-') {
        std::cmp::Ordering::Less
    } else {
        std::cmp::Ordering::Greater
    })
}

/// Translate familiar calculator words that the engine deliberately leaves
/// to front ends. Replacements only apply to complete ASCII words, so unit
/// names and function identifiers are unaffected.
fn normalize_syntax(input: &str) -> Cow<'_, str> {
    let needs_rewrite = input.contains("**")
        || input.split(|c: char| !c.is_ascii_alphabetic()).any(|word| {
            super::units::canonical_word(word).is_some()
                || matches!(
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
            _ => super::units::canonical_word(word).unwrap_or(word.as_str()),
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
    #[test]
    fn evaluation_budget_is_shared_and_input_is_bounded() {
        let expired = Budget {
            deadline: Instant::now() - Duration::from_secs(1),
        };
        assert!(evaluate_inner("sum(1,2,3)", None, &expired, 0).is_none());
        assert!(evaluate(&"1".repeat(4097), None).is_none());
        assert!(evaluate(&format!("{}1{}", "(".repeat(50), ")".repeat(50)), None).is_none());
        assert!(split_arguments("1,").is_none());
    }
    #[test]
    fn comparisons_do_not_narrow_to_floating_point() {
        assert_eq!(
            parse_number_sign("1e-999 m"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            parse_number_sign("-1e999 m"),
            Some(std::cmp::Ordering::Less)
        );
        assert_eq!(
            parse_number_sign("0.000 m"),
            Some(std::cmp::Ordering::Equal)
        );
    }
}
