//! Clock-time arithmetic ("1pm + 5"), timezone conversion ("3pm est to
//! pst"), and relative-date arithmetic ("5 days from now", "2 weeks ago").
//!
//! No `chrono`/`time` crate: dates are hand-rolled via Howard Hinnant's
//! `days_from_civil`/`civil_from_days` algorithms (public domain,
//! well-known — see http://howardhinnant.github.io/date_algorithms.html),
//! and "now" comes from `native::local_now` (OS local wall-clock time, so
//! DST is already baked in for the *current* date). Timezone conversion
//! uses a small fixed UTC-offset table below — not DST-aware, since that
//! would need a real IANA tz database, which this project deliberately
//! avoids pulling in as a dependency.

use super::EvalResult;
use super::grammar;
use crate::native;

/// Recognized timezone/city tokens (lowercase) mapped to a fixed UTC offset
/// in minutes. Bare `cst` and `ist` retain their established North American
/// Central and India meanings; result metadata always exposes that choice.
/// China is available through `china`, `beijing`, `shanghai`, or `cst_asia`.
const TIMEZONES: &[(&[&str], i32)] = &[
    (&["utc", "gmt", "london"], 0),
    (&["bst"], 60),
    (
        &["cet", "paris", "berlin", "madrid", "rome", "amsterdam"],
        60,
    ),
    (&["cest"], 120),
    (&["eet", "athens", "cairo"], 120),
    (&["msk", "moscow"], 180),
    (&["est", "newyork", "nyc", "toronto"], -300),
    (&["edt"], -240),
    (&["cst", "chicago"], -360),
    (&["cdt"], -300),
    (&["mst", "denver"], -420),
    (&["mdt"], -360),
    (
        &["pst", "losangeles", "la", "sf", "sanfrancisco", "seattle"],
        -480,
    ),
    (&["pdt"], -420),
    (&["gst", "dubai"], 240),
    (
        &["ist", "mumbai", "delhi", "bangalore", "kolkata", "india"],
        330,
    ),
    (&["sgt", "singapore", "hongkong"], 480),
    (&["cst_asia", "beijing", "shanghai", "china"], 480),
    (&["jst", "tokyo"], 540),
    (&["kst", "seoul"], 540),
    (&["aest", "sydney", "melbourne"], 600),
    (&["aedt"], 660),
    (&["nzst", "auckland"], 720),
];

fn find_offset(token: &str) -> Option<i32> {
    let token = token.to_lowercase();
    TIMEZONES
        .iter()
        .find(|(names, _)| names.contains(&token.as_str()))
        .map(|(_, offset)| *offset)
}

fn format_utc_offset(minutes: i32) -> String {
    if minutes == 0 {
        return "GMT".to_string();
    }
    let sign = if minutes < 0 { '-' } else { '+' };
    let minutes = minutes.abs();
    if minutes % 60 == 0 {
        format!("GMT{sign}{}", minutes / 60)
    } else {
        format!("GMT{sign}{}:{:02}", minutes / 60, minutes % 60)
    }
}

fn timezone_label(token: &str, offset: i32) -> String {
    let lower = token.to_ascii_lowercase();
    let identity = match lower.as_str() {
        "cst" | "chicago" => "America/Chicago · CST",
        "cdt" => "America/Chicago · CDT",
        "cst_asia" | "beijing" | "shanghai" | "china" => "Asia/Shanghai · CST",
        "ist" | "mumbai" | "delhi" | "bangalore" | "kolkata" | "india" => "Asia/Kolkata · IST",
        "est" | "newyork" | "nyc" | "toronto" => "America/New_York · EST",
        "edt" => "America/New_York · EDT",
        "pst" | "losangeles" | "la" | "sf" | "sanfrancisco" | "seattle" => {
            "America/Los_Angeles · PST"
        }
        "pdt" => "America/Los_Angeles · PDT",
        "jst" | "tokyo" => "Asia/Tokyo · JST",
        "utc" | "gmt" | "london" => "Etc/UTC",
        _ => token,
    };
    format!("{identity} ({})", format_utc_offset(offset))
}

fn timezone_tokens(input: &str) -> Option<(String, String, i32, i32)> {
    let input = input.trim();
    let first_end = input.find(char::is_whitespace)?;
    let first = &input[..first_end];
    let rest = input[first_end..].trim_start();
    let (from_token, keyword_and_target) = if parse_clock(first).is_some() {
        let timezone_end = rest.find(char::is_whitespace)?;
        (&rest[..timezone_end], rest[timezone_end..].trim_start())
    } else {
        (first, rest)
    };
    let from_offset = find_offset(from_token)?;
    let (to_token, matched) = grammar::parse_keyword_then_target(keyword_and_target);
    if !matched {
        return None;
    }
    let to_offset = find_offset(&to_token)?;
    Some((from_token.to_string(), to_token, from_offset, to_offset))
}

