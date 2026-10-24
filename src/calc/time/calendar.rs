use super::*;

pub(super) fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m as i64 + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d as i64 - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

pub(super) fn civil_from_days(z: i64) -> (i64, u32, u32) {
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
pub(super) const MONTHS: [&str; 12] = [
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

pub(super) fn format_date(days_since_epoch: i64) -> String {
    let (y, m, d) = civil_from_days(days_since_epoch);
    let weekday = WEEKDAYS[days_since_epoch.rem_euclid(7) as usize];
    let month = MONTHS[(m - 1) as usize];
    format!("{weekday}, {month} {d}, {y}")
}

#[derive(Default)]
pub(super) struct DurationParts {
    pub(super) months: i64,
    pub(super) days: i64,
    pub(super) seconds: f64,
    pub(super) has_clock: bool,
}

pub(super) fn parse_duration(input: &str) -> Option<DurationParts> {
    let mut parts = DurationParts::default();
    let mut rest = input.trim();
    let mut found = false;
    while !rest.is_empty() {
        let (amount, consumed) = grammar::parse_amount(rest)?;
        // Thousands shorthand belongs to conversions, never time arithmetic.
        if rest[..consumed].ends_with(['k', 'K']) {
            return None;
        }
        rest = rest[consumed..].trim_start();
        let end = rest
            .find(|c: char| !c.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        let unit = rest[..end].to_ascii_lowercase();
        if !amount.is_finite() || amount.abs() > MAX_SECONDS {
            return None;
        }
        match unit.as_str() {
            "year" | "years" | "yr" | "yrs" | "month" | "months" | "mo" => {
                if amount.fract() != 0.0 {
                    return None;
                }
                let months = if unit.starts_with('y') {
                    amount * 12.0
                } else {
                    amount
                };
                if months.abs() > 120_000.0 {
                    return None;
                }
                parts.months = parts.months.checked_add(months as i64)?;
                if parts.months.abs() > 120_000 {
                    return None;
                }
            }
            "week" | "weeks" | "wk" | "wks" | "day" | "days" | "d" => {
                let days = if unit.starts_with('w') {
                    amount * 7.0
                } else {
                    amount
                };
                bounded_seconds(days * 86_400.0)?;
                parts.days = parts.days.checked_add(days.trunc() as i64)?;
                bounded_seconds(parts.days as f64 * 86_400.0)?;
                parts.has_clock |= days.fract() != 0.0;
                parts.seconds += days.fract() * 86_400.0;
                bounded_seconds(parts.seconds)?;
            }
            _ => {
                parts.has_clock = true;
                let factor = match unit.as_str() {
                    "hour" | "hours" | "hr" | "hrs" | "h" => 3600.0,
                    "minute" | "minutes" | "min" | "mins" => 60.0,
                    "second" | "seconds" | "sec" | "secs" | "s" => 1.0,
                    _ => return None,
                };
                parts.seconds += amount * factor;
                bounded_seconds(parts.seconds)?;
            }
        }
        rest =
            rest[end..].trim_start_matches(|c: char| c.is_whitespace() || matches!(c, ',' | '.'));
        found = true;
    }
    found.then_some(parts)
}

pub(super) fn days_in_month(year: i64, month: u32) -> u32 {
    let next = if month == 12 {
        days_from_civil(year + 1, 1, 1)
    } else {
        days_from_civil(year, month + 1, 1)
    };
    (next - days_from_civil(year, month, 1)) as u32
}

fn add_months(year: i64, month: u32, day: u32, months: i64) -> Option<(i64, u32, u32)> {
    if !valid_date(year, month, day) {
        return None;
    }
    let month_index = year
        .checked_mul(12)?
        .checked_add(i64::from(month) - 1)?
        .checked_add(months)?;
    let target_year = month_index.div_euclid(12);
    let target_month = month_index.rem_euclid(12) as u32 + 1;
    let target_day = day.min(days_in_month(target_year, target_month));
    valid_date(target_year, target_month, target_day).then_some((
        target_year,
        target_month,
        target_day,
    ))
}

/// Natural offsets anchored to the current local wall clock.
pub(super) fn parse_date_arithmetic(input: &str) -> Option<EvalResult> {
    let zone = native::system_timezone_name();
    parse_date_arithmetic_with_zone(input, native::local_now_seconds(), zone.as_deref())
}

// Inject the wall clock for deterministic calendar/second boundary tests.
#[cfg(test)]
pub(super) fn parse_date_arithmetic_at(
    input: &str,
    now: (i32, u32, u32, u32, u32, u32),
) -> Option<EvalResult> {
    parse_date_arithmetic_with_zone(input, now, None)
}

pub(super) fn parse_date_arithmetic_with_zone(
    input: &str,
    now: (i32, u32, u32, u32, u32, u32),
    zone: Option<&str>,
) -> Option<EvalResult> {
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
    duration.months = duration.months.checked_mul(sign)?;
    duration.days = duration.days.checked_mul(sign)?;
    duration.seconds *= sign as f64;

    let (year, month, day, hour, minute, second) = now;
    let (year, month, day) = add_months(year as i64, month, day, duration.months)?;
    // Months and whole days are calendar operations. Sub-day durations are
    // elapsed time: cross DST in UTC, never manufacture a nonexistent 2:30am.
    let calendar_days = days_from_civil(year, month, day).checked_add(duration.days)?;
    let (year, month, day) = civil_from_days(calendar_days);
    if !valid_date(year, month, day) {
        return None;
    }
    let start = calendar_days * 86_400
        + i64::from(hour) * 3600
        + i64::from(minute) * 60
        + i64::from(second);
    let offset = bounded_seconds(duration.seconds)?;
    let mut target = if offset != 0 {
        if let Some(zone) = zone {
            let utc = native::timezone_local_to_utc(zone, (year as i32, month, day, hour, minute))?;
            let utc_seconds =
                (date_time_minutes(utc) * 60 + i64::from(second)).checked_add(offset)?;
            let local =
                native::timezone_utc_to_local(zone, minutes_date_time(utc_seconds.div_euclid(60)))?;
            date_time_minutes(local) * 60 + utc_seconds.rem_euclid(60)
        } else {
            start.checked_add(offset)?
        }
    } else {
        start
    };
    if let Some((hour, minute)) = at_time {
        target =
            target.div_euclid(86_400) * 86_400 + i64::from(hour) * 3_600 + i64::from(minute) * 60;
    }
    let target_days = target.div_euclid(86_400);
    let date = civil_from_days(target_days);
    if !valid_date(date.0, date.1, date.2) {
        return None;
    }
    if let Some(zone) = zone {
        native::timezone_local_to_utc(zone, minutes_date_time(target.div_euclid(60)))?;
    }
    let includes_time = at_time.is_some() || duration.has_clock;
    let value = if includes_time {
        format!(
            "{}, {}",
            format_date(target_days),
            format_clock_seconds(target.rem_euclid(86_400))
        )
    } else {
        format_date(target_days)
    };

    Some(EvalResult {
        copy_value: None,
        expression: trimmed.to_string(),
        value,
    })
}
