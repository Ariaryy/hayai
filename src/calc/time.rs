//! Clock-time arithmetic ("1pm + 5"), timezone conversion ("3pm est to
//! pst"), and relative-date arithmetic ("5 days from now", "2 weeks ago").
//!
//! No `chrono`/`time` crate: dates are hand-rolled via Howard Hinnant's
//! `days_from_civil`/`civil_from_days` algorithms (public domain,
//! well-known — see http://howardhinnant.github.io/date_algorithms.html),
//! and "now" comes from `native::local_now` (OS local wall-clock time, so
//! DST is already baked in for the *current* date). Explicit abbreviations
//! use fixed offsets (`CST` always means GMT-6); named cities use Windows'
//! timezone rules for the requested date, avoiding a bundled timezone database.

use super::EvalResult;
use super::grammar;
use crate::native;
use std::time::{SystemTime, UNIX_EPOCH};

const SYSTEM_TIMEZONE: &str = "__system_timezone";
type CivilDateTime = (i32, u32, u32, u32, u32);

fn timezone_tokens(input: &str) -> Option<(String, String, i32, i32)> {
    let input = input.trim();
    let first_end = input.find(char::is_whitespace)?;
    let first = &input[..first_end];
    let rest = input[first_end..].trim_start();
    let (from_token, keyword_and_target) = if let Some((_, clock_rest)) = parse_leading_clock(input)
    {
        let timezone_end = clock_rest
            .find(char::is_whitespace)
            .unwrap_or(clock_rest.len());
        (
            &clock_rest[..timezone_end],
            clock_rest[timezone_end..].trim_start(),
        )
    } else {
        (first, rest)
    };
    let source_zone = resolve_zone(from_token)?;
    let (to_token, matched) = grammar::parse_keyword_then_target(keyword_and_target);
    let to_token = if matched {
        to_token
    } else if parse_leading_clock(input).is_some() && keyword_and_target.is_empty() {
        SYSTEM_TIMEZONE.to_string()
    } else {
        return None;
    };
    let target_zone = resolve_zone(&to_token)?;
    let (year, month, day, _, _) = native::local_now();
    let sample = (year, month, day, 12, 0);
    let (_, from_offset) = zone_to_utc(&source_zone, sample)?;
    let (_, to_offset) = zone_to_utc(&target_zone, sample)?;
    Some((from_token.to_string(), to_token, from_offset, to_offset))
}

fn parse_timezone_difference(input: &str) -> Option<EvalResult> {
    if parse_leading_clock(input).is_some() {
        return None;
    }
    let (from, _, from_offset, to_offset) = timezone_tokens(input)?;
    let difference = to_offset - from_offset;
    let value = if difference == 0 {
        "Same UTC offset".to_string()
    } else {
        let minutes = difference.abs();
        let hours = minutes / 60;
        let remaining_minutes = minutes % 60;
        let amount = match (hours, remaining_minutes) {
            (0, minutes) => format!("{minutes} min"),
            (hours, 0) => format!("{hours} hr"),
            (hours, minutes) => format!("{hours} hr {minutes} min"),
        };
        format!(
            "{amount} {}",
            if difference > 0 { "ahead" } else { "behind" }
        )
    };
    Some(EvalResult {
        copy_value: None,
        expression: from.to_uppercase(),
        value,
    })
}

pub fn timezone_labels(input: &str) -> Option<(String, String)> {
    if let Some(parsed) = parse_zoned_clock(input) {
        let source = resolve_zone(&parsed.from_token)?;
        let target = resolve_zone(&parsed.to_token)?;
        let (utc, source_offset) = zone_to_utc(&source, parsed.local)?;
        let (_, target_offset) = zone_from_utc(&target, utc)?;
        return Some((source.label(source_offset), target.label(target_offset)));
    }
    let (from, to, from_offset, to_offset) = timezone_tokens(input)?;
    let source = resolve_zone(&from)?;
    let target = resolve_zone(&to)?;
    Some((source.label(from_offset), target.label(to_offset)))
}

