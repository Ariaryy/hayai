//! Calendar dates without a clock: "25 dec 2026 + 30 days",
//! "30 days after 1 jan 2026", "days until 25 december", and
//! "1 jan 2026 to 10 nov 2026". These are whole-day calendar operations, so
//! no timezone or DST rules apply.

use super::*;

type Date = (i64, u32, u32);

const UNTIL: &[&str] = &["days until ", "days till "];
const SINCE: &str = "days since ";
const BETWEEN: &str = "days between ";

fn day_number(token: &str) -> Option<u32> {
    let digits = ["st", "nd", "rd", "th"]
        .iter()
        .find_map(|suffix| token.strip_suffix(suffix))
        .unwrap_or(token);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    digits.parse().ok()
}

/// A four-digit year (`1900`, `2026`), or a two-digit one within 1950–2049:
/// `26` is 2026 and `98` is 1998.
fn year_number(token: &str) -> Option<i64> {
    if !matches!(token.len(), 2 | 4) || !token.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let year: i64 = token.parse().ok()?;
    Some(match (token.len(), year) {
        (2, 0..=49) => 2000 + year,
        (2, _) => 1900 + year,
        _ => year,
    })
}

fn numeric_field(token: &str) -> Option<u32> {
    if !(1..=2).contains(&token.len()) || !token.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    token.parse().ok()
}

/// `25-12-26`, `25/12/2026`, `25.12.26`, or ISO `2026-12-25`. A leading
/// four-digit year is ISO order; otherwise the day comes first, regardless
/// of the Windows region. Month-first is used only when day-first is not a
/// real date (`12/25/26` is December 25).
fn numeric_date(token: &str) -> Option<(u32, u32, Option<i64>)> {
    let separator = token.chars().find(|c| matches!(c, '-' | '/' | '.'))?;
    let parts: Vec<&str> = token.split(separator).collect();
    let [first, second, third] = parts.as_slice() else {
        return None;
    };
    if first.len() == 4 {
        return Some((
            numeric_field(third)?,
            numeric_field(second)?,
            Some(year_number(first)?),
        ));
    }
    let (first, second) = (numeric_field(first)?, numeric_field(second)?);
    let year = year_number(third)?;
    let (day, month) = [(first, second), (second, first)]
        .into_iter()
        .find(|(day, month)| valid_date(year, *month, *day))?;
    Some((day, month, Some(year)))
}

/// "25 dec", "25th December 2026", "dec 25 26", "December 25, 2026",
/// "25/12/26", or "2026-12-25" as `(day, month, year)`. The calendar is
/// validated later, because a date without a year ("29 feb") depends on
/// which year is meant.
fn date_parts(input: &str) -> Option<(u32, u32, Option<i64>)> {
    let tokens: Vec<&str> = input
        .split_whitespace()
        .map(|token| token.trim_end_matches([',', '.']))
        .collect();
    match tokens.as_slice() {
        [numeric] => numeric_date(numeric),
        [first, second] | [first, second, _] => {
            let (day, month) = match day_number(first) {
                Some(day) => (day, month_number(second)?),
                None => (day_number(second)?, month_number(first)?),
            };
            let year = match tokens.get(2) {
                Some(year) => Some(year_number(year)?),
                None => None,
            };
            Some((day, month, year))
        }
        _ => None,
    }
}

/// A valid date; an unwritten year means `current_year`.
pub(super) fn parse_date(input: &str, current_year: i64) -> Option<Date> {
    let (day, month, year) = date_parts(input.trim())?;
    let year = year.unwrap_or(current_year);
    valid_date(year, month, day).then_some((year, month, day))
}

fn days(date: Date) -> i64 {
    days_from_civil(date.0, date.1, date.2)
}

fn short_date((year, month, day): Date) -> String {
    format!("{} {day}, {year}", MONTHS[month as usize - 1])
}

/// "25 Dec 2026": how a typed date is echoed, whatever form it was typed in.
fn compact_date((year, month, day): Date) -> String {
    format!("{day} {} {year}", &MONTHS[month as usize - 1][..3])
}

fn count_days(days: i64) -> String {
    if days.abs() == 1 {
        format!("{days} day")
    } else {
        format!("{days} days")
    }
}

