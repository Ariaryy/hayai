//! Preferences and the lazily opened settings window. Persistence and native
//! registration complete before publishing a preference change to the UI.
mod persistence;
mod preferences;
mod window;

use gpui::{App, Global};
use std::path::PathBuf;

pub use preferences::{Preferences, Shortcut};
pub use window::{capture_registered_shortcut, key_badges, open};

pub struct SettingsGlobal {
    pub preferences: Preferences,
    pub error: Option<String>,
    pub saving: bool,
    pub window: Option<gpui::AnyWindowHandle>,
    view: Option<gpui::WeakEntity<window::SettingsView>>,
    path: PathBuf,
}

impl Global for SettingsGlobal {}

pub fn preferences(cx: &App) -> &Preferences {
    &cx.global::<SettingsGlobal>().preferences
}

pub use persistence::save;