/// Parses a clock-time token like `"3pm"`, `"3:30pm"`, or `"15:30"` into
/// 24-hour `(hour, minute)`. Requires a colon or an am/pm suffix — a bare
/// number like `"5"` is deliberately rejected so this never collides with
/// plain arithmetic (`"5 + 3"` must stay a calculator expression).
fn parse_clock(token: &str) -> Option<(u32, u32)> {
    let lower = token.to_lowercase();
    let (digits, meridiem) = if let Some(d) = lower.strip_suffix("am") {
        (d, Some(false))
    } else if let Some(d) = lower.strip_suffix("pm") {
        (d, Some(true))
    } else {
        (lower.as_str(), None)
    };
    if digits.is_empty() || (meridiem.is_none() && !digits.contains(':')) {
        return None;
    }
    let (h_str, m_str) = digits.split_once(':').unwrap_or((digits, "0"));
    let mut hour: u32 = h_str.parse().ok()?;
    let minute: u32 = m_str.parse().ok()?;
    if minute >= 60 {
        return None;
    }
    match meridiem {
        Some(true) => {
            if hour == 0 || hour > 12 {
                return None;
            }
            if hour != 12 {
                hour += 12;
            }
        }
        Some(false) => {
            if hour == 0 || hour > 12 {
                return None;
            }
            if hour == 12 {
                hour = 0;
            }
        }
        None => {
            if hour >= 24 {
                return None;
            }
        }
    }
    Some((hour, minute))
}

fn parse_leading_clock(input: &str) -> Option<((u32, u32), &str)> {
    let input = input.trim_start();
    let first_end = input
        .find(|c: char| c.is_whitespace() || matches!(c, '+' | '-'))
        .unwrap_or(input.len());
    let first = &input[..first_end];
    let rest = input[first_end..].trim_start();
    if let Some(clock) = parse_clock(first) {
        return Some((clock, rest));
    }

    let meridiem_end = rest
        .find(|c: char| c.is_whitespace() || matches!(c, '+' | '-'))
        .unwrap_or(rest.len());
    let meridiem = &rest[..meridiem_end];
    if !matches!(meridiem.to_ascii_lowercase().as_str(), "am" | "pm") {
        return None;
    }
    let clock = parse_clock(&format!("{first}{meridiem}"))?;
    Some((clock, rest[meridiem_end..].trim_start()))
}

struct ParsedZonedClock {
    local: CivilDateTime,
    from_token: String,
    to_token: String,
    explicit_date: bool,
}

fn month_number(token: &str) -> Option<u32> {
    let lower = token.to_ascii_lowercase();
    [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ]
    .iter()
    .position(|month| month.starts_with(&lower) && lower.len() >= 3)
    .map(|index| index as u32 + 1)
}

fn valid_date(year: i64, month: u32, day: u32) -> bool {
    (1..=9999).contains(&year)
        && (1..=12).contains(&month)
        && day >= 1
        && day <= days_in_month(year, month)
}

/// Recognition is distinct from successful evaluation: malformed clock/date
/// requests must report an error, not fall through to physical units/arithmetic.
pub fn is_time_query(input: &str) -> bool {
    let lower = input.trim().to_ascii_lowercase();
    parse_leading_clock(input).is_some()
        || (input.trim_start().starts_with(|c: char| c.is_ascii_digit()) && lower.contains(" at "))
        || lower
            .strip_prefix("in ")
            .is_some_and(|rest| grammar::parse_amount(rest.trim_start()).is_some())
        || (grammar::parse_amount(input.trim_start()).is_some()
            && [" from now", " after now", " before now", " ago", " later"]
                .iter()
                .any(|suffix| lower.ends_with(suffix)))
}

