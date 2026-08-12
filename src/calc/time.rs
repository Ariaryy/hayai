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
/// in minutes. Deliberately excludes ambiguous abbreviations shared by
/// multiple regions (e.g. bare "cst" is both US Central and China
/// Standard Time) in favor of unambiguous ones plus common city names.
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

/// "1pm + 5" / "9:30am - 2" — offsets a clock time by a number of hours.
fn parse_clock_arithmetic(input: &str) -> Option<EvalResult> {
    let input = input.trim();
    let time_end = input.find(char::is_whitespace)?;
    let (hour, minute) = parse_clock(&input[..time_end])?;
    let rest = input[time_end..].trim_start();

    let (sign, rest) = if let Some(r) = rest.strip_prefix('+') {
        (1.0, r)
    } else if let Some(r) = rest.strip_prefix('-') {
        (-1.0, r)
    } else {
        return None;
    };
    let hours: f64 = rest.trim().parse().ok()?;

    let source = hour as i64 * 60 + minute as i64;
    let target = source + (sign * hours * 60.0).round() as i64;
    Some(EvalResult {
        expression: format!(
            "{} {}{}h",
            format_clock(source),
            if sign > 0.0 { "+" } else { "-" },
            hours
        ),
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

fn unit_days(token: &str) -> Option<f64> {
    match token {
        "day" | "days" => Some(1.0),
        "week" | "weeks" => Some(7.0),
        _ => None,
    }
}

/// "5 days from now" / "2 weeks ago" — relative date arithmetic anchored to
/// the current local date (`native::local_now`).
fn parse_date_arithmetic(input: &str) -> Option<EvalResult> {
    let trimmed = input.trim();
    let lower = trimmed.to_lowercase();
    let mut parts = lower.split_whitespace();

    let count: f64 = parts.next()?.parse().ok()?;
    let mult = unit_days(parts.next()?)?;
    let remainder: Vec<&str> = parts.collect();
    let signed_days = match remainder.as_slice() {
        ["from", "now"] => count * mult,
        ["ago"] => -(count * mult),
        _ => return None,
    };

    let (year, month, day, _, _) = native::local_now();
    let today = days_from_civil(year as i64, month, day);
    let target = today + signed_days.round() as i64;

    Some(EvalResult {
        expression: trimmed.to_string(),
        value: format_date(target),
    })
}

pub fn convert(input: &str) -> Option<EvalResult> {
    parse_clock_arithmetic(input)
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
    fn civil_date_roundtrip() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        let z = days_from_civil(2026, 7, 30);
        assert_eq!(civil_from_days(z), (2026, 7, 30));
    }
}
