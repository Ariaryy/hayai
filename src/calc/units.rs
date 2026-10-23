//! Physical conversions share fend's exact unit definitions and arithmetic.
//! This module only supplies friendly aliases and presentation labels. Never
//! add numeric factors here: simple and compound expressions must agree.
use super::{EvalResult, engine, grammar};

struct Unit {
    aliases: &'static [&'static str],
    symbol: &'static str,
    label: &'static str,
}

const UNITS: &[Unit] = &[
    Unit {
        aliases: &["m", "meter", "meters", "metre", "metres"],
        symbol: "m",
        label: "Meter",
    },
    Unit {
        aliases: &["km", "kilometer", "kilometers", "kilometre", "kilometres"],
        symbol: "km",
        label: "Kilometer",
    },
    Unit {
        aliases: &[
            "cm",
            "centimeter",
            "centimeters",
            "centimetre",
            "centimetres",
        ],
        symbol: "cm",
        label: "Centimeter",
    },
    Unit {
        aliases: &[
            "mm",
            "millimeter",
            "millimeters",
            "millimetre",
            "millimetres",
        ],
        symbol: "mm",
        label: "Millimeter",
    },
    Unit {
        aliases: &["pm", "picometer", "picometers", "picometre", "picometres"],
        symbol: "pm",
        label: "Picometer",
    },
    Unit {
        aliases: &["mi", "mile", "miles"],
        symbol: "mi",
        label: "Mile",
    },
    Unit {
        aliases: &["yd", "yard", "yards"],
        symbol: "yd",
        label: "Yard",
    },
    Unit {
        aliases: &["ft", "foot", "feet"],
        symbol: "ft",
        label: "Foot",
    },
    Unit {
        aliases: &["in", "inch", "inches"],
        symbol: "inch",
        label: "Inch",
    },
    Unit {
        aliases: &["nmi", "nauticalmile", "nauticalmiles"],
        symbol: "nautical_mile",
        label: "Nautical Mile",
    },
    Unit {
        aliases: &["kg", "kilogram", "kilograms"],
        symbol: "kg",
        label: "Kilogram",
    },
    Unit {
        aliases: &["g", "gram", "grams"],
        symbol: "g",
        label: "Gram",
    },
    Unit {
        aliases: &["mg", "milligram", "milligrams"],
        symbol: "mg",
        label: "Milligram",
    },
    Unit {
        aliases: &["lb", "lbs", "pound", "pounds"],
        symbol: "lb",
        label: "Pound",
    },
    Unit {
        aliases: &["oz", "ounce", "ounces"],
        symbol: "oz",
        label: "Ounce",
    },
    Unit {
        aliases: &["tonne", "tonnes", "ton", "tons"],
        symbol: "tonne",
        label: "Metric Ton",
    },
    Unit {
        aliases: &["stone", "st"],
        symbol: "stone",
        label: "Stone",
    },
    Unit {
        aliases: &["byte", "bytes"],
        symbol: "B",
        label: "Byte",
    },
    Unit {
        aliases: &["b", "bit", "bits"],
        symbol: "bit",
        label: "Bit",
    },
    Unit {
        aliases: &["kb", "kilobyte", "kilobytes"],
        symbol: "KB",
        label: "Kilobyte",
    },
    Unit {
        aliases: &["mb", "megabyte", "megabytes"],
        symbol: "MB",
        label: "Megabyte",
    },
    Unit {
        aliases: &["gb", "gigabyte", "gigabytes"],
        symbol: "GB",
        label: "Gigabyte",
    },
    Unit {
        aliases: &["tb", "terabyte", "terabytes"],
        symbol: "TB",
        label: "Terabyte",
    },
    Unit {
        aliases: &["kib", "kibibyte", "kibibytes"],
        symbol: "KiB",
        label: "Kibibyte",
    },
    Unit {
        aliases: &["mib", "mebibyte", "mebibytes"],
        symbol: "MiB",
        label: "Mebibyte",
    },
    Unit {
        aliases: &["gib", "gibibyte", "gibibytes"],
        symbol: "GiB",
        label: "Gibibyte",
    },
    Unit {
        aliases: &["tib", "tebibyte", "tebibytes"],
        symbol: "TiB",
        label: "Tebibyte",
    },
    Unit {
        aliases: &["m/s", "meter/second", "meters/second"],
        symbol: "m/s",
        label: "Meter per Second",
    },
    Unit {
        aliases: &["km/s", "kilometer/second", "kilometers/second"],
        symbol: "km/s",
        label: "Kilometer per Second",
    },
    Unit {
        aliases: &["c", "lightspeed", "speedoflight"],
        symbol: "c",
        label: "Speed of Light",
    },
];

fn canonical(token: &str) -> String {
    // A mixed-case symbol is intentional (Mb differs from MB). Only known
    // lowercase legacy spellings and full names are aliases.
    UNITS
        .iter()
        .find(|unit| unit.aliases.contains(&token))
        .map_or_else(|| token.to_string(), |unit| unit.symbol.to_string())
}

pub(super) fn target_prefix(target: &str) -> bool {
    if target.is_empty() {
        return false;
    }
    if target.len() > 1
        && (UNITS
            .iter()
            .any(|unit| unit.aliases.contains(&target) || unit.symbol == target)
            || temperature(target).is_some())
    {
        return false;
    }
    UNITS.iter().any(|unit| {
        unit.aliases
            .iter()
            .chain(std::iter::once(&unit.symbol))
            .any(|name| name.starts_with(target) && *name != target)
    }) || !fend_core::get_completions_for_prefix(target).1.is_empty()
}

