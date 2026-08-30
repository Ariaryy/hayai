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

/// Explicit timezone abbreviations mapped to fixed UTC offsets in minutes.
/// City aliases are deliberately separate below because Windows applies
/// daylight-saving rules to them for the requested date.
const TIMEZONES: &[(&[&str], i32)] = &[
    (&["utc", "gmt"], 0),
    (&["bst"], 60),
    (&["cet"], 60),
    (&["cest"], 120),
    (&["eet"], 120),
    (&["msk"], 180),
    (&["est"], -300),
    (&["edt"], -240),
    (&["cst"], -360),
    (&["cdt"], -300),
    (&["mst"], -420),
    (&["mdt"], -360),
    (&["pst"], -480),
    (&["pdt"], -420),
    (&["gst"], 240),
    (&["ist"], 330),
    (&["sgt"], 480),
    (&["cst_asia"], 480),
    (&["jst"], 540),
    (&["kst"], 540),
    (&["aest"], 600),
    (&["aedt"], 660),
    (&["nzst"], 720),
];

struct CityZone {
    aliases: &'static [&'static str],
    windows_key: &'static str,
    identity: &'static str,
    standard_offset: i32,
    standard_abbreviation: &'static str,
    daylight_abbreviation: &'static str,
}

const CITY_ZONES: &[CityZone] = &[
    CityZone {
        aliases: &["london"],
        windows_key: "GMT Standard Time",
        identity: "Europe/London",
        standard_offset: 0,
        standard_abbreviation: "GMT",
        daylight_abbreviation: "BST",
    },
    CityZone {
        aliases: &["paris", "madrid"],
        windows_key: "Romance Standard Time",
        identity: "Europe/Paris",
        standard_offset: 60,
        standard_abbreviation: "CET",
        daylight_abbreviation: "CEST",
    },
    CityZone {
        aliases: &["berlin", "rome", "amsterdam"],
        windows_key: "W. Europe Standard Time",
        identity: "Europe/Berlin",
        standard_offset: 60,
        standard_abbreviation: "CET",
        daylight_abbreviation: "CEST",
    },
    CityZone {
        aliases: &["athens"],
        windows_key: "GTB Standard Time",
        identity: "Europe/Athens",
        standard_offset: 120,
        standard_abbreviation: "EET",
        daylight_abbreviation: "EEST",
    },
    CityZone {
        aliases: &["cairo"],
        windows_key: "Egypt Standard Time",
        identity: "Africa/Cairo",
        standard_offset: 120,
        standard_abbreviation: "EET",
        daylight_abbreviation: "EEST",
    },
    CityZone {
        aliases: &["moscow"],
        windows_key: "Russian Standard Time",
        identity: "Europe/Moscow",
        standard_offset: 180,
        standard_abbreviation: "MSK",
        daylight_abbreviation: "MSK",
    },
    CityZone {
        aliases: &["newyork", "nyc", "toronto"],
        windows_key: "Eastern Standard Time",
        identity: "America/New_York",
        standard_offset: -300,
        standard_abbreviation: "EST",
        daylight_abbreviation: "EDT",
    },
    CityZone {
        aliases: &["chicago"],
        windows_key: "Central Standard Time",
        identity: "America/Chicago",
        standard_offset: -360,
        standard_abbreviation: "CST",
        daylight_abbreviation: "CDT",
    },
    CityZone {
        aliases: &["denver"],
        windows_key: "Mountain Standard Time",
        identity: "America/Denver",
        standard_offset: -420,
        standard_abbreviation: "MST",
        daylight_abbreviation: "MDT",
    },
    CityZone {
        aliases: &["losangeles", "la", "sf", "sanfrancisco", "seattle"],
        windows_key: "Pacific Standard Time",
        identity: "America/Los_Angeles",
        standard_offset: -480,
        standard_abbreviation: "PST",
        daylight_abbreviation: "PDT",
    },
    CityZone {
        aliases: &["dubai"],
        windows_key: "Arabian Standard Time",
        identity: "Asia/Dubai",
        standard_offset: 240,
        standard_abbreviation: "GST",
        daylight_abbreviation: "GST",
    },
    CityZone {
        aliases: &["mumbai", "delhi", "bangalore", "kolkata", "india"],
        windows_key: "India Standard Time",
        identity: "Asia/Kolkata",
        standard_offset: 330,
        standard_abbreviation: "IST",
        daylight_abbreviation: "IST",
    },
    CityZone {
        aliases: &["singapore"],
        windows_key: "Singapore Standard Time",
        identity: "Asia/Singapore",
        standard_offset: 480,
        standard_abbreviation: "SGT",
        daylight_abbreviation: "SGT",
    },
    CityZone {
        aliases: &["hongkong"],
        windows_key: "China Standard Time",
        identity: "Asia/Hong_Kong",
        standard_offset: 480,
        standard_abbreviation: "HKT",
        daylight_abbreviation: "HKT",
    },
    CityZone {
        aliases: &["beijing", "shanghai", "china"],
        windows_key: "China Standard Time",
        identity: "Asia/Shanghai",
        standard_offset: 480,
        standard_abbreviation: "CST",
        daylight_abbreviation: "CST",
    },
    CityZone {
        aliases: &["tokyo", "shibuya"],
        windows_key: "Tokyo Standard Time",
        identity: "Asia/Tokyo",
        standard_offset: 540,
        standard_abbreviation: "JST",
        daylight_abbreviation: "JST",
    },
    CityZone {
        aliases: &["seoul"],
        windows_key: "Korea Standard Time",
        identity: "Asia/Seoul",
        standard_offset: 540,
        standard_abbreviation: "KST",
        daylight_abbreviation: "KST",
    },
    CityZone {
        aliases: &["sydney", "melbourne"],
        windows_key: "AUS Eastern Standard Time",
        identity: "Australia/Sydney",
        standard_offset: 600,
        standard_abbreviation: "AEST",
        daylight_abbreviation: "AEDT",
    },
    CityZone {
        aliases: &["auckland"],
        windows_key: "New Zealand Standard Time",
        identity: "Pacific/Auckland",
        standard_offset: 720,
        standard_abbreviation: "NZST",
        daylight_abbreviation: "NZDT",
    },
];

