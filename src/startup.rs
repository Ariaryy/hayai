//! Registers/unregisters hayai to launch on login via the per-user
//! `HKCU\...\Run` key. Wired into Velopack's install/uninstall fast
//! callbacks in `main.rs` — no admin rights needed since it's HKCU, not
//! HKLM.

#[cfg(windows)]
use winreg::{RegKey, enums::HKEY_CURRENT_USER};

#[cfg(windows)]
const RUN_KEY: &str = "Software\\Microsoft\\Windows\\CurrentVersion\\Run";
#[cfg(windows)]
const VALUE_NAME: &str = "Hayai";

#[cfg(windows)]
pub fn register() -> anyhow::Result<()> {
    let exe_path = std::env::current_exe()?;
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = hkcu.create_subkey(RUN_KEY)?;
    run.set_value(VALUE_NAME, &format!("\"{}\"", exe_path.display()))?;
    Ok(())
}

#[cfg(windows)]
pub fn unregister() -> anyhow::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    if let Ok(run) = hkcu.open_subkey_with_flags(RUN_KEY, winreg::enums::KEY_SET_VALUE) {
        let _ = run.delete_value(VALUE_NAME);
    }
    Ok(())
}
