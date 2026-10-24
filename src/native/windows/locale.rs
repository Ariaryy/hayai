/// The ISO 4217 currency code (lowercase, e.g. `"usd"`) implied by the
/// user's Windows region setting — used to pick a default "convert to"
/// currency for bare amounts like "$100" with no explicit target. `None`
/// on any lookup failure (falls back to no default, forcing an explicit
/// target).
pub fn system_currency_code() -> Option<String> {
    use windows_sys::Win32::Globalization::{
        GetLocaleInfoEx, GetUserDefaultLocaleName, LOCALE_SINTLSYMBOL,
    };

    unsafe {
        // LOCALE_NAME_MAX_LENGTH, per the Win32 docs — not exposed as a
        // constant by windows-sys.
        let mut locale_name = [0u16; 85];
        if GetUserDefaultLocaleName(locale_name.as_mut_ptr(), locale_name.len() as i32) == 0 {
            return None;
        }
        let mut buffer = [0u16; 8];
        let len = GetLocaleInfoEx(
            locale_name.as_ptr(),
            LOCALE_SINTLSYMBOL,
            buffer.as_mut_ptr(),
            buffer.len() as i32,
        );
        if len <= 1 {
            return None;
        }
        // `len` includes the null terminator.
        let code = String::from_utf16_lossy(&buffer[..(len as usize - 1)])
            .trim()
            .to_lowercase();
        if code.is_empty() { None } else { Some(code) }
    }
}

/// Returns Windows' canonical name for the configured local timezone (for
/// example, "India Standard Time") without loading a timezone database.
pub fn system_timezone_name() -> Option<String> {
    use windows_sys::Win32::System::Time::{
        DYNAMIC_TIME_ZONE_INFORMATION, GetDynamicTimeZoneInformation,
    };

    let mut info = DYNAMIC_TIME_ZONE_INFORMATION::default();
    let status = unsafe { GetDynamicTimeZoneInformation(&mut info) };
    if status == u32::MAX {
        return None;
    }
    let name = if info.TimeZoneKeyName[0] != 0 {
        &info.TimeZoneKeyName[..]
    } else {
        &info.StandardName[..]
    };
    let len = name.iter().position(|character| *character == 0)?;
    String::from_utf16(&name[..len]).ok()
}

thread_local! {
    // Keep only the few zones in use, rather than retaining the Windows zone
    // table. Expiry picks up OS rule updates without a resident polling task.
    static TIMEZONE_CACHE: std::cell::RefCell<Vec<(String, std::time::Instant,
        windows_sys::Win32::System::Time::DYNAMIC_TIME_ZONE_INFORMATION)>> = const { std::cell::RefCell::new(Vec::new()) };
}

fn dynamic_timezone(
    key_name: &str,
) -> Option<windows_sys::Win32::System::Time::DYNAMIC_TIME_ZONE_INFORMATION> {
    use windows_sys::Win32::System::Time::{
        DYNAMIC_TIME_ZONE_INFORMATION, EnumDynamicTimeZoneInformation,
    };
    let cached = TIMEZONE_CACHE.with(|cache| {
        cache
            .borrow()
            .iter()
            .find(|(key, at, _)| key.eq_ignore_ascii_case(key_name) && at.elapsed().as_secs() < 60)
            .map(|(_, _, info)| *info)
    });
    if cached.is_some() {
        return cached;
    }
    for index in 0.. {
        let mut info = DYNAMIC_TIME_ZONE_INFORMATION::default();
        let status = unsafe { EnumDynamicTimeZoneInformation(index, &mut info) };
        if status != 0 {
            return None;
        }
        let len = info
            .TimeZoneKeyName
            .iter()
            .position(|c| *c == 0)
            .unwrap_or(info.TimeZoneKeyName.len());
        // Compare in UTF-16 without allocating every preceding zone's name.
        let matches = key_name.is_ascii()
            && len == key_name.len()
            && info.TimeZoneKeyName[..len]
                .iter()
                .zip(key_name.bytes())
                .all(|(left, right)| *left <= 127 && (*left as u8).eq_ignore_ascii_case(&right));
        if matches {
            TIMEZONE_CACHE.with(|cache| {
                let mut cache = cache.borrow_mut();
                cache.retain(|(key, at, _)| {
                    !key.eq_ignore_ascii_case(key_name) && at.elapsed().as_secs() < 60
                });
                if cache.len() == 4 {
                    cache.remove(0);
                }
                cache.push((key_name.into(), std::time::Instant::now(), info));
            });
            return Some(info);
        }
    }
    None
}

