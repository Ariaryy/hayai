pub(super) fn format_expression_for_display(expression: &str) -> String {
    const SUPERSCRIPT_DIGITS: [char; 10] = ['⁰', '¹', '²', '³', '⁴', '⁵', '⁶', '⁷', '⁸', '⁹'];

    let mut superscripted = String::with_capacity(expression.len());
    let mut characters = expression.chars().peekable();
    while let Some(character) = characters.next() {
        if character != '^' {
            superscripted.push(character);
            continue;
        }

        let mut exponent = String::new();
        if characters.peek() == Some(&'-') {
            characters.next();
            exponent.push('⁻');
        }
        while let Some(digit) = characters.peek().and_then(|next| next.to_digit(10)) {
            characters.next();
            exponent.push(SUPERSCRIPT_DIGITS[digit as usize]);
        }

        if exponent.is_empty() || exponent == "⁻" {
            superscripted.push('^');
            if exponent == "⁻" {
                superscripted.push('-');
            }
        } else {
            superscripted.push_str(&exponent);
        }
    }

    let mut formatted = String::with_capacity(superscripted.len());
    let mut characters = superscripted.chars().peekable();
    while let Some(character) = characters.next() {
        if !matches!(character, '+' | '-' | '*' | '/' | '%' | '=') {
            formatted.push(character);
            continue;
        }

        let previous = formatted
            .chars()
            .rev()
            .find(|character| !character.is_whitespace());
        let next = characters
            .clone()
            .find(|character| !character.is_whitespace());
        let is_binary = previous.is_some_and(|previous| {
            !matches!(previous, '+' | '-' | '*' | '/' | '%' | '=' | '(' | '^')
        }) && next.is_some();

        if is_binary {
            while formatted.ends_with(char::is_whitespace) {
                formatted.pop();
            }
            formatted.push(' ');
            formatted.push(character);
            formatted.push(' ');
            while characters.peek().is_some_and(|next| next.is_whitespace()) {
                characters.next();
            }
        } else {
            formatted.push(character);
        }
    }
    let tokens = formatted.split_whitespace().collect::<Vec<_>>();
    tokens
        .iter()
        .enumerate()
        .map(|(index, token)| {
            let bare_token = token.trim_end_matches([',', '.']);
            let normalized_token = bare_token.to_ascii_lowercase();
            let punctuation = &token[bare_token.len()..];
            let follows_clock = index > 0 && tokens[index - 1].contains(':');
            let precedes_timezone = tokens.get(index + 1).is_some_and(|next| {
                matches!(
                    next.to_ascii_lowercase().as_str(),
                    "utc"
                        | "gmt"
                        | "est"
                        | "edt"
                        | "cst"
                        | "cdt"
                        | "mst"
                        | "mdt"
                        | "pst"
                        | "pdt"
                        | "ist"
                        | "jst"
                        | "kst"
                )
            });
            if (follows_clock || precedes_timezone)
                && matches!(normalized_token.as_str(), "am" | "pm")
            {
                return format!("{}{punctuation}", normalized_token.to_ascii_uppercase());
            }
            if let Some((clock, meridiem)) = normalized_token
                .strip_suffix("am")
                .map(|clock| (clock, "AM"))
                .or_else(|| {
                    normalized_token
                        .strip_suffix("pm")
                        .map(|clock| (clock, "PM"))
                })
                && !clock.is_empty()
                && clock
                    .chars()
                    .all(|character| character.is_ascii_digit() || character == ':')
            {
                return format!("{clock} {meridiem}{punctuation}");
            }
            if matches!(
                normalized_token.as_str(),
                "utc"
                    | "gmt"
                    | "bst"
                    | "cet"
                    | "cest"
                    | "eet"
                    | "msk"
                    | "est"
                    | "edt"
                    | "cst"
                    | "cdt"
                    | "mst"
                    | "mdt"
                    | "pst"
                    | "pdt"
                    | "gst"
                    | "ist"
                    | "sgt"
                    | "jst"
                    | "kst"
                    | "aest"
                    | "aedt"
                    | "nzst"
            ) {
                return format!("{}{punctuation}", normalized_token.to_ascii_uppercase());
            }

            let amount = index
                .checked_sub(1)
                .and_then(|previous| tokens[previous].parse::<f64>().ok());
            let canonical = match normalized_token.as_str() {
                "year" | "years" | "yr" | "yrs" => Some(("year", "years")),
                "month" | "months" | "mo" => Some(("month", "months")),
                "week" | "weeks" | "wk" | "wks" => Some(("week", "weeks")),
                "day" | "days" | "d" => Some(("day", "days")),
                "hour" | "hours" | "hr" | "hrs" | "h" => Some(("hour", "hours")),
                "minute" | "minutes" | "min" | "mins" => Some(("minute", "minutes")),
                "second" | "seconds" | "sec" | "secs" | "s" => Some(("second", "seconds")),
                _ => None,
            };
            if let (Some(amount), Some((one, many))) = (amount, canonical) {
                let singular = (amount.abs() - 1.0).abs() < f64::EPSILON;
                return format!("{}{punctuation}", if singular { one } else { many });
            }

            let adjacent_to_conversion = index
                .checked_sub(1)
                .and_then(|previous| tokens.get(previous))
                .is_some_and(|token| {
                    matches!(token.to_ascii_lowercase().as_str(), "to" | "in" | "as")
                        || token.parse::<f64>().is_ok()
                })
                || tokens.get(index + 1).is_some_and(|token| {
                    matches!(token.to_ascii_lowercase().as_str(), "to" | "in" | "as")
                });
            if normalized_token.len() == 3
                && normalized_token
                    .chars()
                    .all(|character| character.is_ascii_alphabetic())
                && adjacent_to_conversion
            {
                return format!("{}{punctuation}", normalized_token.to_ascii_uppercase());
            }

            (*token).to_string()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

pub(super) fn format_result_for_display(result: &str) -> String {
    let mut formatted = format_expression_for_display(result);
    let mut clock_positions = [" AM", " PM"]
        .into_iter()
        .flat_map(|suffix| formatted.match_indices(suffix).map(|(index, _)| index))
        .filter_map(|meridiem_space| {
            let clock = formatted[..meridiem_space].split_whitespace().next_back()?;
            let (hour, minute) = clock.split_once(':')?;
            (hour.parse::<u8>().is_ok() && minute.parse::<u8>().is_ok()).then(|| {
                let clock_start = formatted[..meridiem_space].rfind(clock).unwrap_or(0);
                (meridiem_space, clock_start)
            })
        })
        .collect::<Vec<_>>();
    clock_positions.sort_unstable_by_key(|position| std::cmp::Reverse(position.0));

    for (meridiem_space, clock_start) in clock_positions {
        if clock_start > 0 && formatted[..clock_start].trim_end().ends_with(',') {
            formatted.replace_range(clock_start - 1..clock_start, "\n");
        }
        formatted.replace_range(meridiem_space..meridiem_space + 1, "\u{00a0}");
    }
    formatted
}
