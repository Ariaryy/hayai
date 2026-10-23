//! Recognize unfinished syntax, rather than retaining an unrelated old result.
//! Only the unfinished suffix is removed; a complete invalid expression stays
//! invalid. At most one prefix is evaluated, regardless of query length.
use std::borrow::Cow;

pub(super) struct Pending<'a> {
    pub source: Cow<'a, str>,
    pub awaiting_operand: bool,
}

const WORD_OPERATORS: &[&str] = &[
    "add", "plus", "minus", "mul", "div", "mod", "modulo", "pow", "power",
];

fn trailing_word_start(input: &str) -> usize {
    input
        .char_indices()
        .rev()
        .find(|(_, c)| !c.is_ascii_alphabetic())
        .map_or(0, |(index, c)| index + c.len_utf8())
}

/// Word operators always require a right operand, even though their engine
/// spelling '%' can also be a complete postfix percentage.
pub(super) fn word_operator_source(input: &str) -> Option<&str> {
    let start = trailing_word_start(input);
    (start > 0 && WORD_OPERATORS.contains(&input[start..].to_ascii_lowercase().as_str()))
        .then(|| input[..start].trim_end())
}

pub(super) fn function(name: &str) -> bool {
    matches!(
        name,
        "sqrt"
            | "cbrt"
            | "sin"
            | "cos"
            | "tan"
            | "asin"
            | "acos"
            | "atan"
            | "sinh"
            | "cosh"
            | "tanh"
            | "log"
            | "ln"
            | "exp"
            | "abs"
            | "round"
            | "floor"
            | "ceil"
            | "sum"
            | "min"
            | "max"
            | "avg"
            | "mean"
            | "average"
    )
}

fn operator(c: char) -> bool {
    matches!(c, '+' | '-' | '*' | '/' | '^' | '=' | '<' | '>' | '&' | '|')
}

fn finish(source: &str, awaiting_operand: bool) -> Option<Pending<'_>> {
    let source = source.trim_end();
    let mut depth = 0usize;
    for c in source.chars() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.checked_sub(1)?,
            _ => {}
        }
        if depth >= 48 {
            return None;
        }
    }
    let source = if depth == 0 {
        Cow::Borrowed(source)
    } else {
        Cow::Owned(format!("{source}{}", ")".repeat(depth)))
    };
    Some(Pending {
        source,
        awaiting_operand,
    })
}

pub(super) fn source(input: &str) -> Option<Pending<'_>> {
    if input.len() > 4096 {
        return None;
    }
    let input = input.trim_end();
    let signless = input.trim_start_matches(['+', '-']);
    if matches!(input, "." | "-." | "+.")
        || (signless.chars().count() == 1
            && signless
                .chars()
                .next()
                .is_some_and(super::currency::is_symbol))
    {
        return Some(Pending {
            source: Cow::Borrowed(""),
            awaiting_operand: true,
        });
    }
    if ["0x", "0X", "0o", "0O", "0b", "0B"]
        .iter()
        .any(|prefix| input.ends_with(prefix))
    {
        return finish(&input[..input.len() - 1], false);
    }
    if let Some(source) = word_operator_source(input) {
        return finish(source, false);
    }
    if super::time::unfinished_duration(input) {
        return Some(Pending {
            source: Cow::Borrowed(input),
            awaiting_operand: true,
        });
    }

    if let Some((source, target)) = super::grammar::split_conversion(input) {
        let partial = if super::currency::contains_currency(source) {
            super::currency::target_prefix(target)
        } else if super::time::is_time_query(source) {
            super::time::target_prefix(target)
        } else {
            ["hex", "hexadecimal", "decimal", "binary", "octal"]
                .iter()
                .any(|name| name.starts_with(target) && *name != target)
                || super::units::target_prefix(target)
        };
        if partial {
            return finish(source, false);
        }
    }

    // Empty/partial conversion keywords and targets work in every domain.
    if let Some((source, tail)) = input.rsplit_once(char::is_whitespace) {
        let tail = tail.to_ascii_lowercase();
        if ["to", "in", "as"]
            .iter()
            .any(|word| word.starts_with(&tail))
            || WORD_OPERATORS.iter().any(|word| word.starts_with(&tail))
        {
            return finish(source, super::time::unfinished_duration(source.trim_end()));
        }
    }

    // Unit/function names and relative-time connectors are also entered a
    // character at a time, including glued forms such as "30mi".
    let word_start = trailing_word_start(input);
    if word_start > 0 && word_start < input.len() {
        let word = &input[word_start..];
        if super::units::target_prefix(word)
            || super::time::duration_prefix(word)
            || ["from", "after", "before", "now", "at", "of"]
                .iter()
                .any(|name| name.starts_with(word))
        {
            let source = &input[..word_start];
            return finish(source, super::time::unfinished_duration(source.trim_end()));
        }
    }
    if input.ends_with(':') && input[..input.len() - 1].ends_with(|c: char| c.is_ascii_digit()) {
        return finish(&input[..input.len() - 1], false);
    }

    // Trim only editing punctuation at the end. Completed bad operands,
    // mismatched closing parentheses, etc. are never searched past.
    let mut source = input;
    let mut removed = false;
    let mut awaiting_operand = false;
    loop {
        let previous = source;
        source = source.trim_end_matches(|c: char| c.is_whitespace() || operator(c));
        if source.ends_with('(') {
            source = source[..source.len() - 1].trim_end();
            let start = trailing_word_start(source);
            if start < source.len() {
                if !function(&source[start..]) {
                    return None;
                }
                source = source[..start].trim_end();
            }
            awaiting_operand = true;
        } else if source.ends_with(',') {
            // Commas only mean unfinished arguments inside an open call.
            let opens = source.chars().filter(|c| *c == '(').count();
            let closes = source.chars().filter(|c| *c == ')').count();
            if opens <= closes {
                return None;
            }
            source = &source[..source.len() - 1];
        }
        if source == previous {
            break;
        }
        removed = true;
    }
    // A signed but unfinished exponent is not multiplication by Euler's e.
    if removed && source.ends_with(['e', 'E']) {
        let number = &source[..source.len() - 1];
        if number.ends_with(|c: char| c.is_ascii_digit() || c == '.') {
            source = number;
        }
    }
    let pending = finish(source, awaiting_operand || source.is_empty())?;
    (removed || pending.source.as_ref() != input).then_some(pending)
}
