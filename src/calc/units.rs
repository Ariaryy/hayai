//! Unit conversion: length, mass, data, temperature. Static factor tables
//! (temperature is affine, not multiplicative, so it's special-cased).

use super::EvalResult;
use super::format::format_number;
use super::grammar;

struct Unit {
    names: &'static [&'static str],
    label: &'static str,
    factor: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum Dimension {
    Length,
    Mass,
    Data,
}

const LENGTH: &[Unit] = &[
    Unit {
        names: &["m", "meter", "meters", "metre", "metres"],
        label: "Meter",
        factor: 1.0,
    },
    Unit {
        names: &["km", "kilometer", "kilometers", "kilometre", "kilometres"],
        label: "Kilometer",
        factor: 1000.0,
    },
    Unit {
        names: &[
            "cm",
            "centimeter",
            "centimeters",
            "centimetre",
            "centimetres",
        ],
        label: "Centimeter",
        factor: 0.01,
    },
    Unit {
        names: &[
            "mm",
            "millimeter",
            "millimeters",
            "millimetre",
            "millimetres",
        ],
        label: "Millimeter",
        factor: 0.001,
    },
    Unit {
        names: &["mile", "miles", "mi"],
        label: "Mile",
        factor: 1609.344,
    },
    Unit {
        names: &["yard", "yards", "yd"],
        label: "Yard",
        factor: 0.9144,
    },
    Unit {
        names: &["foot", "feet", "ft"],
        label: "Foot",
        factor: 0.3048,
    },
    Unit {
        names: &["inch", "inches", "in"],
        label: "Inch",
        factor: 0.0254,
    },
    Unit {
        names: &["nmi", "nauticalmile", "nauticalmiles"],
        label: "Nautical Mile",
        factor: 1852.0,
    },
];

const MASS: &[Unit] = &[
    Unit {
        names: &["kg", "kilogram", "kilograms"],
        label: "Kilogram",
        factor: 1.0,
    },
    Unit {
        names: &["g", "gram", "grams"],
        label: "Gram",
        factor: 0.001,
    },
    Unit {
        names: &["mg", "milligram", "milligrams"],
        label: "Milligram",
        factor: 0.000_001,
    },
    Unit {
        names: &["lb", "lbs", "pound", "pounds"],
        label: "Pound",
        factor: 0.453_592,
    },
    Unit {
        names: &["oz", "ounce", "ounces"],
        label: "Ounce",
        factor: 0.028_349_5,
    },
    Unit {
        names: &["ton", "tonne", "tons", "tonnes"],
        label: "Metric Ton",
        factor: 1000.0,
    },
    Unit {
        names: &["stone", "st"],
        label: "Stone",
        factor: 6.350_29,
    },
];

const DATA: &[Unit] = &[
    Unit {
        names: &["b", "byte", "bytes"],
        label: "Byte",
        factor: 1.0,
    },
    Unit {
        names: &["bit", "bits"],
        label: "Bit",
        factor: 0.125,
    },
    Unit {
        names: &["kb", "kilobyte", "kilobytes"],
        label: "Kilobyte",
        factor: 1_000.0,
    },
    Unit {
        names: &["mb", "megabyte", "megabytes"],
        label: "Megabyte",
        factor: 1_000_000.0,
    },
    Unit {
        names: &["gb", "gigabyte", "gigabytes"],
        label: "Gigabyte",
        factor: 1_000_000_000.0,
    },
    Unit {
        names: &["tb", "terabyte", "terabytes"],
        label: "Terabyte",
        factor: 1_000_000_000_000.0,
    },
    Unit {
        names: &["kib", "kibibyte", "kibibytes"],
        label: "Kibibyte",
        factor: 1024.0,
    },
    Unit {
        names: &["mib", "mebibyte", "mebibytes"],
        label: "Mebibyte",
        factor: 1024.0 * 1024.0,
    },
    Unit {
        names: &["gib", "gibibyte", "gibibytes"],
        label: "Gibibyte",
        factor: 1024.0 * 1024.0 * 1024.0,
    },
    Unit {
        names: &["tib", "tebibyte", "tebibytes"],
        label: "Tebibyte",
        factor: 1024.0 * 1024.0 * 1024.0 * 1024.0,
    },
];

fn find_unit(token: &str) -> Option<(Dimension, f64)> {
    for (dimension, table) in [
        (Dimension::Length, LENGTH),
        (Dimension::Mass, MASS),
        (Dimension::Data, DATA),
    ] {
        if let Some(unit) = table.iter().find(|u| u.names.contains(&token)) {
            return Some((dimension, unit.factor));
        }
    }
    None
}