fn system_time(date_time: (i32, u32, u32, u32, u32)) -> windows_sys::Win32::Foundation::SYSTEMTIME {
    windows_sys::Win32::Foundation::SYSTEMTIME {
        wYear: date_time.0.try_into().ok().unwrap_or_default(),
        wMonth: date_time.1.try_into().ok().unwrap_or_default(),
        wDayOfWeek: 0,
        wDay: date_time.2.try_into().ok().unwrap_or_default(),
        wHour: date_time.3.try_into().ok().unwrap_or_default(),
        wMinute: date_time.4.try_into().ok().unwrap_or_default(),
        wSecond: 0,
        wMilliseconds: 0,
    }
}

fn date_time(time: windows_sys::Win32::Foundation::SYSTEMTIME) -> (i32, u32, u32, u32, u32) {
    (
        time.wYear.into(),
        time.wMonth.into(),
        time.wDay.into(),
        time.wHour.into(),
        time.wMinute.into(),
    )
}

pub fn timezone_local_to_utc(
    key_name: &str,
    local: (i32, u32, u32, u32, u32),
) -> Option<(i32, u32, u32, u32, u32)> {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::TzSpecificLocalTimeToSystemTimeEx;

    let timezone = dynamic_timezone(key_name)?;
    let local = system_time(local);
    let mut utc = SYSTEMTIME::default();
    if unsafe { TzSpecificLocalTimeToSystemTimeEx(&timezone, &local, &mut utc) } == 0 {
        return None;
    }
    let mut roundtrip = SYSTEMTIME::default();
    if unsafe {
        windows_sys::Win32::System::Time::SystemTimeToTzSpecificLocalTimeEx(
            &timezone,
            &utc,
            &mut roundtrip,
        )
    } == 0
        || date_time(roundtrip) != date_time(local)
    {
        return None;
    }
    Some(date_time(utc))
}

pub fn timezone_utc_to_local(
    key_name: &str,
    utc: (i32, u32, u32, u32, u32),
) -> Option<(i32, u32, u32, u32, u32)> {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::Time::SystemTimeToTzSpecificLocalTimeEx;

    let timezone = dynamic_timezone(key_name)?;
    let utc = system_time(utc);
    let mut local = SYSTEMTIME::default();
    (unsafe { SystemTimeToTzSpecificLocalTimeEx(&timezone, &utc, &mut local) } != 0)
        .then(|| date_time(local))
}

/// The user's current local wall-clock time (year, month, day, hour,
/// minute) — the "now" anchor for date/time arithmetic like "5 days from
/// now" or "3pm + 5". Reads OS local time directly rather than computing a
/// UTC offset ourselves, since `GetLocalTime` already accounts for the
/// system's current DST state.
pub fn local_now() -> (i32, u32, u32, u32, u32) {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;

    let mut time: SYSTEMTIME = unsafe { std::mem::zeroed() };
    unsafe { GetLocalTime(&mut time) };
    (
        time.wYear as i32,
        time.wMonth as u32,
        time.wDay as u32,
        time.wHour as u32,
        time.wMinute as u32,
    )
}

/// Seconds are retained for calculator elapsed-time expressions.
pub fn local_now_seconds() -> (i32, u32, u32, u32, u32, u32) {
    use windows_sys::Win32::Foundation::SYSTEMTIME;
    use windows_sys::Win32::System::SystemInformation::GetLocalTime;
    let mut time = SYSTEMTIME::default();
    unsafe { GetLocalTime(&mut time) };
    (
        time.wYear.into(),
        time.wMonth.into(),
        time.wDay.into(),
        time.wHour.into(),
        time.wMinute.into(),
        time.wSecond.into(),
    )
}
