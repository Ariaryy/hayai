//! Currency conversion. Rates are fetched from a free, keyless FX API,
//! cached to `%AppData%\hayai\fx.json` with a ~24h TTL, and served stale
//! (with a subtitle annotation) when offline or between fetches.
//!
//! The cache (`FxCache`) is a plain `Arc<RwLock<..>>` rather than a GPUI
//! global: the fetch itself runs inside a `CommandProvider::background_search`
//! job (see `CalcProvider`), which only gets a `Send` closure, not `cx`. This
//! also means the existing debounce → `apply_results` pipeline (built for
//! file search) is what repaints the row once a fetch lands — no bespoke
//! refresh mechanism needed.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::EvalResult;
use super::format::format_currency;
use super::grammar;
use crate::native;

const TTL: Duration = Duration::from_secs(24 * 60 * 60);
const API_HOST: &str = "open.er-api.com";
const API_PATH: &str = "/v6/latest/USD";

/// Recognized ISO currency codes (lowercase). Gates both auto-claim and the
/// grammar's `from`/`to` tokens so arbitrary three-letter words (or unit
/// abbreviations) never get treated as currencies.
const KNOWN_CODES: &[&str] = &[
    "usd", "eur", "gbp", "jpy", "inr", "aud", "cad", "chf", "cny", "sgd", "hkd", "nzd", "sek",
    "nok", "dkk", "krw", "mxn", "brl", "zar", "rub", "try", "aed", "sar", "thb", "myr", "idr",
    "php", "vnd", "pln", "czk", "huf", "ils", "twd", "pkr", "bdt", "egp", "ngn", "kes",
];

fn symbol_to_code(symbol: char) -> Option<&'static str> {
    match symbol {
        '$' => Some("usd"),
        '€' => Some("eur"),
        '£' => Some("gbp"),
        '¥' => Some("jpy"),
        '₹' => Some("inr"),
        '₩' => Some("krw"),
        _ => None,
    }
}

pub fn is_symbol(c: char) -> bool {
    symbol_to_code(c).is_some()
}

fn is_known(code: &str) -> bool {
    KNOWN_CODES.contains(&code)
}

struct FxSnapshot {
    rates: Option<HashMap<String, f64>>,
    as_of: Option<String>,
    fetched_at: Option<SystemTime>,
}

/// Cheap to clone (all fields are `Arc`, or fixed at construction) — every
/// `CalcProvider` search call and background job shares the same underlying
/// state.
#[derive(Clone)]
pub struct FxCache {
    inner: Arc<RwLock<FxSnapshot>>,
    fetching: Arc<AtomicBool>,
    /// The lowercase ISO code implied by the OS region setting, when it's
    /// one we recognize — the implicit "convert to" target for a bare
    /// amount like "$100" with no explicit target. Fixed at construction;
    /// doesn't need to live behind the `RwLock`.
    default_target: Option<String>,
}

impl FxCache {
    /// Synchronous disk read of the last cached rates, if any. Cheap enough
    /// (a few KB) to run on the UI thread at provider construction time,
    /// mirroring `FileRecents::load`.
    pub fn load() -> Self {
        let snapshot = match load_from_disk() {
            Some((fetched_at, rates, as_of)) => FxSnapshot {
                rates: Some(rates),
                as_of,
                fetched_at: Some(fetched_at),
            },
            None => FxSnapshot {
                rates: None,
                as_of: None,
                fetched_at: None,
            },
        };
        Self {
            inner: Arc::new(RwLock::new(snapshot)),
            fetching: Arc::new(AtomicBool::new(false)),
            default_target: native::system_currency_code().filter(|code| is_known(code)),
        }
    }

    fn default_target(&self) -> Option<String> {
        self.default_target.clone()
    }

    fn is_stale(&self) -> bool {
        let guard = self.inner.read().unwrap();
        match guard.fetched_at {
            None => true,
            Some(fetched_at) => SystemTime::now()
                .duration_since(fetched_at)
                .map(|age| age > TTL)
                .unwrap_or(true),
        }
    }