fn find_offset(token: &str) -> Option<i32> {
    if token == SYSTEM_TIMEZONE {
        return current_system_offset_minutes();
    }
    let token = token.to_lowercase();
    TIMEZONES
        .iter()
        .find(|(names, _)| names.contains(&token.as_str()))
        .map(|(_, offset)| *offset)
}

#[derive(Clone)]
enum ZoneKind {
    Fixed(i32),
    Windows(String),
}

#[derive(Clone)]
struct ResolvedZone {
    kind: ZoneKind,
    identity: String,
    standard_offset: i32,
    standard_abbreviation: String,
    daylight_abbreviation: String,
}

impl ResolvedZone {
    fn abbreviation(&self, offset: i32) -> &str {
        if offset == self.standard_offset {
            &self.standard_abbreviation
        } else {
            &self.daylight_abbreviation
        }
    }

    fn label(&self, offset: i32) -> String {
        format!(
            "{} · {} ({})",
            self.identity,
            self.abbreviation(offset),
            format_utc_offset(offset)
        )
    }
}

fn city_zone_for_key(key: &str) -> Option<&'static CityZone> {
    CITY_ZONES
        .iter()
        .find(|zone| zone.windows_key.eq_ignore_ascii_case(key))
}

fn resolve_zone(token: &str) -> Option<ResolvedZone> {
    let lower = token.to_ascii_lowercase();
    if lower == SYSTEM_TIMEZONE {
        let key = native::system_timezone_name()?;
        let known = city_zone_for_key(&key);
        return Some(ResolvedZone {
            kind: ZoneKind::Windows(key.clone()),
            identity: key,
            standard_offset: known.map_or(current_system_offset_minutes()?, |zone| {
                zone.standard_offset
            }),
            standard_abbreviation: known.map_or_else(
                || "LOCAL".to_string(),
                |zone| zone.standard_abbreviation.to_string(),
            ),
            daylight_abbreviation: known.map_or_else(
                || "LOCAL".to_string(),
                |zone| zone.daylight_abbreviation.to_string(),
            ),
        });
    }
    if let Some(zone) = CITY_ZONES
        .iter()
        .find(|zone| zone.aliases.contains(&lower.as_str()))
    {
        return Some(ResolvedZone {
            kind: ZoneKind::Windows(zone.windows_key.to_string()),
            identity: zone.identity.to_string(),
            standard_offset: zone.standard_offset,
            standard_abbreviation: zone.standard_abbreviation.to_string(),
            daylight_abbreviation: zone.daylight_abbreviation.to_string(),
        });
    }
    let offset = find_offset(&lower)?;
    let abbreviation = match lower.as_str() {
        "cst_asia" => "CST".to_string(),
        _ => lower.to_ascii_uppercase(),
    };
    let identity = match lower.as_str() {
        "cst" | "cdt" => "America/Chicago",
        "est" | "edt" => "America/New_York",
        "mst" | "mdt" => "America/Denver",
        "pst" | "pdt" => "America/Los_Angeles",
        "ist" => "Asia/Kolkata",
        "jst" => "Asia/Tokyo",
        "kst" => "Asia/Seoul",
        "sgt" => "Asia/Singapore",
        "cst_asia" => "Asia/Shanghai",
        "aest" | "aedt" => "Australia/Sydney",
        "nzst" => "Pacific/Auckland",
        "utc" | "gmt" => "Etc/UTC",
        _ => abbreviation.as_str(),
    }
    .to_string();
    Some(ResolvedZone {
        kind: ZoneKind::Fixed(offset),
        identity,
        standard_offset: offset,
        standard_abbreviation: abbreviation.clone(),
        daylight_abbreviation: abbreviation,
    })
}

