//! NL-detected calculator: arithmetic, unit conversion, currency
//! conversion. Claims matching queries via `auto_claim` (tried before
//! keyword routing — see `plugins::PluginRegistry::dispatch`), and stays
//! reachable explicitly via the ` = ` keyword.
//!
//! Arithmetic and general unit expressions are evaluated synchronously by
//! `fend-core` and always answered inline, never debounced. Currency needs a rate table
//! fetched over the network; `CalcProvider` opts into the pipeline's
//! existing debounce/background-job machinery (built for file search) but
//! only actually uses it when a fetch is genuinely needed, so arithmetic
//! and units stay instant.

mod bases;
mod currency;
mod engine;
mod format;
mod grammar;
mod time;
mod units;

use crate::commands::{
    BackgroundSearch, CalculationDetail, CommandAction, CommandItem, CommandProvider, IconSource,
};

#[derive(Clone)]
pub struct EvalResult {
    pub expression: String,
    /// The answer shown large/bold. Clipboard precision is independent.
    pub value: String,
    /// Optional unrounded value for clipboard use; presentation never controls precision.
    pub copy_value: Option<String>,
}

pub struct CalcProvider {
    fx: currency::FxCache,
    memo: std::cell::RefCell<Option<Memo>>,
}

struct Memo {
    query: String,
    revision: u64,
    at: std::time::Instant,
    result: Option<EvalResult>,
}

impl CalcProvider {
    pub fn new() -> Self {
        Self {
            fx: currency::FxCache::load(),
            memo: std::cell::RefCell::new(None),
        }
    }

    // Detection and search cross the same evaluation seam. Retain only the
    // current query, invalidating on a rate refresh or a wall-clock tick.
    fn evaluated(&self, query: &str) -> Option<EvalResult> {
        let revision = self.fx.revision();
        let mut memo = self.memo.borrow_mut();
        if let Some(memo) = memo.as_ref()
            && memo.query == query
            && memo.revision == revision
            && memo.at.elapsed() < std::time::Duration::from_secs(1)
        {
            return memo.result.clone();
        }
        let result = evaluate(query, &self.fx);
        if query.len() <= 4096 {
            *memo = Some(Memo {
                query: query.into(),
                revision,
                at: std::time::Instant::now(),
                result: result.clone(),
            });
        }
        result
    }
}

impl Default for CalcProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Classify intent once. A recognized but invalid request is terminal: it
/// cannot silently acquire a different meaning from a later evaluator.
enum QueryPlan {
    Currency,
    Time,
    Numeric,
}
impl QueryPlan {
    fn parse(input: &str) -> Self {
        if currency::contains_currency(input) {
            Self::Currency
        } else if time::is_time_query(input) && !units::is_clock_collision(input) {
            Self::Time
        } else {
            Self::Numeric
        }
    }
}

fn evaluate(input: &str, fx: &currency::FxCache) -> Option<EvalResult> {
    if input.len() > 4096 {
        return None;
    }
    match QueryPlan::parse(input) {
        QueryPlan::Currency => currency::convert(input, fx),
        QueryPlan::Time => time::convert(input),
        QueryPlan::Numeric => units::convert(input)
            .or_else(|| bases::convert(input))
            .or_else(|| time::convert(input)) // zone pairs without a clock
            .or_else(|| engine::evaluate(input, Some(fx)))
            .or_else(|| evaluate_incomplete_arithmetic(input, fx)),
    }
}

/// Keeps the previous valid arithmetic result visible while the user is in
/// the middle of typing the next operation ("12 + 12 +"). Completed input
/// still goes through the strict evaluators above; this fallback only trims
/// a trailing run of operator characters and preserves the typed expression
/// for display.
fn evaluate_incomplete_arithmetic(input: &str, fx: &currency::FxCache) -> Option<EvalResult> {
    let trimmed = input.trim_end();
    let prefix = trimmed.trim_end_matches(['+', '-', '*', '/', '%', '^', '=', '<', '>', '&', '|']);
    if prefix.len() == trimmed.len() || prefix.trim().is_empty() {
        return None;
    }
    let mut result = engine::evaluate(prefix.trim_end(), Some(fx))?;
    result.expression = trimmed.to_string();
    Some(result)
}

