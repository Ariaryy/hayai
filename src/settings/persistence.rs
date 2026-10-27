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
        let (mut preferences, error) = match std::fs::read_to_string(&path) {
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
        if let Ok(registration) = crate::startup::registration() {
            preferences.general.launch_at_login = registration.is_some();
        }
        Self {
            preferences,
            error,
            saving: false,
            desired: None,
            pending_clear: false,
            window: None,
            view: None,
            path,
        }
    }
}

pub fn save(next: Preferences, cx: &mut App) {
    save_inner(next, false, cx);
}

pub fn clear_history(cx: &mut App) {
    let next = super::display_preferences(cx).clone();
    save_inner(next, true, cx);
}

fn clear_history_memory(cx: &mut App) {
    if let Some(catalog) = cx.try_global::<crate::apps::CatalogGlobal>() {
        catalog.clone_handle().borrow_mut().clear_recents();
    }
    if let Some(recents) = cx.try_global::<crate::files::FileRecentsGlobal>() {
        recents.clone_handle().borrow_mut().clear();
    }
    crate::launcher::LauncherState::clear_recent_history(cx);
}

fn notify_settings(cx: &mut App) {
    let view = cx.global::<SettingsGlobal>().view.clone();
    // A save may originate inside this view's listener. Defer its notification
    // so no nested entity update re-enters an already borrowed SettingsView.
    cx.defer(move |cx| {
        if let Some(view) = view {
            let _ = view.update(cx, |_, cx| cx.notify());
        }
    });
}

fn save_inner(next: Preferences, clear_requested: bool, cx: &mut App) {
    let state = cx.global_mut::<SettingsGlobal>();
    let previous = state.desired.as_ref().unwrap_or(&state.preferences);
    let clear =
        clear_requested || (previous.general.remember_recents && !next.general.remember_recents);
    state.desired = Some(next);
    state.pending_clear |= clear;
    state.error = None;
    if clear {
        // Invalidate snapshots before any queued worker can restore old recents.
        // Rapid off/on edits still retain the clear request until it completes.
        crate::history::configure(false);
        clear_history_memory(cx);
    }
    if !cx.global::<SettingsGlobal>().saving {
        start_save(cx);
    }
    notify_settings(cx);
}

fn start_save(cx: &mut App) {
    let state = cx.global_mut::<SettingsGlobal>();
    let Some(next) = state.desired.clone() else {
        return;
    };
    let clear = std::mem::take(&mut state.pending_clear);
    if next == state.preferences && !clear {
        state.desired = None;
        return;
    }
    state.saving = true;
    let path = state.path.clone();
    let previous = state.preferences.clone();
    let picker_changed = previous.show_command_picker != next.show_command_picker;
    let file_search_changed = previous.file_search != next.file_search;
    let theme_changed = previous.light != next.light
        || previous.theme_for(next.light) != next.theme_for(next.light);
    cx.spawn(async move |cx| {
        let result = cx
            .background_spawn(async move {
                apply_preferences(&path, &previous, &next, clear).map(|()| next)
            })
            .await;
        cx.update(|cx| {
            let state = cx.global_mut::<SettingsGlobal>();
            state.saving = false;
            match result {
                Ok(next) => {
                    if state.desired.as_ref() == Some(&next) {
                        state.desired = None;
                    }
                    state.preferences = next;
                    let remember = state.preferences.general.remember_recents
                        && !state.pending_clear
                        && state
                            .desired
                            .as_ref()
                            .is_none_or(|prefs| prefs.general.remember_recents);
                    crate::history::configure(remember);
                    crate::updates::configure(&state.preferences.general);
                    let light = state.preferences.light;
                    if theme_changed {
                        crate::theme::set_mode(light, cx);
                    }
                    if picker_changed {
                        crate::launcher::LauncherState::refresh_command_picker(cx);
                    }
                    if file_search_changed {
                        if let Some(registry) = cx
                            .try_global::<crate::plugins::RegistryGlobal>()
                            .map(crate::plugins::RegistryGlobal::clone_handle)
                        {
                            registry
                                .borrow_mut()
                                .configure(crate::settings::preferences(cx));
                        }
                        crate::launcher::LauncherState::refresh_file_search(cx);
                    }
                }
                Err(error) => {
                    state.desired = None;
                    state.pending_clear = false;
                    state.error =
                        Some(format!("{} {error}", crate::ui_text::settings::SAVE_FAILED));
                    crate::history::configure(state.preferences.general.remember_recents);
                }
            }
            if cx.global::<SettingsGlobal>().desired.is_some() {
                start_save(cx);
            }
            notify_settings(cx);
        });
    })
    .detach();
}

fn apply_preferences(
    path: &std::path::Path,
    previous: &Preferences,
    next: &Preferences,
    clear: bool,
) -> anyhow::Result<()> {
    let old = previous.launcher_hotkey();
    let new = next.launcher_hotkey();
    let hotkey_changed = old != new;
    let startup_changed = previous.general.launch_at_login != next.general.launch_at_login;
    let registration = if startup_changed {
        crate::startup::registration()?
    } else {
        None
    };
    if hotkey_changed {
        crate::native::set_launcher_hotkey(new)?;
    }
    let result = (|| {
        if startup_changed {
            if next.general.launch_at_login {
                crate::startup::register()?;
            } else {
                crate::startup::unregister()?;
            }
        }
        if clear {
            crate::history::clear()?;
        }
        write_preferences(path, next)
    })();
    if result.is_err() {
        if hotkey_changed {
            let _ = crate::native::set_launcher_hotkey(old);
        }
        if startup_changed {
            let _ = crate::startup::restore(registration.as_deref());
        }
    }
    result
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