/// A recognized date request, even a malformed one ("days until 31 feb"),
/// reports a date error instead of falling through to units/arithmetic.
/// An all-numeric date must be valid to be claimed, so `45-12-26` stays
/// arithmetic.
pub(super) fn is_date_query(lower: &str) -> bool {
    let starts_with_date = |rest: &str| {
        rest.split_whitespace().next().is_some_and(|word| {
            word.starts_with(|c: char| c.is_ascii_digit()) || month_number(word).is_some()
        })
    };
    if let Some(rest) = UNTIL
        .iter()
        .chain(&[SINCE, BETWEEN, "from "])
        .find_map(|prefix| lower.strip_prefix(prefix))
    {
        return starts_with_date(rest);
    }
    let mut words = lower.split_whitespace();
    let first = words.next().unwrap_or_default();
    let numeric = parse_date(first, 2000).is_some();
    let second = words.next().unwrap_or_default();
    let day_month = (day_number(first).is_some() && month_number(second).is_some())
        || (month_number(first).is_some() && day_number(second).is_some());
    numeric
        || day_month
        || (first.starts_with(|c: char| c.is_ascii_digit())
            && [" after ", " before ", " from "].iter().any(|keyword| {
                lower
                    .split_once(keyword)
                    .is_some_and(|(_, date)| starts_with_date(date))
            }))
}

pub(super) fn parse_date_math(input: &str) -> Option<EvalResult> {
    let (year, month, day, _, _) = native::local_now();
    parse_date_math_at(input, (i64::from(year), month, day))
}

fn parse_date_math_at(input: &str, today: Date) -> Option<EvalResult> {
    let input = input.trim();
    let lower = input.to_ascii_lowercase();
    if let Some(result) = countdown(&lower, today) {
        return Some(result);
    }
    let date = |text: &str| parse_date(text, today.0);

    // A date on its own names its weekday; this also keeps a partly typed
    // "25 dec 2026 + 3 days" from showing an error.
    if let Some(date) = date(&lower) {
        return Some(EvalResult {
            expression: compact_date(date),
            value: format_date(days(date)),
            copy_value: None,
        });
    }

    // Two dates: "days between X and Y", "from X to Y", "X to Y", "X - Y".
    // Either end may be today ("15 aug 2000 to now").
    let range_date = |text: &str| match text.trim() {
        "today" | "now" => Some(today),
        text => date(text),
    };
    let range = match lower.strip_prefix(BETWEEN) {
        Some(rest) => rest.split_once(" and "),
        None => {
            let rest = lower.strip_prefix("from ").unwrap_or(&lower);
            rest.split_once(" to ")
                .or_else(|| rest.split_once(" until "))
                .or_else(|| rest.split_once(" - "))
        }
    };
    if let Some((start, end)) = range
        && let (Some(start), Some(end)) = (range_date(start), range_date(end))
    {
        let difference = (days(end) - days(start)).abs();
        return Some(EvalResult {
            expression: format!("{} to {}", short_date(start), short_date(end)),
            value: count_days(difference),
            copy_value: Some(difference.to_string()),
        });
    }

    // A date moved by a calendar duration: "2 weeks before X", "X + 30 days".
    // The echo spells the date out ("1 Jan 2000 + 30 days") so numeric
    // dates are not displayed as subtraction or division.
    let worded = [(" after ", 1), (" from ", 1), (" before ", -1)]
        .iter()
        .find_map(|(keyword, sign)| {
            let (duration, start) = lower.split_once(keyword)?;
            let start = date(start)?;
            let echo = format!(
                "{} {} {}",
                duration.trim(),
                keyword.trim(),
                compact_date(start)
            );
            Some((start, duration, *sign, echo))
        });
    // Try each sign in turn: numeric dates contain '-' themselves.
    let (start, duration, sign, echo) = worded.or_else(|| {
        lower
            .char_indices()
            .filter(|(_, c)| matches!(c, '+' | '-'))
            .find_map(|(index, c)| {
                let start = date(&lower[..index])?;
                let duration = &lower[index + 1..];
                let echo = format!("{} {c} {}", compact_date(start), duration.trim());
                Some((start, duration, if c == '+' { 1 } else { -1 }, echo))
            })
    })?;
    let duration = parse_duration(duration.trim())?;
    if duration.has_clock {
        return None;
    }
    let moved = add_months(
        start.0,
        start.1,
        start.2,
        duration.months.checked_mul(sign)?,
    )?;
    let target = days(moved).checked_add(duration.days.checked_mul(sign)?)?;
    let (year, month, day) = civil_from_days(target);
    if !valid_date(year, month, day) {
        return None;
    }
    Some(EvalResult {
        expression: echo,
        value: format_date(target),
        copy_value: None,
    })
}