fn unit_label(token: &str) -> Option<&'static str> {
    LENGTH
        .iter()
        .chain(MASS)
        .chain(DATA)
        .find(|unit| unit.names.contains(&token))
        .map(|unit| unit.label)
}

#[derive(Clone, Copy)]
enum Temp {
    Celsius,
    Fahrenheit,
    Kelvin,
}

fn find_temp(token: &str) -> Option<Temp> {
    match token {
        "c" | "celsius" | "°c" => Some(Temp::Celsius),
        "f" | "fahrenheit" | "°f" => Some(Temp::Fahrenheit),
        "k" | "kelvin" => Some(Temp::Kelvin),
        _ => None,
    }
}

impl Temp {
    fn to_celsius(self, v: f64) -> f64 {
        match self {
            Temp::Celsius => v,
            Temp::Fahrenheit => (v - 32.0) * 5.0 / 9.0,
            Temp::Kelvin => v - 273.15,
        }
    }

    fn convert_from_celsius(self, c: f64) -> f64 {
        match self {
            Temp::Celsius => c,
            Temp::Fahrenheit => c * 9.0 / 5.0 + 32.0,
            Temp::Kelvin => c + 273.15,
        }
    }
}

fn temp_label(temp: Temp) -> &'static str {
    match temp {
        Temp::Celsius => "Celsius",
        Temp::Fahrenheit => "Fahrenheit",
        Temp::Kelvin => "Kelvin",
    }
}

fn parse_conversion(input: &str) -> Option<(f64, String, String)> {
    grammar::parse_conversion(input).or_else(|| {
        let input = input.trim();
        let from_end = input.find(char::is_whitespace)?;
        let from = input[..from_end].to_lowercase();
        let rest = input[from_end..].trim_start();
        let (to, matched) = grammar::parse_keyword_then_target(rest);
        matched.then_some((1.0, from, to))
    })
}

pub(super) fn conversion_detail(input: &str) -> Option<(String, String)> {
    let (_, from, to) = parse_conversion(input)?;
    if let (Some(from_temp), Some(to_temp)) = (find_temp(&from), find_temp(&to)) {
        return Some((
            temp_label(from_temp).to_string(),
            temp_label(to_temp).to_string(),
        ));
    }

    let (from_dimension, _) = find_unit(&from)?;
    let (to_dimension, _) = find_unit(&to)?;
    (from_dimension == to_dimension).then(|| {
        (
            unit_label(&from).unwrap().to_string(),
            unit_label(&to).unwrap().to_string(),
        )
    })
}

/// Attempts a unit conversion. `None` means either the input isn't shaped
/// like a conversion, or the tokens aren't recognized units — the caller
/// then tries currency conversion next.
pub fn convert(input: &str) -> Option<EvalResult> {
    let (amount, from, to) = parse_conversion(input)?;
    let expression = format!("{} {}", format_number(amount), from);

    if let (Some(from_t), Some(to_t)) = (find_temp(&from), find_temp(&to)) {
        let result = to_t.convert_from_celsius(from_t.to_celsius(amount));
        return Some(EvalResult {
            expression,
            value: format!("{} {}", format_number(result), to),
        });
    }

    let (from_dim, from_factor) = find_unit(&from)?;
    let (to_dim, to_factor) = find_unit(&to)?;
    if from_dim != to_dim {
        return None; // mismatched dimensions (e.g. "km to kg") — not a valid conversion
    }
    let result = amount * from_factor / to_factor;
    Some(EvalResult {
        expression,
        value: format!("{} {}", format_number(result), to),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn length() {
        let r = convert("10 km in miles").unwrap();
        assert!(r.value.starts_with("6.2"));
        assert_eq!(
            conversion_detail("1 inch to cm"),
            Some(("Inch".into(), "Centimeter".into()))
        );
        let implied = convert("inch to cm").unwrap();
        assert_eq!(implied.expression, "1 inch");
        assert_eq!(implied.value, "2.54 cm");
    }

    #[test]
    fn mass() {
        let r = convert("1 kg to lb").unwrap();
        assert!(r.value.starts_with("2.2"));
    }

    #[test]
    fn data() {
        let r = convert("1 gb to mb").unwrap();
        assert_eq!(r.value, "1,000 mb");
    }

    #[test]
    fn temperature() {
        let r = convert("100 c to f").unwrap();
        assert_eq!(r.value, "212 f");
        let r = convert("0 c to k").unwrap();
        assert_eq!(r.value, "273.15 k");
    }

    #[test]
    fn rejects_mismatched_dimensions() {
        assert!(convert("10 km to kg").is_none());
    }

    #[test]
    fn rejects_unknown_units() {
        assert!(convert("10 ironman to miles").is_none());
    }
}
