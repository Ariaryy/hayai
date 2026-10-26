use super::{Preferences, SettingsGlobal};
use gpui::{App, AppContext};
use std::path::PathBuf;

impl SettingsGlobal {
    pub fn load() -> Self {
        let path = std::env::var_os("HAYAI_SETTINGS_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("LOCALAPPDATA").unwrap_or_default())
                    .join("Hayai")
                    .join("settings.json")
            });
        let (preferences, error) = match std::fs::read_to_string(&path) {
            Ok(contents) => match Preferences::parse(&contents) {
                Ok(preferences) => (preferences, None),
                Err(error) => (
                    Preferences::default(),
                    Some(format!("{} {error}", crate::ui_text::settings::LOAD_FAILED)),
                ),
            },
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                (Preferences::default(), None)
            }
            Err(error) => (
                Preferences::default(),
                Some(format!("{} {error}", crate::ui_text::settings::LOAD_FAILED)),
            ),
        };
        Self {
            preferences,
            error,
            saving: false,
            window: None,
            view: None,
            path,
        }
    }
}

pub fn save(next: Preferences, cx: &mut App) {
    let state = cx.global_mut::<SettingsGlobal>();
    if state.saving {
        return;
    }
    state.saving = true;
    state.error = None;
    let path = state.path.clone();
    let previous = state.preferences.clone();
    cx.spawn(async move |cx| {
        let result = cx
            .background_spawn(async move {
                let old = previous.launcher_hotkey();
                let new = next.launcher_hotkey();
                let changed = old != new;
                if changed {
                    crate::native::set_launcher_hotkey(new)?;
                }
                let write = write_preferences(&path, &next);
                if write.is_err() && changed {
                    let _ = crate::native::set_launcher_hotkey(old);
                }
                write.map(|()| next)
            })
            .await;
        cx.update(|cx| {
            let state = cx.global_mut::<SettingsGlobal>();
            state.saving = false;
            match result {
                Ok(next) => {
                    state.preferences = next;
                    let light = state.preferences.light;
                    crate::theme::set_mode(light, cx);
                }
                Err(error) => {
                    state.error = Some(format!("{} {error}", crate::ui_text::settings::SAVE_FAILED))
                }
            }
            cx.refresh_windows();
        });
    })
    .detach();
}

pub fn open_preferences_folder(cx: &mut App) {
    let folder = cx
        .global::<SettingsGlobal>()
        .path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    cx.spawn(async move |cx| {
        let result = cx
            .background_spawn(async move {
                std::fs::create_dir_all(&folder)?;
                crate::native::launch_path(&folder);
                Ok::<_, std::io::Error>(())
            })
            .await;
        if let Err(error) = result {
            cx.update(|cx| {
                cx.global_mut::<SettingsGlobal>().error = Some(format!(
                    "{} {error}",
                    crate::ui_text::settings::OPEN_FOLDER_FAILED
                ));
                cx.refresh_windows();
            });
        }
    })
    .detach();
}

fn write_preferences(path: &std::path::Path, preferences: &Preferences) -> anyhow::Result<()> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        std::fs::create_dir_all(parent)?;
    }
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, preferences.encode())?;
    // ReplaceFile/rename behavior differs on Windows: MoveFileEx replaces the
    // existing file atomically so an interrupted save cannot truncate it.
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let from: Vec<u16> = temporary.as_os_str().encode_wide().chain(Some(0)).collect();
        let to: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe {
            windows_sys::Win32::Storage::FileSystem::MoveFileExW(
                from.as_ptr(),
                to.as_ptr(),
                windows_sys::Win32::Storage::FileSystem::MOVEFILE_REPLACE_EXISTING
                    | windows_sys::Win32::Storage::FileSystem::MOVEFILE_WRITE_THROUGH,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error().into());
        }
    }
    #[cfg(not(windows))]
    std::fs::rename(temporary, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atomic_replacement_and_failed_write_preserve_last_preferences() {
        let root =
            std::env::temp_dir().join(format!("hayai-settings-write-{}", std::process::id()));
        let path = root.join("settings.json");
        let mut preferences = Preferences::default();
        write_preferences(&path, &preferences).unwrap();
        preferences.light = true;
        write_preferences(&path, &preferences).unwrap();
        assert!(
            Preferences::parse(&std::fs::read_to_string(&path).unwrap())
                .unwrap()
                .light
        );
        let temporary = path.with_extension("json.tmp");
        std::fs::create_dir(&temporary).unwrap();
        preferences.light = false;
        assert!(write_preferences(&path, &preferences).is_err());
        assert!(
            Preferences::parse(&std::fs::read_to_string(&path).unwrap())
                .unwrap()
                .light
        );
        std::fs::remove_dir(temporary).unwrap();
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