fn parse_timezone_difference(input: &str) -> Option<EvalResult> {
    let first = input.split_whitespace().next()?;
    if parse_clock(first).is_some() {
        return None;
    }
    let (from, to, from_offset, to_offset) = timezone_tokens(input)?;
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
        expression: format!("{} to {}", from.to_uppercase(), to.to_uppercase()),
        value,
    })
}

pub fn timezone_labels(input: &str) -> Option<(String, String)> {
    let (from, to, from_offset, to_offset) = timezone_tokens(input)?;
    Some((
        timezone_label(&from, from_offset),
        timezone_label(&to, to_offset),
    ))
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

/// "1pm + 5", "9:30am - 2 hours", or "4pm + 30 min".
fn parse_clock_arithmetic(input: &str) -> Option<EvalResult> {
    let input = input.trim();
    let time_end = input.find(char::is_whitespace)?;
    let (hour, minute) = parse_clock(&input[..time_end])?;
    let rest = input[time_end..].trim_start();

    let (sign, rest) = if let Some(r) = rest.strip_prefix('+') {
        (1.0, r)
    } else {
        (-1.0, rest.strip_prefix('-')?)
    };
    let rest = rest.trim();
    let offset_minutes = if let Ok(hours) = rest.parse::<f64>() {
        hours * 60.0
    } else {
        let duration = parse_duration(rest)?;
        if duration.months != 0 {
            return None;
        }
        duration.seconds / 60.0
    };

    let source = hour as i64 * 60 + minute as i64;
    let target = source + (sign * offset_minutes).round() as i64;
    Some(EvalResult {
        expression: input.trim().to_string(),
        value: format_clock(target),
    })
}

/// "3pm est to pst" / "15:00 utc in ist" — converts a clock time between
/// timezones using the fixed offset table above.
fn parse_timezone_conversion(input: &str) -> Option<EvalResult> {
    let input = input.trim();
    let time_end = input.find(char::is_whitespace)?;
    let (hour, minute) = parse_clock(&input[..time_end])?;
    let rest = input[time_end..].trim_start();

    let tz_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    if tz_end == 0 {
        return None;
    }
    let from_token = &rest[..tz_end];
    let from_offset = find_offset(from_token)?;
    let rest = rest[tz_end..].trim_start();

    let (to_token, matched) = grammar::parse_keyword_then_target(rest);
    if !matched {
        return None;
    }
    let to_offset = find_offset(&to_token)?;

    let source = hour as i64 * 60 + minute as i64;
    let target = source - from_offset as i64 + to_offset as i64;
    let day_shift = target.div_euclid(24 * 60);
    let day_note = match day_shift {
        0 => String::new(),
        1 => " (+1 day)".to_string(),
        -1 => " (-1 day)".to_string(),
        n if n > 0 => format!(" (+{n} days)"),
        n => format!(" ({n} days)"),
    };

    Some(EvalResult {
        expression: format!("{} {}", format_clock(source), from_token.to_uppercase()),
        value: format!(
            "{}{day_note} {}",
            format_clock(target),
            to_token.to_uppercase()
        ),
    })
}

fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

const WEEKDAYS: [&str; 7] = [
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

fn format_date(days_since_epoch: i64) -> String {
    let (y, m, d) = civil_from_days(days_since_epoch);
    let weekday = WEEKDAYS[days_since_epoch.rem_euclid(7) as usize];
    let month = MONTHS[(m - 1) as usize];
    format!("{weekday}, {month} {d}, {y}")
}

#[derive(Default)]
struct DurationParts {
    months: i64,
    seconds: f64,
}

fn parse_duration(input: &str) -> Option<DurationParts> {
    let mut parts = DurationParts::default();
    let mut tokens = input.split_whitespace();
    let mut found = false;
    while let Some(amount) = tokens.next() {
        let amount: f64 = amount.parse().ok()?;
        let unit = tokens.next()?.trim_matches(|c: char| c == ',' || c == '.');
        match unit {
            "year" | "years" | "yr" | "yrs" => parts.months += (amount * 12.0).round() as i64,
            "month" | "months" | "mo" => parts.months += amount.round() as i64,
            "week" | "weeks" | "wk" | "wks" => parts.seconds += amount * 7.0 * 86_400.0,
            "day" | "days" | "d" => parts.seconds += amount * 86_400.0,
            "hour" | "hours" | "hr" | "hrs" | "h" => parts.seconds += amount * 3_600.0,
            "minute" | "minutes" | "min" | "mins" => parts.seconds += amount * 60.0,
            "second" | "seconds" | "sec" | "secs" | "s" => parts.seconds += amount,
            _ => return None,
        }
        found = true;
    }
    found.then_some(parts)
}

fn days_in_month(year: i64, month: u32) -> u32 {
    let next = if month == 12 {
        days_from_civil(year + 1, 1, 1)
    } else {
        days_from_civil(year, month + 1, 1)
    };
    (next - days_from_civil(year, month, 1)) as u32
}

fn add_months(year: i64, month: u32, day: u32, months: i64) -> (i64, u32, u32) {
    let month_index = year * 12 + i64::from(month) - 1 + months;
    let target_year = month_index.div_euclid(12);
    let target_month = month_index.rem_euclid(12) as u32 + 1;
    let target_day = day.min(days_in_month(target_year, target_month));
    (target_year, target_month, target_day)
}

/// Natural offsets anchored to the current local wall clock.
fn parse_date_arithmetic(input: &str) -> Option<EvalResult> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();
    let (relative, at_time) = match lower.rsplit_once(" at ") {
        Some((relative, clock)) => (relative, Some(parse_clock(clock.trim())?)),
        None => (lower.as_str(), None),
    };
    let (duration_text, sign) = if let Some(value) = relative.strip_suffix(" from now") {
        (value, 1_i64)
    } else if let Some(value) = relative.strip_suffix(" after now") {
        (value, 1)
    } else if let Some(value) = relative.strip_suffix(" before now") {
        (value, -1)
    } else if let Some(value) = relative.strip_suffix(" ago") {
        (value, -1)
    } else if let Some(value) = relative.strip_suffix(" later") {
        (value, 1)
    } else if let Some(value) = relative.strip_prefix("in ") {
        (value, 1)
    } else {
        return None;
    };
    let mut duration = parse_duration(duration_text.trim())?;
    duration.months *= sign;
    duration.seconds *= sign as f64;

    let (year, month, day, hour, minute) = native::local_now();
    let (year, month, day) = add_months(year as i64, month, day, duration.months);
    let start = days_from_civil(year, month, day) * 86_400
        + i64::from(hour) * 3_600
        + i64::from(minute) * 60;
    let mut target = start + duration.seconds.round() as i64;
    if let Some((hour, minute)) = at_time {
        target =
            target.div_euclid(86_400) * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60;
    }
    let target_days = target.div_euclid(86_400);
    let minute_of_day = target.rem_euclid(86_400) / 60;
    let includes_time = at_time.is_some() || duration.seconds.rem_euclid(86_400.0) != 0.0;
    let value = if includes_time {
        format!(
            "{}, {}",
            format_date(target_days),
            format_clock(minute_of_day)
        )
    } else {
        format_date(target_days)
    };

    Some(EvalResult {
        expression: trimmed.to_string(),
        value,
    })
}

pub fn convert(input: &str) -> Option<EvalResult> {
    parse_clock_arithmetic(input)
        .or_else(|| parse_timezone_difference(input))
        .or_else(|| parse_timezone_conversion(input))
        .or_else(|| parse_date_arithmetic(input))
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
        assert_eq!(result.value, "10:00 PM");
    }

    #[test]
    fn rejects_bare_number_arithmetic() {
        // Must stay a plain calculator expression, not a clock offset.
        assert!(parse_clock_arithmetic("5 + 3").is_none());
    }

    #[test]
    fn timezone_conversion_same_day() {
        let result = convert("9am est to pst").unwrap();
        assert_eq!(result.value, "6:00 AM PST");
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
        assert_eq!(convert("ist to cst").unwrap().value, "11 hr 30 min behind");
        assert_eq!(convert("cst to ist").unwrap().value, "11 hr 30 min ahead");
        assert_eq!(convert("utc to gmt").unwrap().value, "Same UTC offset");
    }

    #[test]
    fn timezone_conversion_notes_day_shift() {
        let result = convert("11pm est to ist").unwrap();
        assert_eq!(result.value, "9:30 AM (+1 day) IST");
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
    fn minutes_from_now_uses_current_wall_clock() {
        let (year, month, day, hour, minute) = native::local_now();
        let start = days_from_civil(year as i64, month, day) * 86_400
            + i64::from(hour) * 3_600
            + i64::from(minute) * 60;
        let target = start + 15 * 60;
        let expected = format!(
            "{}, {}",
            format_date(target.div_euclid(86_400)),
            format_clock(target.rem_euclid(86_400) / 60)
        );
        assert_eq!(convert("15 mins from now").unwrap().value, expected);
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
        assert_eq!(convert("9:15am - 45 mins").unwrap().value, "8:30 AM");
    }

    #[test]
    fn civil_date_roundtrip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        let z = days_from_civil(2026, 7, 30);
        assert_eq!(civil_from_days(z), (2026, 7, 30));
    }
}