/// Two-stage false-positive gate: a cheap first-character check declines
/// non-candidates instantly (this runs on every keystroke across every
/// provider), then each evaluator's own full parse decides for real.
fn is_candidate(input: &str) -> bool {
    if input.len() > 4096 {
        return false;
    }
    let lower = input.to_ascii_lowercase();
    let constant_expression = ["pi", "e", "tau"].iter().any(|constant| {
        lower.strip_prefix(constant).is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .chars()
                    .any(|c| matches!(c, '+' | '-' | '*' | '/' | '%' | '^'))
        })
    });
    input.chars().next().is_some_and(|c| {
        c.is_ascii_digit()
            || c == '('
            || c == '.'
            || c == '-'
            || c == '@'
            || currency::is_symbol(c)
            || matches!(lower.as_str(), "today" | "tomorrow" | "yesterday")
            || constant_expression
            || lower.starts_with("in ")
            || lower.starts_with("now ")
            || (c.is_ascii_alphabetic() && grammar::split_conversion(input).is_some())
            || lower
                .split_once('(')
                .is_some_and(|(name, _)| name.chars().all(|c| c.is_ascii_alphabetic()))
    })
}

fn math_intent(input: &str) -> bool {
    if !input.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let operators = ['+', '-', '*', '/', '%', '^', '=', '<', '>', '&', '|'];
    (input.chars().all(|c| {
        c.is_ascii_digit()
            || c.is_whitespace()
            || operators.contains(&c)
            || matches!(c, '.' | '(' | ')')
    }) && input.chars().any(|c| operators.contains(&c)))
        || [" + ", " - ", " * ", " / ", " ^ "]
            .iter()
            .any(|op| input.contains(op))
        || input.split_once('(').is_some_and(|(name, _)| {
            matches!(
                name,
                "sqrt"
                    | "sin"
                    | "cos"
                    | "tan"
                    | "log"
                    | "ln"
                    | "sum"
                    | "min"
                    | "max"
                    | "avg"
                    | "mean"
                    | "average"
            )
        })
}

fn result_item(query: &str, result: EvalResult, fx: &currency::FxCache) -> CommandItem {
    let calculation_detail = currency::conversion_detail(query, fx)
        .map(
            |(source_label, target_label, updated_label)| CalculationDetail::Currency {
                source_label,
                target_label,
                updated_label,
            },
        )
        .or_else(|| {
            time::timezone_labels(query).map(|(source_label, target_label)| {
                CalculationDetail::Timezones {
                    source_label,
                    target_label,
                }
            })
        })
        .or_else(|| {
            units::conversion_detail(query, &result).map(|(source_label, target_label)| {
                CalculationDetail::Units {
                    source_label,
                    target_label,
                }
            })
        });
    CommandItem {
        file_size: None,
        id: format!("calc:{query}"),
        title: result.value.clone(),
        subtitle: Some(result.expression),
        calculation_detail,
        icon: IconSource::None,
        action: CommandAction::CopyToClipboard(result.copy_value.unwrap_or(result.value)),
    }
}

fn error_item(message: &str) -> CommandItem {
    CommandItem {
        id: "calc:error".into(),
        title: message.into(),
        subtitle: None,
        calculation_detail: None,
        file_size: None,
        icon: IconSource::None,
        action: CommandAction::EmptyState,
    }
}

