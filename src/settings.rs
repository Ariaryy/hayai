//! Preferences and lazy settings UI. Controls can display a queued draft;
//! runtime settings publish after persistence and native registration succeed.
mod general;
mod persistence;
mod preferences;
mod window;

use gpui::{App, Global};
use std::path::PathBuf;

pub use general::{DisplayTarget, General};
pub use preferences::{Preferences, Shortcut};
pub use window::{capture_registered_shortcut, command_badges, key_badges, open};

pub struct SettingsGlobal {
    pub preferences: Preferences,
    pub error: Option<String>,
    pub saving: bool,
    // Latest requested values for controls; runtime routing uses committed preferences.
    desired: Option<Preferences>,
    pending_clear: bool,
    pub window: Option<gpui::AnyWindowHandle>,
    view: Option<gpui::WeakEntity<window::SettingsView>>,
    path: PathBuf,
}

impl Global for SettingsGlobal {}

#[cfg(test)]
impl SettingsGlobal {
    pub(crate) fn for_test(path: PathBuf) -> Self {
        Self {
            preferences: Preferences::default(),
            error: None,
            saving: false,
            desired: None,
            pending_clear: false,
            window: None,
            view: None,
            path,
        }
    }
}

pub fn preferences(cx: &App) -> &Preferences {
    &cx.global::<SettingsGlobal>().preferences
}

pub fn display_preferences(cx: &App) -> &Preferences {
    let state = cx.global::<SettingsGlobal>();
    state.desired.as_ref().unwrap_or(&state.preferences)
}

pub use persistence::{clear_history, open_preferences_folder, save};
