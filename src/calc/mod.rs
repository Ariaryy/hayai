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
mod editing;
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
    result: Evaluation,
}

#[derive(Clone)]
struct Evaluation {
    result: Option<EvalResult>,
    /// Only previews own a different source; complete queries reuse the caller's input.
    source: Option<String>,
    pending: bool,
}

fn resolve(input: &str, fx: &currency::FxCache) -> Evaluation {
    if editing::word_operator_source(input).is_none()
        && let Some(result) = evaluate(input, fx)
    {
        return Evaluation {
            result: Some(result),
            source: None,
            pending: false,
        };
    }
    if let Some(pending) = editing::source(input) {
        let result = evaluate(&pending.source, fx);
        if result.is_some()
            || currency::needs_fetch(&pending.source, fx)
            || (pending.awaiting_operand
                && (pending.source.is_empty() || time::unfinished_duration(&pending.source)))
        {
            return Evaluation {
                result,
                source: Some(pending.source.into_owned()),
                pending: true,
            };
        }
    }
    Evaluation {
        result: None,
        source: None,
        pending: false,
    }
}

fn evaluation_items(
    query: &str,
    evaluation: Evaluation,
    fx: &currency::FxCache,
) -> Vec<CommandItem> {
    let source = evaluation.source.as_deref().unwrap_or(query);
    match evaluation.result {
        Some(result) => vec![result_item(source, result, fx)],
        None if evaluation.pending => vec![CommandItem {
            id: "calc:pending".into(),
            action: CommandAction::PendingState,
            ..error_item("Keep typing to complete the calculation.")
        }],
        None => vec![error_item(if time::is_time_query(query) {
            "Invalid date or time. Try another expression."
        } else {
            "Invalid expression. Try another calculation."
        })],
    }
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
    fn evaluated(&self, query: &str) -> Evaluation {
        let revision = self.fx.revision();
        let mut memo = self.memo.borrow_mut();
        if let Some(memo) = memo.as_ref()
            && memo.query == query
            && memo.revision == revision
            && memo.at.elapsed() < std::time::Duration::from_secs(1)
        {
            return memo.result.clone();
        }
        let result = resolve(query, &self.fx);
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
            .or_else(|| engine::evaluate(input, Some(fx))),
    }
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
            || (c.is_ascii_alphabetic()
                && (grammar::split_conversion(input).is_some()
                    || lower
                        .split_whitespace()
                        .last()
                        .is_some_and(|word| matches!(word, "to" | "in" | "as"))))
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
        let evaluated = self.evaluated(trimmed);
        evaluated.result.is_some()
            || evaluated.pending
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
        evaluation_items(trimmed, self.evaluated(trimmed), &self.fx)
    }

    fn background_search(&self, query: &str) -> Option<BackgroundSearch> {
        let trimmed = query.trim();
        if trimmed.len() > 4096 {
            return None;
        }
        let evaluation = self.evaluated(trimmed);
        let source = evaluation.source.as_deref().unwrap_or(trimmed);
        if !currency::needs_fetch(source, &self.fx) {
            return None;
        }
        let fx = self.fx.clone();
        let query = trimmed.to_string();
        Some(Box::new(move || {
            currency::fetch_blocking(&fx);
            let evaluation = resolve(&query, &fx);
            if evaluation.result.is_none()
                && currency::needs_fetch(evaluation.source.as_deref().unwrap_or(&query), &fx)
            {
                vec![error_item(
                    "Couldn't fetch exchange rates. Check your connection.",
                )]
            } else {
                evaluation_items(&query, evaluation, &fx)
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
            assert_eq!(items[0].subtitle.as_deref(), Some("12 + 12"));
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
    #[test]
    fn pending_suffixes_preview_the_same_complete_input() {
        let provider = isolated_provider();
        for (pending, source) in [
            ("1usd to ", "1usd"),
            ("1 USD in", "1 USD"),
            ("$1 as ", "$1"),
            ("1usd to i", "1usd"),
            ("1usd to ru", "1usd"),
            ("1usd +", "1usd"),
            ("10 km to ", "10 km"),
            ("10 km to kilome", "10 km"),
            ("26 to he", "26"),
            ("0x", "0"),
            ("26 as", "26"),
            ("12 + 12 +", "12 + 12"),
            ("12 + 12 * -", "12 + 12"),
            ("2 plus", "2"),
            ("2 mod", "2"),
            ("2modulo", "2"),
            ("20% +", "20%"),
            ("5! +", "5!"),
            ("20% of", "20%"),
            ("2 * (", "2"),
            ("(2 + 3", "(2 + 3)"),
            ("2 * (3 +", "2 * (3)"),
            ("sqrt(9", "sqrt(9)"),
            ("sum(1,2,", "sum(1,2)"),
            ("1e+", "1"),
            ("4pm +", "4pm"),
            ("4pm + 30 mi", "4pm + 30"),
            ("12pm UTC to ", "12pm UTC"),
            ("12pm UTC to New Y", "12pm UTC"),
        ] {
            assert!(provider.auto_claim(pending), "did not claim {pending}");
            let preview = provider.search(pending).remove(0);
            let complete = provider.search(source).remove(0);
            assert_eq!(preview.title, complete.title, "{pending}");
            if !pending.contains('(') {
                assert_eq!(preview.subtitle, complete.subtitle, "{pending}");
            }
            assert!(
                matches!(preview.action, CommandAction::CopyToClipboard(_)),
                "{pending}"
            );
        }
    }

    #[test]
    fn unfinished_starts_are_neutral_and_finished_errors_stay_invalid() {
        let provider = isolated_provider();
        for query in [
            "(",
            "sqrt(",
            "in 2",
            "in 2 ho",
            "in 2 hours 30 mi",
            "€",
            "₹",
            "-",
            ".",
            "-.",
        ] {
            assert!(provider.auto_claim(query), "{query}");
            assert!(
                matches!(
                    provider.search(query)[0].action,
                    CommandAction::PendingState
                ),
                "{query}"
            );
        }
        for query in [
            "2 / 0",
            "2 / 0 +",
            "10 km to kg",
            "10 km to nope",
            "1 USD to xyzzy",
            "2 + unknown +",
            "min(1,)",
            "2 + 3)",
        ] {
            assert!(
                matches!(provider.search(query)[0].action, CommandAction::EmptyState),
                "{query}"
            );
        }
        // Deleting or replacing the expression must never keep a different query's result.
        provider.search("2+2");
        assert!(matches!(
            provider.search("10 km to kg")[0].action,
            CommandAction::EmptyState
        ));
        assert_eq!(provider.search("26 to he")[0].title, "26");
        assert_eq!(provider.search("26 to hex")[0].title, "0x1A");
    }
    #[test]
    fn unicode_editing_boundaries_are_safe() {
        let provider = isolated_provider();
        for query in ["€to", "₹plus", "£", "1 km to µ", "in 2\u{00a0}ho"] {
            assert_eq!(provider.search(query).len(), 1, "{query}");
        }
    }
}