    fn rate_for(&self, code: &str) -> Option<f64> {
        self.inner
            .read()
            .unwrap()
            .rates
            .as_ref()?
            .get(code)
            .copied()
    }

    fn as_of_label(&self) -> Option<String> {
        self.inner.read().unwrap().as_of.clone()
    }
}

fn load_from_disk() -> Option<(SystemTime, HashMap<String, f64>, Option<String>)> {
    let contents = std::fs::read_to_string(cache_file()?).ok()?;
    let (header, body) = contents.split_once('\n')?;
    let seconds: u64 = header.trim().parse().ok()?;
    let rates = extract_rates(body)?;
    let as_of = extract_string_field(body, "time_last_update_utc");
    Some((UNIX_EPOCH + Duration::from_secs(seconds), rates, as_of))
}

fn write_to_disk(body: &str) {
    let Some(path) = cache_file() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = std::fs::write(path, format!("{seconds}\n{body}"));
}

fn cache_file() -> Option<PathBuf> {
    let app_data = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(app_data).join("hayai").join("fx.json"))
}

/// Extracts the flat `"rates":{"usd":1.0,"eur":0.92,...}` object out of the
/// API response body. Deliberately not a general JSON parser — the response
/// shape is fixed (flat code→number pairs, no nesting), so a small scanner
/// is enough and keeps this dependency-free.
fn extract_rates(body: &str) -> Option<HashMap<String, f64>> {
    let key_idx = body.find("\"rates\"")?;
    let brace_rel = body[key_idx..].find('{')?;
    let start = key_idx + brace_rel + 1;
    let end = start + body[start..].find('}')?;
    let mut rates = HashMap::new();
    for pair in body[start..end].split(',') {
        let (key, value) = pair.split_once(':')?;
        let key = key.trim().trim_matches('"').to_lowercase();
        let value: f64 = value.trim().parse().ok()?;
        if !key.is_empty() {
            rates.insert(key, value);
        }
    }
    if rates.is_empty() { None } else { Some(rates) }
}

fn extract_string_field(body: &str, field: &str) -> Option<String> {
    let needle = format!("\"{field}\"");
    let after_key = &body[body.find(&needle)? + needle.len()..];
    let after_colon = &after_key[after_key.find(':')? + 1..];
    let start = after_colon.find('"')? + 1;
    let end = start + after_colon[start..].find('"')?;
    Some(after_colon[start..end].to_string())
}

/// Blocking network fetch + cache update. Only ever called from a
/// `background_search` job (background pool), never the UI thread.
fn fetch_and_store(cache: &FxCache) -> bool {
    let Some(body) = native::https_get(API_HOST, API_PATH) else {
        return false;
    };
    let Some(rates) = extract_rates(&body) else {
        return false;
    };
    let as_of = extract_string_field(&body, "time_last_update_utc");
    write_to_disk(&body);
    let mut guard = cache.inner.write().unwrap();
    guard.rates = Some(rates);
    guard.as_of = as_of;
    guard.fetched_at = Some(SystemTime::now());
    true
}

/// Whether `input` is shaped like a currency conversion the fetched rate
/// table (once loaded) could answer — independent of whether rates are
/// loaded yet, so auto-claim and the "needs a fetch" check both work before
/// the very first fetch ever completes.
pub fn recognizes(input: &str) -> bool {
    parse_tokens(input).is_some()
}

fn parse_tokens(input: &str) -> Option<(f64, String, String)> {
    if let Some((code, (amount, to))) = parse_symbol_form(input) {
        return Some((amount, code.to_string(), to));
    }
    let (amount, from, to) = grammar::parse_conversion(input)?;
    if is_known(&from) && is_known(&to) {
        Some((amount, from, to))
    } else {
        None
    }
}

/// Parses the "$100 to eur" shape: a leading currency symbol attaches
/// directly to the amount, with no separate from-token.
fn parse_symbol_form(input: &str) -> Option<(&'static str, (f64, String))> {
    let trimmed = input.trim();
    let first = trimmed.chars().next()?;
    let code = symbol_to_code(first)?;
    let rest = &trimmed[first.len_utf8()..];

    let (amount, consumed) = grammar::parse_amount(rest)?;
    let rest = rest[consumed..].trim_start();

    let (target, matched) = grammar::parse_keyword_then_target(rest);
    if !matched || !is_known(&target) {
        return None;
    }
    Some((code, (amount, target)))
}

