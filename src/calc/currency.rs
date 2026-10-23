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
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use super::EvalResult;
use super::format;
use super::grammar;
use crate::native;

const TTL: Duration = Duration::from_secs(24 * 60 * 60);
const API_HOST: &str = "open.er-api.com";
const API_PATH: &str = "/v6/latest/USD";

/// Recognized ISO currency codes (lowercase). Gates both auto-claim and the
/// grammar's `from`/`to` tokens so arbitrary three-letter words (or unit
/// abbreviations) never get treated as currencies.
const KNOWN_CODES: &[&str] = &[
    "aed", "afn", "all", "amd", "ang", "aoa", "ars", "aud", "awg", "azn", "bam", "bbd", "bdt",
    "bgn", "bhd", "bif", "bmd", "bnd", "bob", "bov", "brl", "bsd", "btn", "bwp", "byn", "bzd",
    "cad", "cdf", "che", "chf", "chw", "clf", "clp", "cny", "cop", "cou", "crc", "cuc", "cup",
    "cve", "czk", "djf", "dkk", "dop", "dzd", "egp", "ern", "etb", "eur", "fjd", "fkp", "gbp",
    "gel", "ghs", "gip", "gmd", "gnf", "gtq", "gyd", "hkd", "hnl", "hrk", "htg", "huf", "idr",
    "ils", "inr", "iqd", "irr", "isk", "jmd", "jod", "jpy", "kes", "kgs", "khr", "kmf", "kpw",
    "krw", "kwd", "kyd", "kzt", "lak", "lbp", "lkr", "lrd", "lsl", "lyd", "mad", "mdl", "mga",
    "mkd", "mmk", "mnt", "mop", "mru", "mur", "mvr", "mwk", "mxn", "mxv", "myr", "mzn", "nad",
    "ngn", "nio", "nok", "npr", "nzd", "omr", "pab", "pen", "pgk", "php", "pkr", "pln", "pyg",
    "qar", "ron", "rsd", "rub", "rwf", "sar", "sbd", "scr", "sdg", "sek", "sgd", "shp", "sle",
    "sll", "sos", "srd", "ssp", "stn", "svc", "syp", "szl", "thb", "tjs", "tmt", "tnd", "top",
    "try", "ttd", "twd", "tzs", "uah", "ugx", "usd", "usn", "uyi", "uyu", "uyw", "uzs", "ved",
    "ves", "vnd", "vuv", "wst", "xaf", "xag", "xau", "xba", "xbb", "xbc", "xbd", "xcd", "xdr",
    "xof", "xpd", "xpf", "xpt", "xsu", "xts", "xua", "xxx", "yer", "zar", "zmw", "zwl",
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

fn normalize_code(token: &str) -> Option<&str> {
    match token {
        "dollar" | "dollars" => Some("usd"),
        "euro" | "euros" => Some("eur"),
        "pound" | "pounds" | "sterling" => Some("gbp"),
        "yen" => Some("jpy"),
        "rupee" | "rupees" => Some("inr"),
        "yuan" | "renminbi" => Some("cny"),
        "won" => Some("krw"),
        code if is_known(code) => Some(code),
        name => KNOWN_CODES
            .iter()
            .copied()
            .filter(|code| currency_name(code) != "Currency")
            .find(|code| {
                currency_name(code).eq_ignore_ascii_case(name)
                    || name
                        .strip_suffix('s')
                        .is_some_and(|singular| currency_name(code).eq_ignore_ascii_case(singular))
            }),
    }
}

struct FxSnapshot {
    rates: Option<Arc<HashMap<String, f64>>>,
    as_of: Option<String>,
    fetched_at: Option<SystemTime>,
}

/// Cheap to clone (all fields are `Arc`, or fixed at construction) — every
/// `CalcProvider` search call and background job shares the same underlying
/// state.
#[derive(Clone)]
pub struct FxCache {
    inner: Arc<RwLock<FxSnapshot>>,
    refresh: Arc<Mutex<Option<Instant>>>,
    revision: Arc<AtomicU64>,
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
                rates: Some(Arc::new(rates)),
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
            refresh: Arc::new(Mutex::new(None)),
            revision: Arc::new(AtomicU64::new(0)),
            default_target: native::system_currency_code().filter(|code| is_known(code)),
        }
    }

    pub(super) fn revision(&self) -> u64 {
        self.revision.load(Ordering::Acquire)
    }

    #[cfg(test)]
    pub(super) fn empty() -> Self {
        Self {
            inner: Arc::new(RwLock::new(FxSnapshot {
                rates: None,
                as_of: None,
                fetched_at: None,
            })),
            refresh: Arc::new(Mutex::new(None)),
            revision: Arc::new(AtomicU64::new(0)),
            default_target: None,
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

    pub(super) fn snapshot(&self) -> Option<Arc<HashMap<String, f64>>> {
        self.inner.read().unwrap().rates.clone()
    }

    #[cfg(test)]
    pub(super) fn rate_for(&self, code: &str) -> Option<f64> {
        let code = code.to_ascii_lowercase();
        self.inner
            .read()
            .unwrap()
            .rates
            .as_ref()?
            .get(&code)
            .copied()
    }
}

fn load_from_disk() -> Option<(SystemTime, HashMap<String, f64>, Option<String>)> {
    let contents = std::fs::read_to_string(cache_file()?).ok()?;
    let (header, body) = contents.split_once('\n')?;
    let seconds: u64 = header.trim().parse().ok()?;
    let rates = extract_rates(body)?;
    let as_of = extract_string_field(body, "time_last_update_utc");
    Some((
        UNIX_EPOCH.checked_add(Duration::from_secs(seconds))?,
        rates,
        as_of,
    ))
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

/// Validate the entire snapshot before publishing any rate. serde_json is
/// already used by the app, so this adds no dependency or second JSON parser.
fn extract_rates(body: &str) -> Option<HashMap<String, f64>> {
    let json: serde_json::Value = serde_json::from_str(body).ok()?;
    if json.get("result").and_then(serde_json::Value::as_str) != Some("success") {
        return None;
    }
    let mut rates = HashMap::new();
    for (code, rate) in json.get("rates")?.as_object()? {
        let rate = rate.as_f64()?;
        if !rate.is_finite() || rate <= 0.0 {
            return None;
        }
        if is_known(&code.to_ascii_lowercase()) {
            rates.insert(code.to_ascii_lowercase(), rate);
        }
    }
    (rates.get("usd") == Some(&1.0)).then_some(rates)
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
    guard.rates = Some(Arc::new(rates));
    guard.as_of = as_of;
    guard.fetched_at = Some(SystemTime::now());
    cache.revision.fetch_add(1, Ordering::Release);
    true
}

/// Whether `input` is shaped like a currency conversion the fetched rate
/// table (once loaded) could answer — independent of whether rates are
/// loaded yet, so auto-claim and the "needs a fetch" check both work before
/// the very first fetch ever completes.
pub fn recognizes(input: &str) -> bool {
    parse_tokens(input).is_some()
}

fn parse_tokens(input: &str) -> Option<(String, String, String)> {
    if let Some((code, (amount, to))) = parse_symbol_form(input) {
        return Some((amount, code.to_string(), to));
    }
    let (amount, from, to) = (|| {
        let input = input.trim();
        let (amount, consumed) = grammar::amount_token(input)?;
        let (from, to) = grammar::split_conversion(input[consumed..].trim_start())?;
        Some((amount, from.to_string(), to.to_string()))
    })()
    .or_else(|| parse_currency_pair(input).map(|(from, to)| ("1".into(), from, to)))?;
    Some((
        amount,
        normalize_code(&from.to_ascii_lowercase())?.to_string(),
        target_code(&to)?,
    ))
}

/// Parses a currency pair without an explicit amount ("USD to INR"). A
/// missing amount means one source unit, matching how exchange rates are
/// conventionally presented.
fn parse_currency_pair(input: &str) -> Option<(String, String)> {
    let (from, to) = grammar::split_conversion(input.trim())?;
    Some((from.to_string(), to.to_string()))
}

/// Parses the "$100 to eur" shape: a leading currency symbol attaches
/// directly to the amount, with no separate from-token.
fn parse_symbol_form(input: &str) -> Option<(&'static str, (String, String))> {
    let trimmed = input.trim();
    let first = trimmed.chars().next()?;
    let code = symbol_to_code(first)?;
    let rest = trimmed[first.len_utf8()..].trim_start();

    let (amount, consumed) = grammar::amount_token(rest)?;
    let rest = rest[consumed..].trim_start();

    let (target, matched) = grammar::parse_keyword_then_target(rest);
    if !matched {
        return None;
    }
    let target = target_code(&target)?;
    Some((code, (amount, target)))
}

/// Parses a bare amount with no explicit target — `"$100"` or `"100 usd"`
/// and nothing else — the shape that gets auto-converted to the user's
/// regional currency (see `recognizes_bare`/`convert`).
fn parse_bare_amount(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim();
    let first = trimmed.chars().next()?;
    if let Some(code) = symbol_to_code(first) {
        let rest = trimmed[first.len_utf8()..].trim_start();
        let (amount, consumed) = grammar::amount_token(rest)?;
        if !rest[consumed..].trim().is_empty() {
            return None;
        }
        return Some((amount, code.to_string()));
    }

    let (amount, consumed) = grammar::amount_token(trimmed)?;
    let code = trimmed[consumed..].trim();
    if code.is_empty() || code.contains(char::is_whitespace) {
        return None;
    }
    // Lowercase "cup" is the kitchen unit; an isolated currency amount
    // must use CUP. Explicit currency pairs can disambiguate lowercase cup.
    if code == "cup" {
        return None;
    }
    let code = code.to_lowercase();
    Some((amount, normalize_code(&code)?.to_string()))
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
    if !cache.is_stale() {
        return false;
    }
    if let Some((_, from, to)) = parse_tokens(input) {
        return from != to;
    }
    if parse_bare_amount(input).is_some() {
        return recognizes_bare(input, cache);
    }
    contains_currency(input)
}

/// Whether `input` is currency-shaped at all — explicit target or bare
/// amount with a usable regional default — regardless of fetch/staleness
/// state. Used by `CalcProvider::auto_claim`.
pub fn recognizes_any(input: &str, cache: &FxCache) -> bool {
    recognizes(input) || recognizes_bare(input, cache) || contains_currency(input)
}

/// The job owns no refresh state until it actually runs. All callers can
/// schedule a job; workers share one serialized refresh, then each evaluates
/// its own query against the completed snapshot. Cancellation cannot strand
/// a flag and an older query cannot steal a newer query's completion.
pub fn fetch_blocking(cache: &FxCache) -> bool {
    refresh_with(cache, || fetch_and_store(cache))
}

fn refresh_with(cache: &FxCache, fetch: impl FnOnce() -> bool) -> bool {
    let mut last_failure = cache.refresh.lock().unwrap();
    if !cache.is_stale() {
        return true;
    }
    if last_failure.is_some_and(|at| at.elapsed() < Duration::from_secs(30)) {
        return false;
    }
    let success = fetch();
    *last_failure = (!success).then(Instant::now);
    success
}

fn target_code(token: &str) -> Option<String> {
    let token = token.trim();
    if token.chars().count() == 1 {
        return symbol_to_code(token.chars().next()?).map(str::to_string);
    }
    normalize_code(&token.to_ascii_lowercase()).map(str::to_string)
}

pub(super) fn target_prefix(target: &str) -> bool {
    let target = target.to_ascii_lowercase();
    if target.is_empty() || target_code(&target).is_some() {
        return false;
    }
    KNOWN_CODES.iter().any(|code| code.starts_with(&target))
        || KNOWN_CODES.iter().any(|code| {
            currency_name(code)
                .to_ascii_lowercase()
                .starts_with(&target)
        })
        || [
            "dollars", "euros", "rupees", "pounds", "sterling", "yen", "yuan", "renminbi", "won",
        ]
        .iter()
        .any(|name| name.starts_with(&target))
}

pub(super) fn contains_currency(input: &str) -> bool {
    parse_tokens(input).is_some()
        || input.chars().any(is_symbol)
        || input.split(|c: char| !c.is_ascii_alphabetic()).any(|word| {
            (word != "cup" && is_known(&word.to_ascii_lowercase()))
                || matches!(
                    word.to_ascii_lowercase().as_str(),
                    "dollar"
                        | "dollars"
                        | "euro"
                        | "euros"
                        | "rupee"
                        | "rupees"
                        | "yen"
                        | "yuan"
                        | "renminbi"
                )
        })
}

/// Only rewrite a query with unambiguous currency intent. In particular,
/// "pounds to kg" must retain the physical meaning of pounds.
pub(super) fn expression(input: &str, cache: &FxCache) -> Option<String> {
    if !contains_currency(input) {
        return None;
    }
    if let Some((_, from, to)) = parse_tokens(input) {
        return Some(format!(
            "{} {} to {}",
            source_amount(input)?,
            from.to_ascii_uppercase(),
            to.to_ascii_uppercase()
        ));
    }
    let mut out = String::with_capacity(input.len() + 16);
    let mut offset = 0;
    while offset < input.len() {
        let ch = input[offset..].chars().next()?;
        if let Some(code) = symbol_to_code(ch) {
            offset += ch.len_utf8();
            let tail = input[offset..].trim_start();
            if let Some((amount, consumed)) = grammar::amount_token(tail) {
                out.push_str(&format!("({amount} {})", code.to_ascii_uppercase()));
                offset = input.len() - tail.len() + consumed;
            } else {
                out.push_str(&code.to_ascii_uppercase());
            }
        } else if ch.is_ascii_alphabetic() {
            let end = input[offset..]
                .find(|c: char| !c.is_ascii_alphabetic())
                .map_or(input.len(), |i| offset + i);
            let word = &input[offset..end];
            if word != "cup"
                && let Some(code) = normalize_code(&word.to_ascii_lowercase())
            {
                out.push_str(&code.to_ascii_uppercase());
            } else {
                out.push_str(word);
            }
            offset = end;
        } else {
            out.push(ch);
            offset += ch.len_utf8();
        }
    }
    // The conversion-only thousands shorthand is normalized before the engine.
    if let Some((amount, from, to)) = grammar::conversion_tokens(&out) {
        out = format!("{amount} {from} to {to}");
    }
    if parse_bare_amount(input).is_some() {
        let (_, from) = parse_bare_amount(input)?;
        let target = cache.default_target();
        if let Some((amount, consumed)) = grammar::amount_token(input.trim()) {
            out = format!(
                "{amount} {}",
                input.trim()[consumed..].trim().to_ascii_uppercase()
            );
        }
        if let Some(target) = target
            && from != target
        {
            out = format!("{out} to {}", target.to_ascii_uppercase());
        }
    }
    Some(out)
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

fn minor_places(code: &str) -> usize {
    match code {
        "jpy" | "krw" | "vnd" | "clp" | "isk" | "xaf" | "xof" | "xpf" | "bif" | "djf" | "gnf"
        | "pyg" | "rwf" | "ugx" | "vuv" => 0,
        "bhd" | "iqd" | "jod" | "kwd" | "lyd" | "omr" | "tnd" => 3,
        _ => 2,
    }
}

fn display_amount(code: &str, amount: &str) -> Option<String> {
    let value = format::currency(amount, minor_places(code))?;
    let upper = code.to_ascii_uppercase();
    Some(match symbol_prefix(code) {
        Some(symbol) => format!("{symbol}{value} {upper}"),
        None => format!("{value} {upper}"),
    })
}

fn source_amount(input: &str) -> Option<String> {
    let input = input.trim();
    let first = input.chars().next()?;
    let numeric = if is_symbol(first) {
        input[first.len_utf8()..].trim_start()
    } else {
        input
    };
    let (amount, _) = grammar::amount_token(numeric).unwrap_or(("1".into(), 0));
    Some(super::engine::evaluate(&amount, None)?.value)
}

fn currency_label(code: &str) -> String {
    match currency_name(code) {
        "Currency" => code.to_ascii_uppercase(),
        name => name.to_string(),
    }
}

fn currency_name(code: &str) -> &'static str {
    match code {
        "usd" => "US Dollar",
        "eur" => "Euro",
        "gbp" => "British Pound",
        "jpy" => "Japanese Yen",
        "inr" => "Indian Rupee",
        "aud" => "Australian Dollar",
        "cad" => "Canadian Dollar",
        "chf" => "Swiss Franc",
        "cny" => "Chinese Yuan",
        "sgd" => "Singapore Dollar",
        "hkd" => "Hong Kong Dollar",
        "nzd" => "New Zealand Dollar",
        "sek" => "Swedish Krona",
        "nok" => "Norwegian Krone",
        "dkk" => "Danish Krone",
        "krw" => "South Korean Won",
        "mxn" => "Mexican Peso",
        "brl" => "Brazilian Real",
        "zar" => "South African Rand",
        "rub" => "Russian Ruble",
        "try" => "Turkish Lira",
        "aed" => "UAE Dirham",
        "sar" => "Saudi Riyal",
        "thb" => "Thai Baht",
        "myr" => "Malaysian Ringgit",
        "idr" => "Indonesian Rupiah",
        "php" => "Philippine Peso",
        "vnd" => "Vietnamese Dong",
        "pln" => "Polish Zloty",
        "czk" => "Czech Koruna",
        "huf" => "Hungarian Forint",
        "ils" => "Israeli Shekel",
        "twd" => "Taiwan Dollar",
        "pkr" => "Pakistani Rupee",
        "bdt" => "Bangladeshi Taka",
        "egp" => "Egyptian Pound",
        "ngn" => "Nigerian Naira",
        "kes" => "Kenyan Shilling",
        "npr" => "Nepalese Rupee",
        "lkr" => "Sri Lankan Rupee",
        "kwd" => "Kuwaiti Dinar",
        "bhd" => "Bahraini Dinar",
        "omr" => "Omani Rial",
        "qar" => "Qatari Riyal",
        "uah" => "Ukrainian Hryvnia",
        "ron" => "Romanian Leu",
        _ => "Currency",
    }
}

/// Attempts a currency conversion using whatever rates are currently
/// cached (possibly stale). Tries an explicit target first ("100 usd to
/// inr", "$100 to eur"); falls back to a bare amount ("$100") converted to
/// the user's regional default. `None` if the input isn't currency-shaped
/// at all, or no rates are loaded yet.
pub fn convert(input: &str, cache: &FxCache) -> Option<EvalResult> {
    // A literal with no distinct regional target needs no rate table. Keep
    // it usable as a preview even on the very first offline launch.
    if let Some((_, from)) = parse_bare_amount(input)
        && cache.default_target().is_none_or(|target| target == from)
    {
        let amount = source_amount(input)?;
        let value = display_amount(&from, &amount)?;
        return Some(EvalResult {
            expression: value.clone(),
            value,
            copy_value: Some(format!("{amount} {}", from.to_ascii_uppercase())),
        });
    }
    let expression = expression(input, cache)?;
    let mut result = super::engine::evaluate(&expression, Some(cache))?;
    result.copy_value = Some(result.value.clone());
    if let Some((_amount, from, to)) = parse_tokens(input).or_else(|| {
        let (amount, from) = parse_bare_amount(input)?;
        Some((amount, from, cache.default_target()?))
    }) {
        result.expression = display_amount(&from, &source_amount(input)?)?;
        if let Some(number) = result.value.split_whitespace().next() {
            result.value = display_amount(&to, number)?;
        }
    } else {
        result.expression = input.trim().to_string();
    }
    Some(result)
}

pub(super) fn conversion_detail(input: &str, cache: &FxCache) -> Option<(String, String, String)> {
    let (_, from, to) = parse_tokens(input).or_else(|| {
        let (amount, from) = parse_bare_amount(input)?;
        Some((amount, from, cache.default_target()?))
    })?;
    let guard = cache.inner.read().unwrap();
    let rate_date = guard.as_of.as_deref().and_then(compact_rate_date);
    let fetched = guard.fetched_at.map(relative_update_label);
    let mut updated_label = rate_date
        .map(|date| format!("Rates as of {date}"))
        .or(fetched)?;
    if cache_age_is_stale(guard.fetched_at) {
        updated_label.push_str(" · stale cached rates");
    }
    Some((currency_label(&from), currency_label(&to), updated_label))
}

fn cache_age_is_stale(fetched_at: Option<SystemTime>) -> bool {
    fetched_at.is_none_or(|at| {
        SystemTime::now()
            .duration_since(at)
            .map_or(true, |age| age > TTL)
    })
}

fn relative_update_label(updated_at: SystemTime) -> String {
    let age = SystemTime::now()
        .duration_since(updated_at)
        .unwrap_or_default();
    match age.as_secs() {
        0..=59 => "Updated less than a minute ago".to_string(),
        60..=119 => "Updated 1 minute ago".to_string(),
        120..=3_599 => format!("Updated {} minutes ago", age.as_secs() / 60),
        3_600..=7_199 => "Updated 1 hour ago".to_string(),
        7_200..=86_399 => format!("Updated {} hours ago", age.as_secs() / 3_600),
        86_400..=172_799 => "Updated 1 day ago".to_string(),
        seconds => format!("Updated {} days ago", seconds / 86_400),
    }
}

fn compact_rate_date(date: &str) -> Option<String> {
    if date.len() == 10
        && date.as_bytes().get(4) == Some(&b'-')
        && date.as_bytes().get(7) == Some(&b'-')
    {
        return Some(date.to_string());
    }
    let mut parts = date.split_whitespace();
    let _weekday = parts.next()?;
    let day = parts.next()?;
    let month = parts.next()?;
    let year = parts.next()?;
    Some(format!("{day} {month} {year}"))
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
                rates: Some(Arc::new(rates)),
                as_of: Some("2026-01-01".to_string()),
                fetched_at: Some(SystemTime::now()),
            })),
            refresh: Arc::new(Mutex::new(None)),
            revision: Arc::new(AtomicU64::new(0)),
            default_target: default_target.map(str::to_string),
        }
    }

    #[test]
    fn recognizes_code_form() {
        assert!(recognizes("100 usd to inr"));
        assert!(recognizes("usd to inr"));
        assert!(recognizes("1 yen to inr"));
        assert!(recognizes("5 dollars to rupees"));
        assert!(!recognizes("100 km to inr"));
    }

    #[test]
    fn recognizes_symbol_form() {
        assert!(recognizes("$100 to eur"));
    }

    #[test]
    fn converts_with_cached_rates() {
        let cache = cache_with_rates(&[("usd", 1.0), ("inr", 83.0), ("jpy", 150.0)]);
        let result = convert("100 usd to inr", &cache).unwrap();
        assert_eq!(result.value, "₹8,300.00 INR");
        assert_eq!(result.expression, "$100.00 USD");
        assert_eq!(
            conversion_detail("100 usd to inr", &cache),
            Some((
                "US Dollar".into(),
                "Indian Rupee".into(),
                "Rates as of 2026-01-01".into()
            ))
        );

        let yen = convert("1 yen to inr", &cache).unwrap();
        assert_eq!(yen.expression, "¥1 JPY");
        assert_eq!(yen.value, "₹0.55 INR");

        let rate = convert("usd to inr", &cache).unwrap();
        assert_eq!(rate.expression, "$1.00 USD");
        assert_eq!(rate.value, "₹83.00 INR");
        assert!(conversion_detail("usd to inr", &cache).is_some());
    }

    #[test]
    fn converts_symbol_form() {
        let cache = cache_with_rates(&[("usd", 1.0), ("eur", 0.9)]);
        let result = convert("$100 to eur", &cache).unwrap();
        assert_eq!(result.value, "€90.00 EUR");
    }

    #[test]
    fn converts_k_suffix_amount() {
        let cache = cache_with_rates(&[("usd", 1.0), ("inr", 83.0)]);
        let result = convert("5k usd to inr", &cache).unwrap();
        assert_eq!(result.value, "₹415,000.00 INR");
        let result = convert("$5k to inr", &cache).unwrap();
        assert_eq!(result.value, "₹415,000.00 INR");
    }

    #[test]
    fn converts_bare_amount_to_regional_default() {
        let cache = cache_with_rates_and_default(&[("usd", 1.0), ("inr", 83.0)], Some("inr"));
        let result = convert("$100", &cache).unwrap();
        assert_eq!(result.value, "₹8,300.00 INR");
    }

    #[test]
    fn bare_amount_stays_literal_without_regional_default() {
        let cache = cache_with_rates(&[("usd", 1.0), ("inr", 83.0)]);
        assert!(convert("$100", &cache).is_some());
    }

    #[test]
    fn bare_amount_stays_literal_when_matching_default() {
        let cache = cache_with_rates_and_default(&[("usd", 1.0)], Some("usd"));
        assert!(!recognizes_bare("$100", &cache));
        assert!(convert("$100", &cache).is_some());
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
    #[test]
    fn currency_syntax_and_precision_share_one_path() {
        let cache = cache_with_rates_and_default(
            &[
                ("usd", 1.0),
                ("eur", 0.9),
                ("inr", 83.0),
                ("npr", 133.0),
                ("kwd", 0.3),
            ],
            Some("inr"),
        );
        for query in [
            "$100 to rupees",
            "$ 100 to INR",
            "100 dollars to ₹",
            "1e2 USD to INR",
            "100USD to INR",
            "100 US dollars to Indian rupees",
        ] {
            assert_eq!(
                convert(query, &cache).unwrap().value,
                "₹8,300.00 INR",
                "{query}"
            );
        }
        assert_eq!(convert("1 USD to NPR", &cache).unwrap().value, "133.00 NPR");
        assert_eq!(convert("1 USD to KWD", &cache).unwrap().value, "0.300 KWD");
        assert_eq!(
            convert("100000000000000001 USD to USD", &cache)
                .unwrap()
                .value,
            "$100,000,000,000,000,001.00 USD"
        );
        let small = convert("0.000001 USD to INR", &cache).unwrap();
        assert_eq!(small.value, "₹0.000083 INR");
        assert_eq!(small.copy_value.as_deref(), Some("0.000083 INR"));
        assert!(convert("100 USD + 50 EUR to INR", &cache).is_some());
        assert!(needs_fetch("100 USD + 50 EUR to INR", &FxCache::empty()));
        assert!(needs_fetch("100 USD to NPR", &FxCache::empty()));
    }

    #[test]
    fn concurrent_queries_share_refresh_but_not_results() {
        use std::sync::atomic::AtomicUsize;
        let cache = FxCache::empty();
        let calls = AtomicUsize::new(0);
        let gate = std::sync::Barrier::new(3);
        std::thread::scope(|scope| {
            let first = scope.spawn(|| {
                gate.wait();
                refresh_with(&cache, || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    let mut snapshot = cache.inner.write().unwrap();
                    snapshot.rates = Some(Arc::new(HashMap::from([
                        ("usd".into(), 1.0),
                        ("inr".into(), 83.0),
                    ])));
                    snapshot.fetched_at = Some(SystemTime::now());
                    cache.revision.fetch_add(1, Ordering::Release);
                    true
                });
                convert("100 USD to INR", &cache).unwrap().value
            });
            let second = scope.spawn(|| {
                gate.wait();
                refresh_with(&cache, || {
                    calls.fetch_add(1, Ordering::SeqCst);
                    let mut snapshot = cache.inner.write().unwrap();
                    snapshot.rates = Some(Arc::new(HashMap::from([
                        ("usd".into(), 1.0),
                        ("inr".into(), 83.0),
                    ])));
                    snapshot.fetched_at = Some(SystemTime::now());
                    cache.revision.fetch_add(1, Ordering::Release);
                    true
                });
                convert("200 USD to INR", &cache).unwrap().value
            });
            gate.wait();
            assert_eq!(first.join().unwrap(), "₹8,300.00 INR");
            assert_eq!(second.join().unwrap(), "₹16,600.00 INR");
        });
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn failed_refresh_preserves_stale_data_and_backs_off() {
        let cache = cache_with_rates(&[("usd", 1.0), ("inr", 83.0)]);
        cache.inner.write().unwrap().fetched_at = Some(UNIX_EPOCH);
        assert!(!refresh_with(&cache, || false));
        assert!(!refresh_with(&cache, || panic!(
            "failure backoff must suppress another request"
        )));
        assert!(convert("100 USD to INR", &cache).is_some());
        assert!(
            conversion_detail("100 USD to INR", &cache)
                .unwrap()
                .2
                .contains("stale")
        );
    }

    #[test]
    fn reject_invalid_rate_snapshots() {
        for body in [
            r#"{"result":"error","rates":{"USD":1}}"#,
            r#"{"result":"success","rates":{"USD":0}}"#,
            r#"{"result":"success","rates":{"USD":1,"EUR":-1}}"#,
            r#"{"result":"success","rates":{"EUR":1}}"#,
        ] {
            assert!(extract_rates(body).is_none());
        }
    }
    #[test]
    fn an_evaluation_keeps_one_immutable_rate_snapshot() {
        let cache = cache_with_rates(&[("usd", 1.0), ("inr", 83.0)]);
        let before = cache.snapshot().unwrap();
        cache.inner.write().unwrap().rates = Some(Arc::new(HashMap::from([
            ("usd".into(), 1.0),
            ("inr".into(), 90.0),
        ])));
        assert_eq!(before.get("inr"), Some(&83.0));
        assert_eq!(cache.snapshot().unwrap().get("inr"), Some(&90.0));
    }
    #[test]
    fn pending_currency_queries_keep_prefix_labels_copy_precision_and_fetch_behavior() {
        use crate::commands::{CommandAction, CommandProvider};
        let provider = super::super::CalcProvider {
            fx: cache_with_rates_and_default(
                &[("usd", 1.0), ("inr", 83.0), ("eur", 0.9)],
                Some("inr"),
            ),
            memo: std::cell::RefCell::new(None),
        };
        let complete = provider.search("1usd").remove(0);
        for query in [
            "1usd to ",
            "1usd to i",
            "1usd to in",
            "$1 in ",
            "1usd +",
            "1usd to INR +",
        ] {
            let preview = provider.search(query).remove(0);
            assert_eq!(preview.title, complete.title, "{query}");
            assert_eq!(preview.subtitle, complete.subtitle, "{query}");
            let Some(crate::commands::CalculationDetail::Currency { target_label, .. }) =
                preview.calculation_detail
            else {
                panic!("lost prefix labels for {query}");
            };
            assert_eq!(target_label, "Indian Rupee");
            let CommandAction::CopyToClipboard(value) = preview.action else {
                panic!("lost prefix copy action");
            };
            assert_eq!(value, "83 INR");
        }
        assert_eq!(provider.search("1usd to eur")[0].title, "€0.90 EUR");
        assert!(matches!(
            provider.search("1usd / 0 +")[0].action,
            CommandAction::EmptyState
        ));
        provider.fx.inner.write().unwrap().fetched_at = Some(UNIX_EPOCH);
        assert!(provider.background_search("1usd to ").is_some());
        assert!(provider.background_search("1usd to i").is_some());
    }
}