/// "days until 25 december" counts to the next occurrence when no year is
/// written; "days since 1 jan" counts from the most recent one. Feb 29
/// without a year means the nearest leap year, never a shifted date.
fn countdown(lower: &str, today: Date) -> Option<EvalResult> {
    let (rest, future) = match UNTIL.iter().find_map(|prefix| lower.strip_prefix(prefix)) {
        Some(rest) => (rest, true),
        None => (lower.strip_prefix(SINCE)?, false),
    };
    let (day, month, year) = date_parts(rest.trim())?;
    let today_days = days(today);
    let target = match year {
        Some(year) => valid_date(year, month, day).then_some((year, month, day))?,
        None => {
            let step = if future { 1 } else { -1 };
            // Leap years are at most eight years apart.
            (0..=8).find_map(|offset| {
                let date = (today.0 + offset * step, month, day);
                let reached = if future {
                    days(date) >= today_days
                } else {
                    days(date) <= today_days
                };
                (valid_date(date.0, date.1, date.2) && reached).then_some(date)
            })?
        }
    };
    let difference = if future {
        days(target) - today_days
    } else {
        today_days - days(target)
    };
    let value = match difference {
        0 => "Today".to_string(),
        n if n < 0 && future => format!("{} ago", count_days(-n)),
        n if n < 0 => format!("In {}", count_days(-n)),
        n => count_days(n),
    };
    Some(EvalResult {
        expression: format_date(days(target)),
        value,
        copy_value: Some(difference.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const TODAY: Date = (2026, 11, 10);

    fn value(input: &str) -> String {
        parse_date_math_at(input, TODAY)
            .unwrap_or_else(|| panic!("{input}"))
            .value
    }

    #[test]
    fn parses_written_dates() {
        for input in [
            "25 dec 2026",
            "25th December 2026",
            "dec 25 2026",
            "December 25, 2026",
            "2026-12-25",
            "25 dec 26",
            "dec 25, 26",
            "25-12-26",
            "25/12/26",
            "25.12.2026",
            "25/12/2026",
        ] {
            assert_eq!(parse_date(input, 2000), Some((2026, 12, 25)), "{input}");
        }
        assert_eq!(parse_date("25 dec", 2026), Some((2026, 12, 25)));
        assert_eq!(parse_date("15 aug 98", 2026), Some((1998, 8, 15)));
        assert_eq!(parse_date("1/1/49", 2026), Some((2049, 1, 1)));
        assert_eq!(parse_date("1/1/50", 2026), Some((1950, 1, 1)));
        // Numeric dates are always day first; month first only when day
        // first is impossible.
        assert_eq!(parse_date("05/06/26", 2026), Some((2026, 6, 5)));
        assert_eq!(parse_date("12/25/26", 2026), Some((2026, 12, 25)));
        // Full four-digit years work in every form, including the 1900s.
        for (input, expected) in [
            ("25 dec 1900", (1900, 12, 25)),
            ("15 aug 1947", (1947, 8, 15)),
            ("15/08/1947", (1947, 8, 15)),
            ("15-08-1947", (1947, 8, 15)),
            ("15.8.1947", (1947, 8, 15)),
            ("1947-08-15", (1947, 8, 15)),
            ("aug 15, 1947", (1947, 8, 15)),
            ("1/1/2099", (2099, 1, 1)),
            ("1 jan 1066", (1066, 1, 1)),
        ] {
            assert_eq!(parse_date(input, 2026), Some(expected), "{input}");
        }
        for input in [
            "31/31/26",
            "31 feb 2026",
            "29 feb",
            "25 decimal",
            "2026-13-01",
            "25 dec 202",
            "25-12",
            "25-12/26",
            "10/2/5",
            "dec",
        ] {
            assert!(parse_date(input, 2026).is_none(), "{input}");
        }
    }

    #[test]
    fn counts_days_until_and_since() {
        assert_eq!(value("days until 25 december"), "45 days");
        assert_eq!(value("days until 25-12-26"), "45 days");
        assert_eq!(value("days until 25/12/26"), "45 days");
        assert_eq!(value("days till 11 nov"), "1 day");
        assert_eq!(value("days until 10 nov"), "Today");
        // A passed date without a year means next year's occurrence.
        assert_eq!(value("days until 1 jan"), "52 days");
        assert_eq!(value("days until 1 jan 2026"), "313 days ago");
        assert_eq!(value("days since 1 jan 2026"), "313 days");
        assert_eq!(value("days since 1/1/26"), "313 days");
        assert_eq!(value("days since 25 dec"), "320 days");
        assert_eq!(value("days since 25 dec 2026"), "In 45 days");
        let leap = parse_date_math_at("days until 29 feb", TODAY).unwrap();
        assert_eq!(leap.expression, "Tuesday, February 29, 2028");
        let christmas = parse_date_math_at("days until 25 dec", TODAY).unwrap();
        assert_eq!(christmas.expression, "Friday, December 25, 2026");
        assert_eq!(christmas.copy_value.as_deref(), Some("45"));
        assert!(parse_date_math_at("days until 31 feb", TODAY).is_none());
    }

    #[test]
    fn measures_days_between_dates() {
        for input in [
            "1 jan 2026 to 10 nov 2026",
            "from 1 jan 2026 to 10 nov 2026",
            "days between 1 jan 2026 and 10 nov 2026",
            "10 nov 2026 - 1 jan 2026",
            "2026-11-10 - 2026-01-01",
            "1/1/26 to 10/11/26",
        ] {
            assert_eq!(value(input), "313 days", "{input}");
        }
        let result = parse_date_math_at("1 jan 2026 to 10 nov 2026", TODAY).unwrap();
        assert_eq!(result.expression, "January 1, 2026 to November 10, 2026");
        assert_eq!(result.copy_value.as_deref(), Some("313"));
        assert_eq!(value("1 jan 2026 to today"), "313 days");
        assert_eq!(value("from 1 jan 2026 to now"), "313 days");
    }

    #[test]
    fn a_date_alone_names_its_weekday() {
        for input in [
            "25 dec 2026",
            "25 dec 26",
            "25-12-26",
            "25/12/26",
            "2026-12-25",
        ] {
            let result = parse_date_math_at(input, TODAY).unwrap();
            assert_eq!(result.value, "Friday, December 25, 2026", "{input}");
            assert_eq!(result.expression, "25 Dec 2026", "{input}");
        }
        assert_eq!(value("25 dec 1900"), "Tuesday, December 25, 1900");
        assert_eq!(value("15/08/1947"), "Friday, August 15, 1947");
        assert_eq!(value("15 aug 47"), "Thursday, August 15, 2047");
        assert_eq!(value("15 aug 1947 + 1 day"), "Saturday, August 16, 1947");
        assert_eq!(value("days since 15/08/1947"), "28942 days");
        assert_eq!(value("1/1/1900 to 1/1/2000"), "36524 days");
    }

    #[test]
    fn moves_a_date_by_calendar_durations() {
        assert_eq!(value("1 jan 2000 + 30 days"), "Monday, January 31, 2000");
        assert_eq!(value("1 jan 2000+30 days"), "Monday, January 31, 2000");
        assert_eq!(value("1 mar 2026 - 1 day"), "Saturday, February 28, 2026");
        assert_eq!(value("2026-01-31 + 1 month"), "Saturday, February 28, 2026");
        assert_eq!(value("25-12-26 + 7 days"), "Friday, January 1, 2027");
        assert_eq!(value("25/12/26 - 1 week"), "Friday, December 18, 2026");
        assert_eq!(
            value("30 days after 1 jan 2026"),
            "Saturday, January 31, 2026"
        );
        assert_eq!(
            value("2 weeks before 25 dec 2026"),
            "Friday, December 11, 2026"
        );
        assert_eq!(
            value("1 year from 29 feb 2024"),
            "Friday, February 28, 2025"
        );
        // The echo spells out the date, so it is not shown as arithmetic.
        let moved = parse_date_math_at("25-12-26 + 7 days", TODAY).unwrap();
        assert_eq!(moved.expression, "25 Dec 2026 + 7 days");
        let worded = parse_date_math_at("30 days after 1 jan 2026", TODAY).unwrap();
        assert_eq!(worded.expression, "30 days after 1 Jan 2026");
        for input in [
            "31 feb 2026 + 3 days",
            "1 jan 2026 + 3 hours",
            "1 jan 2026 + 1e300 days",
            "1 jan 9999 + 1 year",
        ] {
            assert!(parse_date_math_at(input, TODAY).is_none(), "{input}");
        }
    }

    #[test]
    fn recognizes_only_date_shaped_requests() {
        for input in [
            "days until 25 december",
            "days until 31 feb",
            "days since jan 1",
            "1 jan 2026 to 10 nov 2026",
            "2026-01-01 + 3 days",
            "30 days after 1 jan 2026",
            "from 1 jan to 5 feb",
            "25-12-26",
            "25/12/26 + 3 days",
            "dec 25 to dec 31",
            "12/25/26",
        ] {
            assert!(is_date_query(input), "{input}");
        }
        // Numeric text that is not a valid date stays arithmetic.
        for input in [
            "days gone",
            "days until dawn",
            "5 days from now",
            "1 min to s",
            "10 km to miles",
            "255 to oct",
            "2026",
            "45-12-26",
            "10/2/5",
            "100/4/20",
        ] {
            assert!(!is_date_query(input), "{input}");
        }
    }
}