fn parse_zoned_clock(input: &str) -> Option<ParsedZonedClock> {
    let input = input.trim();
    let lower = input.to_ascii_lowercase();
    let (year, month, day, clock_input, explicit_date) = if let Some(at) = lower.find(" at ") {
        let date = input[..at].split_whitespace().collect::<Vec<_>>();
        if !(2..=3).contains(&date.len()) {
            return None;
        }
        let day = date[0].trim_end_matches([',', '.']).parse::<u32>().ok()?;
        let month = month_number(date[1].trim_end_matches([',', '.']))?;
        let year = if let Some(year) = date.get(2) {
            year.trim_end_matches([',', '.']).parse::<i32>().ok()?
        } else {
            native::local_now().0
        };
        (year, month, day, &input[at + 4..], true)
    } else {
        let (year, month, day, _, _) = native::local_now();
        (year, month, day, input, false)
    };

    if !valid_date(year as i64, month, day) {
        return None;
    }
    let ((hour, minute), rest) = parse_leading_clock(clock_input)?;
    let (from_token, to_token) = if let Some((from, to)) = grammar::split_conversion(rest) {
        (from.to_string(), to.to_string())
    } else {
        (rest.to_string(), SYSTEM_TIMEZONE.to_string())
    };
    resolve_zone(&from_token)?;
    resolve_zone(&to_token)?;
    Some(ParsedZonedClock {
        local: (year, month, day, hour, minute),
        from_token,
        to_token,
        explicit_date,
    })
}

fn format_clock(total_minutes: i64) -> String {
    let m = total_minutes.rem_euclid(24 * 60);
    let hour24 = m / 60;
    let minute = m % 60;
    let (h12, suffix) = match hour24 {
        0 => (12, "AM"),
        1..=11 => (hour24, "AM"),
        12 => (12, "PM"),
        _ => (hour24 - 12, "PM"),
    };
    format!("{h12}:{minute:02} {suffix}")
}

// All OS date calculations stay within a small, explicitly supported domain.
// Reject before float-to-integer conversion; Rust's saturating cast is not validation.
const MAX_SECONDS: f64 = 366.0 * 10_000.0 * 86_400.0;
fn bounded_seconds(value: f64) -> Option<i64> {
    (value.is_finite() && value.abs() <= MAX_SECONDS).then(|| value.round() as i64)
}

fn day_note(days: i64) -> String {
    match days {
        0 => String::new(),
        1 => " (+1 day)".into(),
        -1 => " (-1 day)".into(),
        n if n > 0 => format!(" (+{n} days)"),
        n => format!(" ({n} days)"),
    }
}

fn format_clock_seconds(seconds: i64) -> String {
    let clock = format_clock(seconds.div_euclid(60));
    let remainder = seconds.rem_euclid(60);
    if remainder == 0 {
        return clock;
    }
    let (time, meridiem) = clock.split_once(' ').unwrap();
    format!("{time}:{remainder:02} {meridiem}")
}

/// "1pm + 5", "9:30am - 2 hours", or "4pm + 30 min".
fn parse_clock_arithmetic(input: &str) -> Option<EvalResult> {
    let input = input.trim();
    let ((hour, minute), rest) = parse_leading_clock(input)?;

    let (sign, rest) = if let Some(r) = rest.strip_prefix('+') {
        (1.0, r)
    } else {
        (-1.0, rest.strip_prefix('-')?)
    };
    let rest = rest.trim();
    let offset_seconds = if let Ok(hours) = rest.parse::<f64>() {
        hours * 3600.0
    } else {
        let duration = parse_duration(rest)?;
        if duration.months != 0 {
            return None;
        }
        duration.seconds + duration.days as f64 * 86_400.0
    };

    let source = hour as i64 * 3600 + minute as i64 * 60;
    let target = source.checked_add(bounded_seconds(sign * offset_seconds)?)?;
    Some(EvalResult {
        copy_value: None,
        expression: input.trim().to_string(),
        value: format!(
            "{}{}",
            format_clock_seconds(target),
            day_note(target.div_euclid(86_400))
        ),
    })
}

