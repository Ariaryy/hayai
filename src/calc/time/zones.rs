use super::*;

/// Explicit timezone abbreviations mapped to fixed UTC offsets in minutes.
/// City aliases are deliberately separate below because Windows applies
/// daylight-saving rules to them for the requested date.
/// Ambiguous abbreviations take their most common meaning in English: `IST`
/// is India, `CST` and `AST` are North American, and `BST` is British.
const TIMEZONES: &[(&[&str], i32)] = &[
    (&["utc", "gmt"], 0),
    (&["wet"], 0),
    (&["west"], 60),
    (&["bst"], 60),
    (&["cet"], 60),
    (&["wat"], 60),
    (&["cest"], 120),
    (&["eet"], 120),
    (&["sast"], 120),
    (&["cat"], 120),
    (&["eest"], 180),
    (&["msk"], 180),
    (&["eat"], 180),
    (&["trt"], 180),
    (&["irst"], 210),
    (&["nst"], -210),
    (&["ndt"], -150),
    (&["ast"], -240),
    (&["adt"], -180),
    (&["brt"], -180),
    (&["art"], -180),
    (&["est"], -300),
    (&["edt"], -240),
    (&["cst"], -360),
    (&["cdt"], -300),
    (&["mst"], -420),
    (&["mdt"], -360),
    (&["pst"], -480),
    (&["pdt"], -420),
    (&["akst"], -540),
    (&["akdt"], -480),
    (&["hst"], -600),
    (&["gst"], 240),
    (&["pkt"], 300),
    (&["ist"], 330),
    (&["npt"], 345),
    (&["ict"], 420),
    (&["wib"], 420),
    (&["sgt"], 480),
    (&["hkt"], 480),
    (&["pht"], 480),
    (&["myt"], 480),
    (&["awst"], 480),
    (&["cst_asia"], 480),
    (&["jst"], 540),
    (&["kst"], 540),
    (&["acst"], 570),
    (&["acdt"], 630),
    (&["aest"], 600),
    (&["aedt"], 660),
    (&["nzst"], 720),
    (&["nzdt"], 780),
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
        aliases: &["london", "uk", "unitedkingdom", "england", "britain"],
        windows_key: "GMT Standard Time",
        identity: "Europe/London",
        standard_offset: 0,
        standard_abbreviation: "GMT",
        daylight_abbreviation: "BST",
    },
    CityZone {
        aliases: &["paris", "madrid", "brussels", "france", "spain"],
        windows_key: "Romance Standard Time",
        identity: "Europe/Paris",
        standard_offset: 60,
        standard_abbreviation: "CET",
        daylight_abbreviation: "CEST",
    },
    CityZone {
        aliases: &[
            "berlin",
            "rome",
            "amsterdam",
            "zurich",
            "vienna",
            "stockholm",
            "germany",
            "italy",
            "netherlands",
        ],
        windows_key: "W. Europe Standard Time",
        identity: "Europe/Berlin",
        standard_offset: 60,
        standard_abbreviation: "CET",
        daylight_abbreviation: "CEST",
    },
    CityZone {
        aliases: &["athens", "greece"],
        windows_key: "GTB Standard Time",
        identity: "Europe/Athens",
        standard_offset: 120,
        standard_abbreviation: "EET",
        daylight_abbreviation: "EEST",
    },
    CityZone {
        aliases: &["cairo", "egypt"],
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
        aliases: &[
            "newyork",
            "nyc",
            "toronto",
            "boston",
            "washington",
            "miami",
            "usa",
            "unitedstates",
            "america",
            "et",
            "eastern",
            "easterntime",
        ],
        windows_key: "Eastern Standard Time",
        identity: "America/New_York",
        standard_offset: -300,
        standard_abbreviation: "EST",
        daylight_abbreviation: "EDT",
    },
    CityZone {
        aliases: &[
            "chicago",
            "houston",
            "dallas",
            "ct",
            "central",
            "centraltime",
        ],
        windows_key: "Central Standard Time",
        identity: "America/Chicago",
        standard_offset: -360,
        standard_abbreviation: "CST",
        daylight_abbreviation: "CDT",
    },
    CityZone {
        aliases: &["denver", "mt", "mountain", "mountaintime"],
        windows_key: "Mountain Standard Time",
        identity: "America/Denver",
        standard_offset: -420,
        standard_abbreviation: "MST",
        daylight_abbreviation: "MDT",
    },
    CityZone {
        aliases: &[
            "losangeles",
            "la",
            "sf",
            "sanfrancisco",
            "seattle",
            "vancouver",
            "lasvegas",
            "pt",
            "pacific",
            "pacifictime",
        ],
        windows_key: "Pacific Standard Time",
        identity: "America/Los_Angeles",
        standard_offset: -480,
        standard_abbreviation: "PST",
        daylight_abbreviation: "PDT",
    },
    CityZone {
        aliases: &["dubai", "abudhabi", "uae"],
        windows_key: "Arabian Standard Time",
        identity: "Asia/Dubai",
        standard_offset: 240,
        standard_abbreviation: "GST",
        daylight_abbreviation: "GST",
    },
    CityZone {
        aliases: &[
            "mumbai",
            "delhi",
            "newdelhi",
            "bangalore",
            "bengaluru",
            "kolkata",
            "india",
        ],
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
        aliases: &["tokyo", "shibuya", "japan", "osaka"],
        windows_key: "Tokyo Standard Time",
        identity: "Asia/Tokyo",
        standard_offset: 540,
        standard_abbreviation: "JST",
        daylight_abbreviation: "JST",
    },
    CityZone {
        aliases: &["seoul", "korea", "southkorea"],
        windows_key: "Korea Standard Time",
        identity: "Asia/Seoul",
        standard_offset: 540,
        standard_abbreviation: "KST",
        daylight_abbreviation: "KST",
    },
    CityZone {
        aliases: &["sydney", "melbourne", "canberra", "australia", "aet"],
        windows_key: "AUS Eastern Standard Time",
        identity: "Australia/Sydney",
        standard_offset: 600,
        standard_abbreviation: "AEST",
        daylight_abbreviation: "AEDT",
    },
    CityZone {
        aliases: &["auckland", "newzealand", "nzt"],
        windows_key: "New Zealand Standard Time",
        identity: "Pacific/Auckland",
        standard_offset: 720,
        standard_abbreviation: "NZST",
        daylight_abbreviation: "NZDT",
    },
    // Major cities with their own rules. Keep this list to the most-used
    // places; IANA names and Windows zone names cover the rest. Zones that
    // share a Windows key come after the first city for that key, which
    // labels the system timezone.
    CityZone {
        aliases: &["dublin", "ireland"],
        windows_key: "GMT Standard Time",
        identity: "Europe/Dublin",
        standard_offset: 0,
        standard_abbreviation: "GMT",
        daylight_abbreviation: "IST",
    },
    CityZone {
        aliases: &["lisbon", "portugal"],
        windows_key: "GMT Standard Time",
        identity: "Europe/Lisbon",
        standard_offset: 0,
        standard_abbreviation: "WET",
        daylight_abbreviation: "WEST",
    },
    CityZone {
        aliases: &["honolulu", "hawaii", "ht"],
        windows_key: "Hawaiian Standard Time",
        identity: "Pacific/Honolulu",
        standard_offset: -600,
        standard_abbreviation: "HST",
        daylight_abbreviation: "HST",
    },
    CityZone {
        aliases: &["anchorage", "alaska", "akt"],
        windows_key: "Alaskan Standard Time",
        identity: "America/Anchorage",
        standard_offset: -540,
        standard_abbreviation: "AKST",
        daylight_abbreviation: "AKDT",
    },
    CityZone {
        aliases: &["phoenix", "arizona"],
        windows_key: "US Mountain Standard Time",
        identity: "America/Phoenix",
        standard_offset: -420,
        standard_abbreviation: "MST",
        daylight_abbreviation: "MST",
    },
    CityZone {
        aliases: &["mexicocity", "mexico"],
        windows_key: "Central Standard Time (Mexico)",
        identity: "America/Mexico_City",
        standard_offset: -360,
        standard_abbreviation: "CST",
        daylight_abbreviation: "CDT",
    },
    CityZone {
        aliases: &["saopaulo", "riodejaneiro", "brazil"],
        windows_key: "E. South America Standard Time",
        identity: "America/Sao_Paulo",
        standard_offset: -180,
        standard_abbreviation: "BRT",
        daylight_abbreviation: "BRST",
    },
    CityZone {
        aliases: &["buenosaires", "argentina"],
        windows_key: "Argentina Standard Time",
        identity: "America/Argentina/Buenos_Aires",
        standard_offset: -180,
        standard_abbreviation: "ART",
        daylight_abbreviation: "ART",
    },
    CityZone {
        aliases: &["istanbul", "turkey"],
        windows_key: "Turkey Standard Time",
        identity: "Europe/Istanbul",
        standard_offset: 180,
        standard_abbreviation: "TRT",
        daylight_abbreviation: "TRT",
    },
    CityZone {
        aliases: &["riyadh", "saudiarabia"],
        windows_key: "Arab Standard Time",
        identity: "Asia/Riyadh",
        standard_offset: 180,
        standard_abbreviation: "AST",
        daylight_abbreviation: "AST",
    },
    CityZone {
        aliases: &["tehran", "iran"],
        windows_key: "Iran Standard Time",
        identity: "Asia/Tehran",
        standard_offset: 210,
        standard_abbreviation: "IRST",
        daylight_abbreviation: "IRDT",
    },
    CityZone {
        aliases: &["karachi", "lahore", "pakistan"],
        windows_key: "Pakistan Standard Time",
        identity: "Asia/Karachi",
        standard_offset: 300,
        standard_abbreviation: "PKT",
        daylight_abbreviation: "PKT",
    },
    CityZone {
        aliases: &["dhaka", "bangladesh"],
        windows_key: "Bangladesh Standard Time",
        identity: "Asia/Dhaka",
        standard_offset: 360,
        standard_abbreviation: "BST",
        daylight_abbreviation: "BST",
    },
    CityZone {
        aliases: &["kathmandu", "nepal"],
        windows_key: "Nepal Standard Time",
        identity: "Asia/Kathmandu",
        standard_offset: 345,
        standard_abbreviation: "NPT",
        daylight_abbreviation: "NPT",
    },
    CityZone {
        aliases: &["bangkok", "thailand"],
        windows_key: "SE Asia Standard Time",
        identity: "Asia/Bangkok",
        standard_offset: 420,
        standard_abbreviation: "ICT",
        daylight_abbreviation: "ICT",
    },
    CityZone {
        aliases: &["jakarta", "indonesia"],
        windows_key: "SE Asia Standard Time",
        identity: "Asia/Jakarta",
        standard_offset: 420,
        standard_abbreviation: "WIB",
        daylight_abbreviation: "WIB",
    },
    CityZone {
        aliases: &["manila", "philippines"],
        windows_key: "Singapore Standard Time",
        identity: "Asia/Manila",
        standard_offset: 480,
        standard_abbreviation: "PHT",
        daylight_abbreviation: "PHT",
    },
    CityZone {
        aliases: &["kualalumpur", "malaysia"],
        windows_key: "Singapore Standard Time",
        identity: "Asia/Kuala_Lumpur",
        standard_offset: 480,
        standard_abbreviation: "MYT",
        daylight_abbreviation: "MYT",
    },
    CityZone {
        aliases: &["taipei", "taiwan"],
        windows_key: "Taipei Standard Time",
        identity: "Asia/Taipei",
        standard_offset: 480,
        standard_abbreviation: "CST",
        daylight_abbreviation: "CST",
    },
    CityZone {
        aliases: &["perth"],
        windows_key: "W. Australia Standard Time",
        identity: "Australia/Perth",
        standard_offset: 480,
        standard_abbreviation: "AWST",
        daylight_abbreviation: "AWST",
    },
    CityZone {
        aliases: &["brisbane"],
        windows_key: "E. Australia Standard Time",
        identity: "Australia/Brisbane",
        standard_offset: 600,
        standard_abbreviation: "AEST",
        daylight_abbreviation: "AEST",
    },
    CityZone {
        aliases: &["adelaide"],
        windows_key: "Cen. Australia Standard Time",
        identity: "Australia/Adelaide",
        standard_offset: 570,
        standard_abbreviation: "ACST",
        daylight_abbreviation: "ACDT",
    },
    CityZone {
        aliases: &["johannesburg", "capetown", "southafrica"],
        windows_key: "South Africa Standard Time",
        identity: "Africa/Johannesburg",
        standard_offset: 120,
        standard_abbreviation: "SAST",
        daylight_abbreviation: "SAST",
    },
    CityZone {
        aliases: &["lagos", "nigeria"],
        windows_key: "W. Central Africa Standard Time",
        identity: "Africa/Lagos",
        standard_offset: 60,
        standard_abbreviation: "WAT",
        daylight_abbreviation: "WAT",
    },
    CityZone {
        aliases: &["nairobi", "kenya"],
        windows_key: "E. Africa Standard Time",
        identity: "Africa/Nairobi",
        standard_offset: 180,
        standard_abbreviation: "EAT",
        daylight_abbreviation: "EAT",
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
pub(super) struct ResolvedZone {
    kind: ZoneKind,
    identity: String,
    standard_offset: i32,
    standard_abbreviation: String,
    daylight_abbreviation: String,
}

impl ResolvedZone {
    pub(super) fn abbreviation(&self, offset: i32) -> &str {
        if offset == self.standard_offset {
            &self.standard_abbreviation
        } else {
            &self.daylight_abbreviation
        }
    }

    pub(super) fn label(&self, offset: i32) -> String {
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

fn compact_place(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// "utc+3", "gmt-8", "utc+5:30", "utc+0530", "gmt + 5:45", or the sign first
/// as in "+5:30 gmt" (spaces are already removed) as a fixed offset in minutes
/// and its display name. Real offsets span UTC-12 to UTC+14.
fn utc_offset(compact: &str) -> Option<(i32, String)> {
    let (prefix, rest) = ["utc", "gmt"].iter().find_map(|prefix| {
        let rest = compact.strip_prefix(prefix).or_else(|| {
            compact
                .strip_suffix(prefix)
                .filter(|rest| rest.starts_with(['+', '-']))
        })?;
        Some((*prefix, rest))
    })?;
    let sign = match rest.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let digits = &rest[1..];
    let (hours, minutes) = match digits.split_once(':') {
        Some(parts) => parts,
        None if digits.len() > 2 => digits.split_at(digits.len() - 2),
        None => (digits, "0"),
    };
    let field = |text: &str| -> Option<i32> {
        (!text.is_empty() && text.len() <= 2 && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse().ok())
            .flatten()
    };
    let (hours, minutes) = (field(hours)?, field(minutes)?);
    let offset = sign * (hours * 60 + minutes);
    if minutes >= 60 || !(-12 * 60..=14 * 60).contains(&offset) {
        return None;
    }
    let suffix = format_utc_offset(offset);
    Some((
        offset,
        format!(
            "{}{}",
            prefix.to_ascii_uppercase(),
            suffix.trim_start_matches("GMT")
        ),
    ))
}

pub(super) fn resolve_zone(token: &str) -> Option<ResolvedZone> {
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
    let compact = compact_place(&lower);
    if let Some(zone) = CITY_ZONES.iter().find(|zone| {
        zone.aliases.contains(&compact.as_str())
            || zone.identity.eq_ignore_ascii_case(&lower)
            || zone.windows_key.eq_ignore_ascii_case(&lower)
    }) {
        return Some(ResolvedZone {
            kind: ZoneKind::Windows(zone.windows_key.to_string()),
            identity: zone.identity.to_string(),
            standard_offset: zone.standard_offset,
            standard_abbreviation: zone.standard_abbreviation.to_string(),
            daylight_abbreviation: zone.daylight_abbreviation.to_string(),
        });
    }
    if let Some((offset, name)) = utc_offset(&compact) {
        return Some(ResolvedZone {
            kind: ZoneKind::Fixed(offset),
            identity: "Fixed offset".to_string(),
            standard_offset: offset,
            standard_abbreviation: name.clone(),
            daylight_abbreviation: name,
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
        "akst" | "akdt" => "America/Anchorage",
        "hst" => "Pacific/Honolulu",
        "ast" | "adt" => "America/Halifax",
        "nst" | "ndt" => "America/St_Johns",
        "brt" => "America/Sao_Paulo",
        "art" => "America/Argentina/Buenos_Aires",
        "wet" | "west" => "Europe/Lisbon",
        "bst" => "Europe/London",
        "cet" | "cest" => "Europe/Paris",
        "eet" | "eest" => "Europe/Athens",
        "msk" => "Europe/Moscow",
        "trt" => "Europe/Istanbul",
        "wat" => "Africa/Lagos",
        "cat" => "Africa/Maputo",
        "sast" => "Africa/Johannesburg",
        "eat" => "Africa/Nairobi",
        "irst" => "Asia/Tehran",
        "gst" => "Asia/Dubai",
        "pkt" => "Asia/Karachi",
        "ist" => "Asia/Kolkata",
        "npt" => "Asia/Kathmandu",
        "ict" => "Asia/Bangkok",
        "wib" => "Asia/Jakarta",
        "sgt" => "Asia/Singapore",
        "hkt" => "Asia/Hong_Kong",
        "pht" => "Asia/Manila",
        "myt" => "Asia/Kuala_Lumpur",
        "cst_asia" => "Asia/Shanghai",
        "jst" => "Asia/Tokyo",
        "kst" => "Asia/Seoul",
        "awst" => "Australia/Perth",
        "acst" | "acdt" => "Australia/Adelaide",
        "aest" | "aedt" => "Australia/Sydney",
        "nzst" | "nzdt" => "Pacific/Auckland",
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

pub(super) fn date_time_minutes(date_time: CivilDateTime) -> i64 {
    days_from_civil(date_time.0 as i64, date_time.1, date_time.2) * 24 * 60
        + date_time.3 as i64 * 60
        + date_time.4 as i64
}

pub(super) fn minutes_date_time(minutes: i64) -> CivilDateTime {
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

pub(super) fn zone_to_utc(
    zone: &ResolvedZone,
    local: CivilDateTime,
) -> Option<(CivilDateTime, i32)> {
    let utc = match &zone.kind {
        ZoneKind::Fixed(offset) => minutes_date_time(date_time_minutes(local) - *offset as i64),
        ZoneKind::Windows(key) => native::timezone_local_to_utc(key, local)?,
    };
    let offset = (date_time_minutes(local) - date_time_minutes(utc)) as i32;
    Some((utc, offset))
}

pub(super) fn zone_from_utc(
    zone: &ResolvedZone,
    utc: CivilDateTime,
) -> Option<(CivilDateTime, i32)> {
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

pub(super) fn format_utc_offset(minutes: i32) -> String {
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

pub(in crate::calc) fn target_prefix(target: &str) -> bool {
    if target.is_empty() || resolve_zone(target).is_some() {
        return false;
    }
    let prefix = compact_place(target);
    TIMEZONES
        .iter()
        .any(|(names, _)| names.iter().any(|name| name.starts_with(&prefix)))
        || CITY_ZONES.iter().any(|zone| {
            zone.aliases.iter().any(|name| name.starts_with(&prefix))
                || compact_place(zone.identity).starts_with(&prefix)
                || compact_place(zone.windows_key).starts_with(&prefix)
        })
}