fn date_time_minutes(date_time: CivilDateTime) -> i64 {
    days_from_civil(date_time.0 as i64, date_time.1, date_time.2) * 24 * 60
        + date_time.3 as i64 * 60
        + date_time.4 as i64
}

fn minutes_date_time(minutes: i64) -> CivilDateTime {
    let days = minutes.div_euclid(24 * 60);
    let minute_of_day = minutes.rem_euclid(24 * 60);
    let (year, month, day) = civil_from_days(days);
    (
        year as i32,
        month,
        day,
        (minute_of_day / 60) as u32,
        (minute_of_day % 60) as u32,
    )
}

fn zone_to_utc(zone: &ResolvedZone, local: CivilDateTime) -> Option<(CivilDateTime, i32)> {
    let utc = match &zone.kind {
        ZoneKind::Fixed(offset) => minutes_date_time(date_time_minutes(local) - *offset as i64),
        ZoneKind::Windows(key) => native::timezone_local_to_utc(key, local)?,
    };
    let offset = (date_time_minutes(local) - date_time_minutes(utc)) as i32;
    Some((utc, offset))
}

fn zone_from_utc(zone: &ResolvedZone, utc: CivilDateTime) -> Option<(CivilDateTime, i32)> {
    let local = match &zone.kind {
        ZoneKind::Fixed(offset) => minutes_date_time(date_time_minutes(utc) + *offset as i64),
        ZoneKind::Windows(key) => native::timezone_utc_to_local(key, utc)?,
    };
    let offset = (date_time_minutes(local) - date_time_minutes(utc)) as i32;
    Some((local, offset))
}

fn current_system_offset_minutes() -> Option<i32> {
    let utc_minutes = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs()
        .checked_div(60)? as i64;
    let (year, month, day, hour, minute) = native::local_now();
    let local_minutes =
        days_from_civil(year as i64, month, day) * 24 * 60 + hour as i64 * 60 + minute as i64;
    i32::try_from(local_minutes - utc_minutes).ok()
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
    let first_end = input.find(char::is_whitespace).unwrap_or(input.len());
    let first = &input[..first_end];
    let rest = input[first_end..].trim_start();
    if let Some(clock) = parse_clock(first) {
        return Some((clock, rest));
    }

    let meridiem_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
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

    let ((hour, minute), rest) = parse_leading_clock(clock_input)?;
    let from_end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    if from_end == 0 {
        return None;
    }
    let from_token = rest[..from_end].to_ascii_lowercase();
    resolve_zone(&from_token)?;
    let remainder = rest[from_end..].trim_start();
    let (target, matched) = grammar::parse_keyword_then_target(remainder);
    let to_token = if matched {
        target
    } else if remainder.is_empty() {
        SYSTEM_TIMEZONE.to_string()
    } else {
        return None;
    };
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

/// Converts clock times between fixed abbreviations, DST-aware named places,
/// and the current Windows timezone, with an optional leading calendar date.
fn parse_timezone_conversion(input: &str) -> Option<EvalResult> {
    let parsed = parse_zoned_clock(input)?;
    let source_zone = resolve_zone(&parsed.from_token)?;
    let target_zone = resolve_zone(&parsed.to_token)?;
    let (utc, source_offset) = zone_to_utc(&source_zone, parsed.local)?;
    let (target, target_offset) = zone_from_utc(&target_zone, utc)?;
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
        expression: format!("{} {source_abbreviation}", format_clock(source_clock)),
        value,
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
    } else {
        (relative.strip_prefix("in ")?, 1)
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
}
