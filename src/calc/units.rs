//! Unit conversion: length, mass, data, temperature. Static factor tables
//! (temperature is affine, not multiplicative, so it's special-cased).

use super::EvalResult;
use super::format::format_number;
use super::grammar;

struct Unit {
    names: &'static [&'static str],
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
        factor: 1.0,
    },
    Unit {
        names: &["km", "kilometer", "kilometers", "kilometre", "kilometres"],
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
        factor: 0.001,
    },
    Unit {
        names: &["mile", "miles", "mi"],
        factor: 1609.344,
    },
    Unit {
        names: &["yard", "yards", "yd"],
        factor: 0.9144,
    },
    Unit {
        names: &["foot", "feet", "ft"],
        factor: 0.3048,
    },
    Unit {
        names: &["inch", "inches", "in"],
        factor: 0.0254,
    },
    Unit {
        names: &["nmi", "nauticalmile", "nauticalmiles"],
        factor: 1852.0,
    },
];

const MASS: &[Unit] = &[
    Unit {
        names: &["kg", "kilogram", "kilograms"],
        factor: 1.0,
    },
    Unit {
        names: &["g", "gram", "grams"],
        factor: 0.001,
    },
    Unit {
        names: &["mg", "milligram", "milligrams"],
        factor: 0.000_001,
    },
    Unit {
        names: &["lb", "lbs", "pound", "pounds"],
        factor: 0.453_592,
    },
    Unit {
        names: &["oz", "ounce", "ounces"],
        factor: 0.028_349_5,
    },
    Unit {
        names: &["ton", "tonne", "tons", "tonnes"],
        factor: 1000.0,
    },
    Unit {
        names: &["stone", "st"],
        factor: 6.350_29,
    },
];

const DATA: &[Unit] = &[
    Unit {
        names: &["b", "byte", "bytes"],
        factor: 1.0,
    },
    Unit {
        names: &["bit", "bits"],
        factor: 0.125,
    },
    Unit {
        names: &["kb", "kilobyte", "kilobytes"],
        factor: 1_000.0,
    },
    Unit {
        names: &["mb", "megabyte", "megabytes"],
        factor: 1_000_000.0,
    },
    Unit {
        names: &["gb", "gigabyte", "gigabytes"],
        factor: 1_000_000_000.0,
    },
    Unit {
        names: &["tb", "terabyte", "terabytes"],
        factor: 1_000_000_000_000.0,
    },
    Unit {
        names: &["kib", "kibibyte", "kibibytes"],
        factor: 1024.0,
    },
    Unit {
        names: &["mib", "mebibyte", "mebibytes"],
        factor: 1024.0 * 1024.0,
    },
    Unit {
        names: &["gib", "gibibyte", "gibibytes"],
        factor: 1024.0 * 1024.0 * 1024.0,
    },
    Unit {
        names: &["tib", "tebibyte", "tebibytes"],
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

/// Attempts a unit conversion. `None` means either the input isn't shaped
/// like a conversion, or the tokens aren't recognized units — the caller
/// then tries currency conversion next.
pub fn convert(input: &str) -> Option<EvalResult> {
    let (amount, from, to) = grammar::parse_conversion(input)?;
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
