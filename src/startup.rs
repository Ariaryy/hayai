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
    restore(Some(&format!("\"{}\"", exe_path.display())))
}

#[cfg(windows)]
pub fn unregister() -> anyhow::Result<()> {
    restore(None)
}

/// Read the actual Windows registration so an external startup change is
/// reflected on the next launch. Never rewrite it during an unrelated save.
#[cfg(windows)]
pub fn registration() -> anyhow::Result<Option<String>> {
    registration_at(RUN_KEY)
}

#[cfg(windows)]
fn registration_at(path: &str) -> anyhow::Result<Option<String>> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    match hkcu.open_subkey(path) {
        Ok(run) => match run.get_value(VALUE_NAME) {
            Ok(value) => Ok(Some(value)),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}
#[cfg(windows)]
pub fn restore(value: Option<&str>) -> anyhow::Result<()> {
    restore_at(RUN_KEY, value)
}

#[cfg(windows)]
fn restore_at(path: &str, value: Option<&str>) -> anyhow::Result<()> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (run, _) = hkcu.create_subkey(path)?;
    if let Some(value) = value {
        run.set_value(VALUE_NAME, &value)?;
    } else {
        match run.delete_value(VALUE_NAME) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}
#[cfg(not(windows))]
pub fn registration() -> anyhow::Result<Option<String>> {
    Ok(None)
}
#[cfg(not(windows))]
pub fn restore(_: Option<&str>) -> anyhow::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn register() -> anyhow::Result<()> {
    Ok(())
}
#[cfg(not(windows))]
pub fn unregister() -> anyhow::Result<()> {
    Ok(())
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    #[test]
    fn registration_can_be_disabled_and_restored_without_losing_its_command() {
        // This is an isolated ordinary key, never the user's actual Run key.
        let key = format!(
            "Software\\HayaiAgentTests\\Startup-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        assert_eq!(registration_at(&key).unwrap(), None);
        let command = r#""C:\Apps With Spaces\Hayai.exe" --existing-flag"#;
        restore_at(&key, Some(command)).unwrap();
        let previous = registration_at(&key).unwrap();
        restore_at(&key, None).unwrap();
        assert_eq!(registration_at(&key).unwrap(), None);
        restore_at(&key, previous.as_deref()).unwrap();
        assert_eq!(registration_at(&key).unwrap().as_deref(), Some(command));
        restore_at(&key, None).unwrap();
        restore_at(&key, None).unwrap();
        RegKey::predef(HKEY_CURRENT_USER)
            .delete_subkey(key)
            .unwrap();
    }
}