impl CommandProvider for CalcProvider {
    fn namespace(&self) -> &'static str {
        "calc"
    }

    fn keyword(&self) -> Option<&'static str> {
        Some("=")
    }

    fn wants_debounce(&self) -> bool {
        true
    }

    /// Claim successful calculations or clear calculator intent. Invalid
    /// requests with that intent get a dedicated empty view; ordinary app
    /// names still fall through to the app provider.
    fn auto_claim(&self, query: &str) -> bool {
        let trimmed = query.trim();
        if trimmed.is_empty() || !is_candidate(trimmed) {
            return false;
        }
        self.evaluated(trimmed).is_some()
            || currency::recognizes_any(trimmed, &self.fx)
            || time::is_time_query(trimmed)
            || grammar::conversion_tokens(trimmed).is_some()
            || math_intent(trimmed)
    }

    fn search(&self, query: &str) -> Vec<CommandItem> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        match self.evaluated(trimmed) {
            Some(result) => vec![result_item(trimmed, result, &self.fx)],
            None => vec![error_item(if time::is_time_query(trimmed) {
                "Invalid date or time. Try another expression."
            } else {
                "Invalid expression. Try another calculation."
            })],
        }
    }

    fn background_search(&self, query: &str) -> Option<BackgroundSearch> {
        let trimmed = query.trim();
        if trimmed.len() > 4096 || !currency::needs_fetch(trimmed, &self.fx) {
            return None;
        }
        let fx = self.fx.clone();
        let query = trimmed.to_string();
        Some(Box::new(move || {
            currency::fetch_blocking(&fx);
            match evaluate(&query, &fx) {
                Some(result) => vec![result_item(&query, result, &fx)],
                None => vec![error_item(
                    "Couldn't convert this amount. Check your expression or connection.",
                )],
            }
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_claim_arithmetic() {
        let provider = CalcProvider::new();
        assert!(provider.auto_claim("2+2*3"));
        assert!(provider.auto_claim("7"));
        assert!(!provider.auto_claim("1password"));
        assert!(!provider.auto_claim("e"));
        assert!(!provider.auto_claim("pi"));
        assert!(!provider.auto_claim("tau"));
        assert!(provider.auto_claim("e^2"));
        assert!(provider.auto_claim("pi * 2"));
    }

    #[test]
    fn auto_claim_units() {
        let provider = CalcProvider::new();
        assert!(provider.auto_claim("10 km in miles"));
        assert!(provider.auto_claim("1pm to m"));
        let picometers = provider.search("1pm to m");
        assert_eq!(picometers[0].subtitle.as_deref(), Some("1 pm"));
        assert!(matches!(
            picometers[0].calculation_detail,
            Some(CalculationDetail::Units { .. })
        ));
        let light = provider.search("1 c");
        assert!(
            matches!(
                light[0].calculation_detail,
                Some(CalculationDetail::Units { .. })
            ),
            "unexpected light-speed result: {light:?}"
        );
        assert!(provider.auto_claim("10 km in kg"));
    }

    #[test]
    fn auto_claim_currency_codes_regardless_of_live_rates() {
        let provider = CalcProvider::new();
        assert!(provider.auto_claim("100 usd to inr"));
    }

    #[test]
    fn auto_claim_bases() {
        let provider = CalcProvider::new();
        assert!(provider.auto_claim("0x1A to decimal"));
        assert!(provider.auto_claim("26 to hex"));
        assert!(!provider.auto_claim("26 to miles")); // "miles" isn't a recognized base name
    }

    #[test]
    fn auto_claim_relative_time() {
        let provider = CalcProvider::new();
        assert!(provider.auto_claim("15 mins from now"));
        assert!(provider.auto_claim("in 2 hours 30 minutes"));
        assert_eq!(provider.search("4pm + 30 min")[0].title, "4:30 PM");
    }

    #[test]
    fn auto_claim_timezone_difference() {
        let provider = CalcProvider::new();
        assert!(provider.auto_claim("IST to CST"));
        assert!(provider.auto_claim("12pm UTC"));
        let result = provider.search("IST to CST");
        assert_eq!(result[0].title, "11 hr 30 min behind");
    }

    #[test]
    fn search_returns_base_conversion() {
        let provider = CalcProvider::new();
        let items = provider.search("26 to hex");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "0x1A");
    }

    #[test]
    fn search_returns_computed_row() {
        let provider = CalcProvider::new();
        let items = provider.search("2+2");
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "4");
    }

    #[test]
    fn keeps_last_result_during_trailing_operator_input() {
        let provider = CalcProvider::new();
        for query in ["12 + 12 +", "12 + 12 -", "12 + 12 *", "12 + 12 /"] {
            assert!(
                provider.auto_claim(query),
                "calculator did not claim {query}"
            );
            let items = provider.search(query);
            assert_eq!(items.len(), 1, "missing preview for {query}");
            assert_eq!(items[0].title, "24");
            assert_eq!(items[0].subtitle.as_deref(), Some(query));
        }
        assert!(!provider.auto_claim("+"));
    }

    #[test]
    fn search_accepts_bare_number() {
        let provider = CalcProvider::new();
        assert_eq!(provider.search("7")[0].title, "7");
    }
    fn isolated_provider() -> CalcProvider {
        CalcProvider {
            fx: currency::FxCache::empty(),
            memo: std::cell::RefCell::new(None),
        }
    }

    #[test]
    fn spacing_contract_across_domains() {
        let provider = isolated_provider();
        for (joined, spaced) in [
            ("1C to F", "1 C to F"),
            ("(1c) to f", "1 c to f"),
            ("(1 gb) to mb", "1 gb to mb"),
            ("1kg to lb", "1 kg to lb"),
            ("1Mb to MB", "1 Mb to MB"),
            ("1pm+30min", "1 pm + 30 min"),
            ("1 pm+30min", "1pm +30 min"),
            ("1C", "1 C"),
            ("1c", "1 c"),
            ("4pm+2h30min", "4 pm + 2 hours 30 minutes"),
        ] {
            assert!(provider.auto_claim(joined), "{joined}");
            assert!(provider.auto_claim(spaced), "{spaced}");
            assert_eq!(
                provider.search(joined)[0].title,
                provider.search(spaced)[0].title,
                "{joined}"
            );
        }
        assert_eq!(provider.search("1Mb to MB")[0].title, "0.125 MB");
        assert_eq!(provider.search("1MB to Mb")[0].title, "8 Mb");
    }

    #[test]
    fn invalid_time_requests_cannot_fall_through() {
        let provider = isolated_provider();
        for query in [
            "2 / 0",
            "10 km to kg",
            "2 + unknown",
            "4pm + inf",
            "4pm + NaN",
            "in 1e300 years",
            "31 February 2026 at 12pm utc to gmt",
        ] {
            assert!(provider.auto_claim(query), "{query}");
            assert!(
                matches!(provider.search(query)[0].action, CommandAction::EmptyState),
                "{query}"
            );
        }
    }

    #[test]
    fn cancelled_background_jobs_own_no_refresh_state() {
        let provider = isolated_provider();
        let abandoned = provider.background_search("100 USD to INR").unwrap();
        drop(abandoned); // mirrors the launcher's generation check before debounce ends
        assert!(provider.background_search("200 USD to INR").is_some());
        assert!(
            provider
                .background_search(&format!("{} USD to INR", "1".repeat(4097)))
                .is_none()
        );
        assert!(
            provider
                .background_search("100 USD + 50 EUR to INR")
                .is_some()
        );
    }

    #[test]
    fn broad_expressions_pass_the_provider_seam() {
        let provider = isolated_provider();
        for query in [
            "sum(1, 2, 3)",
            "avg(1, 2, 3)",
            "1 acre to m^2",
            "1 cup to ml",
            "1cup",
            "5 kg * 9.8 m/s^2 to N",
            "1 kWh to J",
            "1,000 m to km",
            "1e3 m to km",
            "1 GiB to MiB",
            "0xff to decimal",
            "0b1010 + 1",
        ] {
            assert!(provider.auto_claim(query), "{query}");
            assert!(
                matches!(
                    provider.search(query)[0].action,
                    CommandAction::CopyToClipboard(_)
                ),
                "{query}"
            );
        }
    }
    #[test]
    fn ordinary_app_queries_are_not_claimed_as_invalid_dates() {
        let provider = isolated_provider();
        for query in ["in design", "meet at work", "spotify", "1password", "pi"] {
            assert!(!provider.auto_claim(query), "{query}");
        }
    }
}