/// Converts clock times between fixed abbreviations, DST-aware named places,
/// and the current Windows timezone, with an optional leading calendar date.
fn parse_timezone_conversion(input: &str) -> Option<EvalResult> {
    let parsed = parse_zoned_clock(input)?;
    let source_zone = resolve_zone(&parsed.from_token)?;
    let target_zone = resolve_zone(&parsed.to_token)?;
    let (utc, source_offset) = zone_to_utc(&source_zone, parsed.local)?;
    let (target, target_offset) = zone_from_utc(&target_zone, utc)?;
    if !valid_date(target.0 as i64, target.1, target.2) {
        return None;
    }
    let source_days = days_from_civil(parsed.local.0 as i64, parsed.local.1, parsed.local.2);
    let target_days = days_from_civil(target.0 as i64, target.1, target.2);
    let day_shift = target_days - source_days;
    let day_note = match day_shift {
        0 => String::new(),
        1 => " (+1 day)".to_string(),
        -1 => " (-1 day)".to_string(),
        n if n > 0 => format!(" (+{n} days)"),
        n => format!(" ({n} days)"),
    };

    let source_abbreviation = source_zone.abbreviation(source_offset);
    let target_abbreviation = target_zone.abbreviation(target_offset);
    let source_clock = parsed.local.3 as i64 * 60 + parsed.local.4 as i64;
    let target_clock = target.3 as i64 * 60 + target.4 as i64;
    let value = if parsed.explicit_date {
        format!(
            "{} {}, {}, {} {}",
            MONTHS[target.1 as usize - 1],
            target.2,
            target.0,
            format_clock(target_clock),
            target_abbreviation
        )
    } else {
        format!(
            "{}{day_note} {}",
            format_clock(target_clock),
            target_abbreviation
        )
    };
    Some(EvalResult {
        copy_value: None,
        expression: format!("{} {source_abbreviation}", format_clock(source_clock)),
        value,
    })
}

pub(super) fn unfinished_duration(input: &str) -> bool {
    let lower = input.to_ascii_lowercase();
    let Some(duration) = lower.strip_prefix("in ") else {
        return false;
    };
    let duration = duration.trim();
    let start = duration
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace() || c.is_ascii_alphabetic())
        .map_or(0, |(index, c)| index + c.len_utf8());
    let (head, amount) = duration.split_at(start);
    grammar::parse_amount(amount).is_some_and(|(_, consumed)| consumed == amount.len())
        && (head.trim().is_empty() || parse_duration(head.trim()).is_some())
}

pub(super) fn duration_prefix(word: &str) -> bool {
    [
        "seconds", "secs", "minutes", "mins", "hours", "hrs", "days", "weeks", "wks", "months",
        "years", "yrs",
    ]
    .iter()
    .any(|name| name.starts_with(word) && *name != word)
}

fn clock_literal(input: &str) -> Option<EvalResult> {
    let ((hour, minute), rest) = parse_leading_clock(input)?;
    if !rest.is_empty() {
        return None;
    }
    Some(EvalResult {
        expression: input.to_string(),
        value: format_clock(i64::from(hour) * 60 + i64::from(minute)),
        copy_value: None,
    })
}