/// Parses a bare amount with no explicit target — `"$100"` or `"100 usd"`
/// and nothing else — the shape that gets auto-converted to the user's
/// regional currency (see `recognizes_bare`/`convert`).
fn parse_bare_amount(input: &str) -> Option<(f64, String)> {
    let trimmed = input.trim();
    let first = trimmed.chars().next()?;
    if let Some(code) = symbol_to_code(first) {
        let rest = &trimmed[first.len_utf8()..];
        let (amount, consumed) = grammar::parse_amount(rest)?;
        if !rest[consumed..].trim().is_empty() {
            return None;
        }
        return Some((amount, code.to_string()));
    }

    let (amount, consumed) = grammar::parse_amount(trimmed)?;
    let code = trimmed[consumed..].trim();
    if code.is_empty() || code.contains(char::is_whitespace) {
        return None;
    }
    let code = code.to_lowercase();
    is_known(&code).then_some((amount, code))
}

/// Whether `input` is a bare currency amount (see `parse_bare_amount`) that
/// the cache has a distinct regional default target for — distinct so a
/// user whose region already matches the typed currency doesn't get a
/// no-op "$100 -> $100" card.
fn recognizes_bare(input: &str, cache: &FxCache) -> bool {
    match (parse_bare_amount(input), cache.default_target()) {
        (Some((_, from)), Some(to)) => from != to,
        _ => false,
    }
}

/// True when `input` needs a network fetch before it can be answered: it's
/// currency-shaped (explicit target or a bare amount with a regional
/// default), but the cache is missing or past its TTL.
pub fn needs_fetch(input: &str, cache: &FxCache) -> bool {
    (recognizes(input) || recognizes_bare(input, cache)) && cache.is_stale()
}

/// Whether `input` is currency-shaped at all — explicit target or bare
/// amount with a usable regional default — regardless of fetch/staleness
/// state. Used by `CalcProvider::auto_claim`.
pub fn recognizes_any(input: &str, cache: &FxCache) -> bool {
    recognizes(input) || recognizes_bare(input, cache)
}

/// Claims the right to run a fetch for this cache, returning `false` if
/// another `background_search` job already has one in flight (so a burst of
/// keystrokes doesn't pile up redundant HTTP round trips).
pub fn mark_fetching(cache: &FxCache) -> bool {
    cache
        .fetching
        .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

/// Runs the blocking fetch and releases the in-flight claim. Call only after
/// `mark_fetching` returned `true`.
pub fn fetch_blocking(cache: &FxCache) -> bool {
    let ok = fetch_and_store(cache);
    cache.fetching.store(false, Ordering::SeqCst);
    ok
}

/// A currency symbol to prefix the display value with, for the handful of
/// majors that have one; other currencies fall back to a trailing code
/// suffix (see `display_amount`).
fn symbol_prefix(code: &str) -> Option<&'static str> {
    match code {
        "usd" => Some("$"),
        "eur" => Some("€"),
        "gbp" => Some("£"),
        "jpy" => Some("¥"),
        "inr" => Some("₹"),
        "krw" => Some("₩"),
        _ => None,
    }
}

fn display_amount(code: &str, amount: f64) -> String {
    match symbol_prefix(code) {
        Some(symbol) => format!("{symbol}{}", format_currency(amount)),
        None => format!("{} {}", format_currency(amount), code.to_uppercase()),
    }
}

