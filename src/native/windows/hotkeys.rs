use super::*;

const HOTKEY_ID: i32 = 1;
pub(super) const WM_SET_HOTKEY: u32 = WM_APP + 2;
static MESSAGE_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
thread_local! { static ACTIVE_HOTKEY: std::cell::Cell<i32> = const { std::cell::Cell::new(HOTKEY_ID) }; }

/// Background-only: the native message thread owns both registrations. Register
/// the replacement first so a conflict never leaves the launcher without a key.
pub fn set_launcher_hotkey((modifiers, key): (u32, u32)) -> anyhow::Result<()> {
    let hwnd = MESSAGE_HWND.load(std::sync::atomic::Ordering::Acquire) as HWND;
    anyhow::ensure!(!hwnd.is_null(), crate::ui_text::settings::HOTKEY_FAILED);
    anyhow::ensure!(
        unsafe { SendMessageW(hwnd, WM_SET_HOTKEY, modifiers as WPARAM, key as LPARAM) } != 0,
        crate::ui_text::settings::HOTKEY_FAILED
    );
    Ok(())
}

pub(super) fn install(hwnd: HWND, hotkey: (u32, u32)) {
    if register_hotkey(hwnd, hotkey).is_err() {
        send(NativeCommand::ShortcutUnavailable);
    }
    MESSAGE_HWND.store(hwnd as isize, std::sync::atomic::Ordering::Release);
}

pub(super) fn is_active(id: i32) -> bool {
    ACTIVE_HOTKEY.with(|active| active.get() == id)
}

pub(super) fn unregister(hwnd: HWND) {
    ACTIVE_HOTKEY.with(|active| unsafe {
        UnregisterHotKey(hwnd, active.get());
    });
    MESSAGE_HWND.store(0, std::sync::atomic::Ordering::Release);
}

pub(super) fn replace(hwnd: HWND, modifiers: u32, key: u32) -> LRESULT {
    ACTIVE_HOTKEY.with(|active| {
        let next = if active.get() == HOTKEY_ID {
            HOTKEY_ID + 1
        } else {
            HOTKEY_ID
        };
        if unsafe { RegisterHotKey(hwnd, next, modifiers | MOD_NOREPEAT, key) } == 0 {
            return 0;
        }
        unsafe {
            UnregisterHotKey(hwnd, active.get());
        }
        active.set(next);
        1
    })
}

fn register_hotkey(hwnd: HWND, (modifiers, key): (u32, u32)) -> io::Result<()> {
    if unsafe { RegisterHotKey(hwnd, HOTKEY_ID, modifiers | MOD_NOREPEAT, key) } == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}
