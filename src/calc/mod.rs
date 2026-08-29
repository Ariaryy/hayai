//! NL-detected calculator: arithmetic, unit conversion, currency
//! conversion. Claims matching queries via `auto_claim` (tried before
//! keyword routing — see `plugins::PluginRegistry::dispatch`), and stays
//! reachable explicitly via the ` = ` keyword.
//!
//! Arithmetic and units are pure, synchronous, zero-dependency evaluation —
//! always answered inline, never debounced. Currency needs a rate table
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

pub struct EvalResult {
    pub expression: String,
    /// The answer, shown large/bold — also what gets copied on Enter.
    pub value: String,
}

pub struct CalcProvider {
    fx: currency::FxCache,
}

impl CalcProvider {
    pub fn new() -> Self {
        Self {
            fx: currency::FxCache::load(),
        }
    }
}

impl Default for CalcProvider {
    fn default() -> Self {
        Self::new()
    }
}

fn evaluate(input: &str, fx: &currency::FxCache) -> Option<EvalResult> {
    // These adapters provide OS-backed rates and wall-clock data, plus Hayai's
    // established display conventions for common conversions. The general
    // expression engine handles arithmetic and compositions of those values.
    if let Some(result) = currency::convert(input, fx) {
        return Some(result);
    }
    if let Some(result) = time::convert(input) {
        return Some(result);
    }
    if let Some(result) = units::convert(input) {
        return Some(result);
    }
    if let Some(result) = bases::convert(input) {
        return Some(result);
    }
    if let Some(result) = engine::evaluate(input, Some(fx)) {
        return Some(result);
    }
    None
}

/// Two-stage false-positive gate: a cheap first-character check declines
/// non-candidates instantly (this runs on every keystroke across every
/// provider), then each evaluator's own full parse decides for real.
fn is_candidate(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    input.chars().next().is_some_and(|c| {
        c.is_ascii_digit()
            || c == '('
            || c == '.'
            || c == '-'
            || c == '@'
            || currency::is_symbol(c)
            || matches!(
                lower.as_str(),
                "pi" | "e" | "tau" | "today" | "tomorrow" | "yesterday"
            )
            || lower.starts_with("in ")
            || lower.starts_with("now ")
            || lower
                .split_once('(')
                .is_some_and(|(name, _)| name.chars().all(|c| c.is_ascii_alphabetic()))
    })
}

fn result_item(query: &str, result: EvalResult) -> CommandItem {
    let calculation_detail =
        time::timezone_labels(query).map(|(source_label, target_label)| CalculationDetail {
            source_label,
            target_label,
        });
    CommandItem {
        id: format!("calc:{query}"),
        title: result.value.clone(),
        subtitle: Some(if calculation_detail.is_some() {
            query.to_string()
        } else {
            result.expression
        }),
        calculation_detail,
        icon: IconSource::None,
        action: CommandAction::CopyToClipboard(result.value),
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

    /// Claims only when confident a real result follows — a claim with no
    /// result would show as a blank list instead of falling back to the
    /// default app search, which is worse than not claiming at all.
    fn auto_claim(&self, query: &str) -> bool {
        let trimmed = query.trim();
        if trimmed.is_empty() || !is_candidate(trimmed) {
            return false;
        }
        units::convert(trimmed).is_some()
            || currency::recognizes_any(trimmed, &self.fx)
            || bases::convert(trimmed).is_some()
            || time::convert(trimmed).is_some()
            || engine::evaluate(trimmed, Some(&self.fx)).is_some()
    }

    fn search(&self, query: &str) -> Vec<CommandItem> {
        let trimmed = query.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }
        match evaluate(trimmed, &self.fx) {
            Some(result) => vec![result_item(trimmed, result)],
            None => Vec::new(),
        }
    }

    fn background_search(&self, query: &str) -> Option<BackgroundSearch> {
        let trimmed = query.trim();
        if !currency::needs_fetch(trimmed, &self.fx) || !currency::mark_fetching(&self.fx) {
            return None;
        }
        let fx = self.fx.clone();
        let query = trimmed.to_string();
        Some(Box::new(move || {
            currency::fetch_blocking(&fx);
            match evaluate(&query, &fx) {
                Some(result) => vec![result_item(&query, result)],
                None => vec![CommandItem {
                    id: "calc:fx-error".into(),
                    title: "Couldn't fetch exchange rates".into(),
                    subtitle: Some("Check your internet connection and try again".into()),
                    calculation_detail: None,
                    icon: IconSource::None,
                    action: CommandAction::ShowText(String::new()),
                }],
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
    }

    #[test]
    fn auto_claim_units() {
        let provider = CalcProvider::new();
        assert!(provider.auto_claim("10 km in miles"));
        assert!(!provider.auto_claim("10 km in kg"));
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
    fn search_accepts_bare_number() {
        let provider = CalcProvider::new();
        assert_eq!(provider.search("7")[0].title, "7");
    }
}