/// Attempts a currency conversion using whatever rates are currently
/// cached (possibly stale). Tries an explicit target first ("100 usd to
/// inr", "$100 to eur"); falls back to a bare amount ("$100") converted to
/// the user's regional default. `None` if the input isn't currency-shaped
/// at all, or no rates are loaded yet.
pub fn convert(input: &str, cache: &FxCache) -> Option<EvalResult> {
    let (amount, from, to) = match parse_tokens(input) {
        Some(tokens) => tokens,
        None => {
            let (amount, from) = parse_bare_amount(input)?;
            let to = cache.default_target()?;
            if to == from {
                return None;
            }
            (amount, from, to)
        }
    };
    let from_rate = cache.rate_for(&from)?;
    let to_rate = cache.rate_for(&to)?;
    let converted = amount / from_rate * to_rate;
    let staleness = cache
        .as_of_label()
        .map(|date| format!(" (rates from {date})"))
        .unwrap_or_default();
    Some(EvalResult {
        expression: format!("{}{staleness}", display_amount(&from, amount)),
        value: display_amount(&to, converted),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cache_with_rates(pairs: &[(&str, f64)]) -> FxCache {
        cache_with_rates_and_default(pairs, None)
    }

    fn cache_with_rates_and_default(
        pairs: &[(&str, f64)],
        default_target: Option<&str>,
    ) -> FxCache {
        let rates = pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        FxCache {
            inner: Arc::new(RwLock::new(FxSnapshot {
                rates: Some(rates),
                as_of: Some("2026-01-01".to_string()),
                fetched_at: Some(SystemTime::now()),
            })),
            fetching: Arc::new(AtomicBool::new(false)),
            default_target: default_target.map(str::to_string),
        }
    }

    #[test]
    fn recognizes_code_form() {
        assert!(recognizes("100 usd to inr"));
        assert!(!recognizes("100 km to inr"));
    }

    #[test]
    fn recognizes_symbol_form() {
        assert!(recognizes("$100 to eur"));
    }

    #[test]
    fn converts_with_cached_rates() {
        let cache = cache_with_rates(&[("usd", 1.0), ("inr", 83.0)]);
        let result = convert("100 usd to inr", &cache).unwrap();
        assert_eq!(result.value, "₹8,300.00");
    }

    #[test]
    fn converts_symbol_form() {
        let cache = cache_with_rates(&[("usd", 1.0), ("eur", 0.9)]);
        let result = convert("$100 to eur", &cache).unwrap();
        assert_eq!(result.value, "€90.00");
    }

    #[test]
    fn converts_k_suffix_amount() {
        let cache = cache_with_rates(&[("usd", 1.0), ("inr", 83.0)]);
        let result = convert("5k usd to inr", &cache).unwrap();
        assert_eq!(result.value, "₹415,000.00");
        let result = convert("$5k to inr", &cache).unwrap();
        assert_eq!(result.value, "₹415,000.00");
    }

    #[test]
    fn converts_bare_amount_to_regional_default() {
        let cache = cache_with_rates_and_default(&[("usd", 1.0), ("inr", 83.0)], Some("inr"));
        let result = convert("$100", &cache).unwrap();
        assert_eq!(result.value, "₹8,300.00");
    }

    #[test]
    fn bare_amount_declines_without_regional_default() {
        let cache = cache_with_rates(&[("usd", 1.0), ("inr", 83.0)]);
        assert!(convert("$100", &cache).is_none());
    }

    #[test]
    fn bare_amount_declines_when_matching_default() {
        let cache = cache_with_rates_and_default(&[("usd", 1.0)], Some("usd"));
        assert!(!recognizes_bare("$100", &cache));
        assert!(convert("$100", &cache).is_none());
    }

    #[test]
    fn declines_without_rates() {
        let cache = FxCache::load();
        // Fresh load may or may not find a disk cache in the test
        // environment; only assert the no-disk-cache case is handled.
        if cache.rate_for("usd").is_none() {
            assert!(convert("100 usd to inr", &cache).is_none());
        }
    }

    #[test]
    fn extracts_rates_from_api_shape() {
        let body = r#"{"result":"success","time_last_update_utc":"Wed, 01 Jan 2026 00:00:00 +0000","rates":{"USD":1,"EUR":0.92,"INR":83.1}}"#;
        let rates = extract_rates(body).unwrap();
        assert_eq!(rates.get("usd"), Some(&1.0));
        assert_eq!(rates.get("eur"), Some(&0.92));
        assert_eq!(
            extract_string_field(body, "time_last_update_utc"),
            Some("Wed, 01 Jan 2026 00:00:00 +0000".to_string())
        );
    }
}