pub fn convert(input: &str) -> Option<EvalResult> {
    parse_clock_arithmetic(input)
        .or_else(|| parse_timezone_difference(input))
        .or_else(|| parse_timezone_conversion(input))
        .or_else(|| parse_date_arithmetic(input))
        .or_else(|| clock_literal(input))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clock_plus_hours() {
        let result = convert("1pm + 5").unwrap();
        assert_eq!(result.value, "6:00 PM");
    }

    #[test]
    fn clock_minus_hours_wraps_backward() {
        let result = convert("1am - 3").unwrap();
        assert_eq!(result.value, "10:00 PM (-1 day)");
    }

    #[test]
    fn rejects_bare_number_arithmetic() {
        // Must stay a plain calculator expression, not a clock offset.
        assert!(parse_clock_arithmetic("5 + 3").is_none());
    }

    #[test]
    fn timezone_conversion_same_day() {
        let result = convert("9am est to pst").unwrap();
        assert_eq!(result.expression, "9:00 AM EST");
        assert_eq!(result.value, "6:00 AM PST");

        let spaced = convert("12 pm ist to cst").unwrap();
        assert_eq!(spaced.expression, "12:00 PM IST");
        assert_eq!(spaced.value, "12:30 AM CST");
        assert!(timezone_labels("12 pm ist to cst").is_some());

        let local = convert("12pm utc").unwrap();
        assert_eq!(local.expression, "12:00 PM UTC");
        assert!(local.value.split_whitespace().count() >= 3);
        let labels = timezone_labels("12pm utc").unwrap();
        assert!(labels.1.contains("(GMT"));
        assert!(!labels.1.starts_with("System timezone"));
    }

    #[test]
    fn ambiguous_abbreviations_get_explicit_region_labels() {
        let labels = timezone_labels("12pm cst to ist").unwrap();
        assert_eq!(labels.0, "America/Chicago · CST (GMT-6)");
        assert_eq!(labels.1, "Asia/Kolkata · IST (GMT+5:30)");

        let china = timezone_labels("12pm china to ist").unwrap();
        assert_eq!(china.0, "Asia/Shanghai · CST (GMT+8)");

        let without_time = timezone_labels("ist to cst").unwrap();
        assert_eq!(without_time.0, "Asia/Kolkata · IST (GMT+5:30)");
        assert_eq!(without_time.1, "America/Chicago · CST (GMT-6)");
    }

    #[test]
    fn compares_timezone_offsets_without_a_clock() {
        let result = convert("ist to cst").unwrap();
        assert_eq!(result.expression, "IST");
        assert_eq!(result.value, "11 hr 30 min behind");
        assert_eq!(convert("cst to ist").unwrap().value, "11 hr 30 min ahead");
        assert_eq!(convert("utc to gmt").unwrap().value, "Same UTC offset");
    }

    #[test]
    fn timezone_conversion_notes_day_shift() {
        let result = convert("11pm est to ist").unwrap();
        assert_eq!(result.value, "9:30 AM (+1 day) IST");
    }

    #[cfg(windows)]
    #[test]
    fn city_conversions_follow_windows_daylight_rules() {
        let city = convert("29 August 2026 at 12pm chicago to ist").unwrap();
        assert_eq!(city.expression, "12:00 PM CDT");
        assert_eq!(city.value, "August 29, 2026, 10:30 PM IST");
        let labels = timezone_labels("29 August 2026 at 12pm chicago to ist").unwrap();
        assert_eq!(labels.0, "America/Chicago · CDT (GMT-5)");

        let fixed = convert("29 August 2026 at 12pm cst to ist").unwrap();
        assert_eq!(fixed.expression, "12:00 PM CST");
        assert_eq!(fixed.value, "August 29, 2026, 11:30 PM IST");
    }

    #[cfg(windows)]
    #[test]
    fn parses_dated_city_timezone_phrases() {
        let result = convert("3 Aug 2026 at 10pm shibuya to pst").unwrap();
        assert_eq!(result.expression, "10:00 PM JST");
        assert_eq!(result.value, "August 3, 2026, 5:00 AM PST");
        let labels = timezone_labels("3 Aug 2026 at 10pm shibuya to pst").unwrap();
        assert_eq!(labels.0, "Asia/Tokyo · JST (GMT+9)");
        assert_eq!(labels.1, "America/Los_Angeles · PST (GMT-8)");
    }

    #[test]
    fn days_from_now_uses_local_date() {
        let (year, month, day, _, _) = native::local_now();
        let today = days_from_civil(year as i64, month, day);
        let expected = format_date(today + 5);
        let result = convert("5 days from now").unwrap();
        assert_eq!(result.value, expected);
    }

    #[test]
    fn weeks_ago() {
        let (year, month, day, _, _) = native::local_now();
        let today = days_from_civil(year as i64, month, day);
        let expected = format_date(today - 14);
        let result = convert("2 weeks ago").unwrap();
        assert_eq!(result.value, expected);
    }

    #[test]
    fn relative_seconds_use_an_injected_clock() {
        let now = (2026, 10, 23, 16, 0, 15);
        assert!(
            parse_date_arithmetic_at("in 30sec", now)
                .unwrap()
                .value
                .ends_with("4:00:45 PM")
        );
        assert!(
            parse_date_arithmetic_at("in 15 mins", now)
                .unwrap()
                .value
                .ends_with("4:15:15 PM")
        );
        assert!(
            parse_date_arithmetic_at("in 1 month", (2024, 1, 31, 12, 0, 0))
                .unwrap()
                .value
                .contains("February 29, 2024")
        );
    }

    #[test]
    fn supports_multi_part_relative_offsets() {
        assert!(convert("in 2 hours 30 minutes").is_some());
        assert!(convert("1 year 3 months ago").is_some());
        assert!(
            convert("2 months ago at 5pm")
                .unwrap()
                .value
                .ends_with("5:00 PM")
        );
    }

    #[test]
    fn clock_arithmetic_accepts_duration_units() {
        assert_eq!(convert("4pm + 30 min").unwrap().value, "4:30 PM");
        assert_eq!(convert("4 pm + 30 min").unwrap().value, "4:30 PM");
        assert_eq!(convert("9:15am - 45 mins").unwrap().value, "8:30 AM");
    }

    #[test]
    fn civil_date_roundtrip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        let z = days_from_civil(2026, 7, 30);
        assert_eq!(civil_from_days(z), (2026, 7, 30));
    }
    #[test]
    fn validated_calendar_and_offsets() {
        for query in [
            "4pm + NaN",
            "4pm + inf",
            "4pm + 1e300",
            "in 1e300 years",
            "in 999999999 days",
            "in NaN seconds",
            "31 February 2026 at 12pm utc to gmt",
            "0 February 2026 at 12pm utc to gmt",
            "29 February 2025 at 12pm utc to gmt",
        ] {
            assert!(convert(query).is_none(), "{query}");
        }
        assert!(convert("29 February 2024 at 12pm utc to gmt").is_some());
        assert_eq!(convert("4pm+30sec").unwrap().value, "4:00:30 PM");
        assert_eq!(convert("4pm + 30 MIN").unwrap().value, "4:30 PM");
        assert_eq!(convert("11pm+3").unwrap().value, "2:00 AM (+1 day)");
        assert!(parse_date_arithmetic_at("in 1 day", (9999, 12, 31, 12, 0, 0)).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn multiword_places_and_iana_names_share_zone_rules() {
        for source in [
            "newyork",
            "New York",
            "America/New_York",
            "Eastern Standard Time",
        ] {
            assert_eq!(
                convert(&format!("1 July 2026 at 12pm {source} to UTC"))
                    .unwrap()
                    .value,
                "July 1, 2026, 4:00 PM UTC",
                "{source}"
            );
        }
        assert!(convert("8 March 2026 at 2:30am New York to UTC").is_none());
    }
    #[cfg(windows)]
    #[test]
    fn elapsed_and_calendar_offsets_cross_dst_deliberately() {
        let now = (2026, 3, 8, 1, 30, 0);
        let zone = Some("Eastern Standard Time");
        assert!(
            parse_date_arithmetic_with_zone("in 2 hours", now, zone)
                .unwrap()
                .value
                .ends_with("4:30 AM")
        );
        assert!(
            parse_date_arithmetic_with_zone("in 24 hours", now, zone)
                .unwrap()
                .value
                .ends_with("2:30 AM")
        );
        assert!(
            parse_date_arithmetic_with_zone("in 1 day at 1:30am", now, zone)
                .unwrap()
                .value
                .ends_with("1:30 AM")
        );
        assert!(
            parse_date_arithmetic_with_zone("in 1 day at 2:30am", (2026, 3, 7, 1, 30, 0), zone)
                .is_none()
        );
    }
}

mod calendar;
use calendar::*;
mod zones;
pub(super) use zones::target_prefix;
use zones::*;