pub(super) fn canonical_word(token: &str) -> Option<&'static str> {
    if matches!(token, "in" | "b") {
        return None;
    } // Fend already handles inches/bits; rewriting b would corrupt 0b literals.
    UNITS
        .iter()
        .find(|unit| unit.aliases.contains(&token) && unit.symbol != token)
        .map(|unit| unit.symbol)
}

fn temperature(token: &str) -> Option<(&'static str, &'static str)> {
    match token.to_ascii_lowercase().as_str() {
        "c" | "°c" | "celsius" => Some(("C", "Celsius")),
        "f" | "°f" | "fahrenheit" => Some(("F", "Fahrenheit")),
        "k" | "kelvin" => Some(("K", "Kelvin")),
        _ => None,
    }
}

/// A temperature target supplies the context for ambiguous c/f/k symbols,
/// including inside parentheses. Apply this before the shared engine parser.
pub(super) fn temperature_expression(input: &str) -> Option<String> {
    let (source, target) = grammar::split_conversion(input)?;
    let target = temperature(target)?.0;
    let mut output = String::with_capacity(input.len());
    let mut word = String::new();
    let flush = |word: &mut String, output: &mut String| {
        if !word.is_empty() {
            output.push_str(temperature(word).map_or(word.as_str(), |unit| unit.0));
            word.clear();
        }
    };
    for ch in source.chars() {
        if ch.is_ascii_alphabetic() {
            word.push(ch);
        } else {
            flush(&mut word, &mut output);
            output.push(ch);
        }
    }
    flush(&mut word, &mut output);
    Some(format!("{output} to {target}"))
}

fn parsed(input: &str) -> Option<(String, String, String)> {
    grammar::conversion_tokens(input).or_else(|| {
        let (from, to) = grammar::split_conversion(input.trim())?;
        Some(("1".into(), from.into(), to.into()))
    })
}

pub(super) fn is_clock_collision(input: &str) -> bool {
    parsed(input).is_some_and(|(_, from, to)| {
        from == "pm"
            && UNITS.iter().any(|unit| {
                unit.aliases.contains(&to.as_str())
                    && matches!(
                        unit.symbol,
                        "m" | "km" | "cm" | "mm" | "pm" | "mi" | "ft" | "inch"
                    )
            })
    })
}

pub fn convert(input: &str) -> Option<EvalResult> {
    let (amount, from, to) = parsed(input)?;
    let compact_from: String = from.split_whitespace().collect();
    let compact_to: String = to.split_whitespace().collect();
    let (source, target) =
        if let (Some(source), Some(target)) = (temperature(&from), temperature(&to)) {
            (source.0.to_string(), target.0.to_string())
        } else {
            (canonical(&compact_from), canonical(&compact_to))
        };
    // Preserve compound expressions, including their whitespace.
    let source = if from.contains(char::is_whitespace) && !from.contains('/') {
        from.clone()
    } else {
        source
    };
    let target = if to.contains(char::is_whitespace) && !to.contains('/') {
        to.clone()
    } else {
        target
    };
    let mut result = engine::evaluate(&format!("{amount} {source} to {target}"), None)?;
    result.expression = format!("{amount} {from}");
    Some(result)
}

fn label(token: &str) -> String {
    UNITS
        .iter()
        .find(|unit| unit.aliases.contains(&token) || unit.symbol == token)
        .map_or_else(|| token.to_string(), |unit| unit.label.to_string())
}

pub(super) fn conversion_detail(input: &str, result: &EvalResult) -> Option<(String, String)> {
    if super::currency::contains_currency(input) {
        return None;
    }
    if let Some((_, from, to)) = parsed(input) {
        if let (Some(source), Some(target)) = (temperature(&from), temperature(&to)) {
            return Some((source.1.into(), target.1.into()));
        }
        return Some((label(&from), label(&to)));
    }
    let (_, consumed) = grammar::amount_token(input)?;
    let from = input[consumed..].trim();
    if from.is_empty() || from.contains(char::is_whitespace) {
        return None;
    }
    let (_, to) = result.value.split_once(char::is_whitespace)?;
    Some((label(from), label(to.trim())))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn symbols_preserve_case() {
        assert_eq!(convert("1 Mb to MB").unwrap().value, "0.125 MB");
        assert_eq!(convert("1 MB to Mb").unwrap().value, "8 Mb");
        assert_eq!(convert("1 gb to mb").unwrap().value, "1000 MB");
        assert_eq!(convert("1 b to B").unwrap().value, "0.125 B");
        assert_eq!(convert("1 Gb to GB").unwrap().value, "0.125 GB");
        assert_eq!(convert("1 kb to KB").unwrap().value, "1 kB");
    }
    #[test]
    fn canonical_factors_are_shared() {
        for unit in ["lb", "oz", "stone"] {
            assert_eq!(
                convert(&format!("1 {unit} to kg")).unwrap().value,
                engine::evaluate(&format!("(1 {unit}) to kg"), None)
                    .unwrap()
                    .value
            );
        }
        assert_eq!(convert("1 lb to kg").unwrap().value, "0.45359237 kg");
    }
    #[test]
    fn conversions_cover_broad_dimensions() {
        for input in [
            "1 acre to m^2",
            "2 cups to ml",
            "1 kWh to J",
            "2 bar to psi",
            "1 GiB to MiB",
            "1 c to m/s",
            "1 pm to m",
            "5k km to miles",
            "inch to cm",
            "5.5kg as lb",
        ] {
            assert!(convert(input).is_some(), "{input}");
        }
        assert_eq!(convert("100 c to f").unwrap().value, "212 °F");
        assert!(convert("10 km to kg").is_none());
    }
}
