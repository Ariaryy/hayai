use gpui::Keystroke;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shortcut {
    ToggleLauncher,
    Actions,
    FileSearch,
    Settings,
}

impl Shortcut {
    pub const ALL: [Self; 4] = [
        Self::ToggleLauncher,
        Self::Actions,
        Self::FileSearch,
        Self::Settings,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Self::ToggleLauncher => "toggle_launcher",
            Self::Actions => "actions",
            Self::FileSearch => "file_search",
            Self::Settings => "settings",
        }
    }

    pub fn default_key(self) -> &'static str {
        match self {
            Self::ToggleLauncher => "alt-space",
            Self::Actions => "ctrl-k",
            Self::FileSearch => "ctrl-f",
            Self::Settings => "ctrl-,",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Binding {
    text: String,
    keystroke: Keystroke,
    /// Another action sharing this key. Derived by `refresh_conflicts`.
    conflict: Option<Shortcut>,
}

impl Binding {
    fn parse(text: &str) -> anyhow::Result<Self> {
        let keystroke = Keystroke::parse(text)?;
        Ok(Self {
            text: keystroke.unparse(),
            keystroke,
            conflict: None,
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Preferences {
    pub light: bool,
    pub general: super::General,
    pub file_search: super::FileSearch,
    pub commands: crate::command_router::Aliases,
    pub show_command_picker: bool,
    light_theme: crate::theme::ThemeFamily,
    dark_theme: crate::theme::ThemeFamily,
    bindings: [Binding; 4],
}

impl Default for Preferences {
    fn default() -> Self {
        Self {
            light: false,
            general: super::General::default(),
            file_search: super::FileSearch::default(),
            commands: crate::command_router::Aliases::default(),
            show_command_picker: true,
            light_theme: crate::theme::ThemeFamily::default(),
            dark_theme: crate::theme::ThemeFamily::default(),
            bindings: Shortcut::ALL.map(|action| Binding::parse(action.default_key()).unwrap()),
        }
    }
}

impl Preferences {
    pub fn theme_for(&self, light: bool) -> crate::theme::ThemeFamily {
        if light {
            self.light_theme
        } else {
            self.dark_theme
        }
    }

    pub fn set_theme_for(&mut self, light: bool, family: crate::theme::ThemeFamily) {
        if light {
            self.light_theme = family;
        } else {
            self.dark_theme = family;
        }
    }

    pub fn binding(&self, action: Shortcut) -> &str {
        &self.bindings[action as usize].text
    }

    pub fn launcher_hotkey(&self) -> (u32, u32) {
        native_hotkey(&self.bindings[Shortcut::ToggleLauncher as usize].keystroke)
            .expect("launcher binding is validated when preferences are loaded or changed")
    }

    /// The other action sharing this action's key. Conflicts are saved and
    /// shown, not rejected; a conflicted launcher shortcut stays inactive until
    /// one side changes. The system-wide launcher hotkey is registered with
    /// Windows, so it keeps working and only disables the launcher action.
    pub fn binding_conflict(&self, action: Shortcut) -> Option<Shortcut> {
        self.bindings[action as usize].conflict
    }

    pub fn matches(&self, action: Shortcut, key: &Keystroke) -> bool {
        // Matching is on every input event: parse once when loading/saving,
        // and ignore key_char (typed text metadata is not shortcut identity).
        let binding = &self.bindings[action as usize];
        binding.conflict.is_none()
            && binding.keystroke.key == key.key
            && binding.keystroke.modifiers == key.modifiers
    }

    fn refresh_conflicts(&mut self) {
        let conflicts = Shortcut::ALL.map(|action| {
            let key = &self.bindings[action as usize].keystroke;
            Shortcut::ALL.into_iter().find(|&other| {
                let candidate = &self.bindings[other as usize].keystroke;
                other != action && candidate.key == key.key && candidate.modifiers == key.modifiers
            })
        });
        for (binding, conflict) in self.bindings.iter_mut().zip(conflicts) {
            binding.conflict = conflict;
        }
    }

    pub fn set_binding(&mut self, action: Shortcut, binding: &str) -> anyhow::Result<()> {
        let key = Keystroke::parse(binding)?;
        // Launcher shortcuts must not consume ordinary typing or the input's
        // editing/navigation keys. Escape remains an unconditional way out.
        anyhow::ensure!(
            key.modifiers.control || key.modifiers.alt || key.modifiers.platform,
            crate::ui_text::settings::MODIFIER_REQUIRED
        );
        anyhow::ensure!(
            !matches!(
                key.key.as_str(),
                "escape"
                    | "enter"
                    | "tab"
                    | "backspace"
                    | "delete"
                    | "up"
                    | "down"
                    | "left"
                    | "right"
                    | "control"
                    | "shift"
                    | "alt"
                    | "platform"
                    | "function"
                    | ""
            ),
            crate::ui_text::settings::RESERVED_KEY
        );
        anyhow::ensure!(
            !(key.modifiers.alt && key.key == "f4"),
            crate::ui_text::settings::RESERVED_KEY
        );
        if action == Shortcut::ToggleLauncher {
            anyhow::ensure!(
                native_hotkey(&key).is_some(),
                crate::ui_text::settings::GLOBAL_SUPPORTED
            );
        }
        self.bindings[action as usize] = Binding {
            text: key.unparse(),
            keystroke: key,
            conflict: None,
        };
        self.refresh_conflicts();
        Ok(())
    }

    pub(super) fn parse(contents: &str) -> anyhow::Result<Self> {
        let json: serde_json::Value = serde_json::from_str(contents)?;
        anyhow::ensure!(json["version"] == 1, "Unsupported settings version");
        let light = match json["appearance"].as_str() {
            Some("light") => true,
            Some("dark") | None => false,
            _ => anyhow::bail!("Unknown appearance"),
        };
        let legacy_theme = match json["theme"].as_str() {
            Some(id) => crate::theme::ThemeFamily::from_id(id)
                .ok_or_else(|| anyhow::anyhow!("Unknown theme"))?,
            None => crate::theme::ThemeFamily::default(),
        };
        let theme_for = |mode: &str| -> anyhow::Result<crate::theme::ThemeFamily> {
            match json["themes"][mode].as_str() {
                Some(id) => crate::theme::ThemeFamily::from_id(id)
                    .ok_or_else(|| anyhow::anyhow!("Unknown theme")),
                None => Ok(legacy_theme),
            }
        };
        let mut result = Self {
            light,
            general: super::General::parse(&json["general"])?,
            file_search: super::FileSearch::parse(&json["file_search"])?,
            commands: crate::command_router::Aliases::parse(&json["commands"])?,
            show_command_picker: match json["show_command_picker"] {
                serde_json::Value::Null => true,
                serde_json::Value::Bool(show) => show,
                _ => anyhow::bail!("Invalid command picker visibility"),
            },
            light_theme: theme_for("light")?,
            dark_theme: theme_for("dark")?,
            ..Self::default()
        };
        for action in Shortcut::ALL {
            if let Some(binding) = json["keybindings"][action.id()].as_str() {
                result.set_binding(action, binding)?;
            }
        }
        Ok(result)
    }

    pub(super) fn encode(&self) -> String {
        let bindings: serde_json::Map<String, serde_json::Value> = Shortcut::ALL
            .into_iter()
            .map(|action| (action.id().into(), self.binding(action).into()))
            .collect();
        serde_json::json!({"version": 1, "appearance": if self.light { "light" } else { "dark" }, "themes": {"light": self.light_theme.id(), "dark": self.dark_theme.id()}, "general": self.general.encode(), "file_search": self.file_search.encode(), "commands": self.commands.encode(), "show_command_picker": self.show_command_picker, "keybindings": bindings}).to_string()
    }
}

/// Win32 modifier/VK encoding, deliberately restricted to layout-independent
/// letters, digits, function keys and Space for the system-wide shortcut.
fn native_hotkey(key: &Keystroke) -> Option<(u32, u32)> {
    if key.modifiers.function {
        return None;
    }
    let modifiers = u32::from(key.modifiers.alt)
        | (u32::from(key.modifiers.control) << 1)
        | (u32::from(key.modifiers.shift) << 2)
        | (u32::from(key.modifiers.platform) << 3);
    let vk = match key.key.as_str() {
        "space" => 0x20,
        key if key.len() == 1 && key.as_bytes()[0].is_ascii_alphanumeric() => {
            key.as_bytes()[0].to_ascii_uppercase() as u32
        }
        key => {
            key.strip_prefix('f')?
                .parse::<u32>()
                .ok()
                .filter(|n| (1..=24).contains(n))?
                + 0x6f
        }
    };
    Some((modifiers, vk))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn settings_roundtrip_and_conflicts() {
        let mut prefs = Preferences {
            light: true,
            ..Preferences::default()
        };
        prefs
            .set_binding(Shortcut::Actions, "ctrl-shift-k")
            .unwrap();
        assert_eq!(Preferences::parse(&prefs.encode()).unwrap(), prefs);
        assert!(prefs.set_binding(Shortcut::Actions, "k").is_err());
        assert!(
            prefs
                .set_binding(Shortcut::Actions, "ctrl-backspace")
                .is_err()
        );
        assert!(prefs.set_binding(Shortcut::Actions, "alt-f4").is_err());
        assert!(Preferences::parse("{\"version\":2}").is_err());
    }

    #[test]
    fn shared_shortcuts_are_saved_flagged_and_inactive() {
        let mut prefs = Preferences::default();
        prefs.set_binding(Shortcut::Settings, "ctrl-k").unwrap();
        assert_eq!(
            prefs.binding_conflict(Shortcut::Settings),
            Some(Shortcut::Actions)
        );
        assert_eq!(
            prefs.binding_conflict(Shortcut::Actions),
            Some(Shortcut::Settings)
        );
        let key = Keystroke::parse("ctrl-k").unwrap();
        assert!(!prefs.matches(Shortcut::Actions, &key));
        assert!(!prefs.matches(Shortcut::Settings, &key));
        let reloaded = Preferences::parse(&prefs.encode()).unwrap();
        assert_eq!(
            reloaded.binding_conflict(Shortcut::Settings),
            Some(Shortcut::Actions)
        );
        prefs.set_binding(Shortcut::Settings, "ctrl-,").unwrap();
        assert_eq!(prefs.binding_conflict(Shortcut::Actions), None);
        assert!(prefs.matches(Shortcut::Actions, &key));
        // The system hotkey still registers; only the launcher action yields.
        prefs
            .set_binding(Shortcut::ToggleLauncher, "ctrl-f")
            .unwrap();
        assert_eq!(
            prefs.binding_conflict(Shortcut::FileSearch),
            Some(Shortcut::ToggleLauncher)
        );
        assert_eq!(prefs.launcher_hotkey(), (2, b'F' as u32));
    }

    #[test]
    fn general_preferences_roundtrip_without_resetting_other_pages() {
        let legacy = Preferences::parse(r#"{"version":1}"#).unwrap();
        assert_eq!(legacy.general, super::super::General::default());
        for display in super::super::DisplayTarget::ALL {
            for enabled in [false, true] {
                let mut prefs = Preferences::default();
                prefs.general.display = display;
                prefs.general.launch_at_login = enabled;
                prefs.general.remember_recents = enabled;
                prefs.general.check_updates = enabled;
                prefs.general.download_updates = !enabled;
                prefs
                    .set_binding(Shortcut::Settings, "ctrl-shift-s")
                    .unwrap();
                assert_eq!(Preferences::parse(&prefs.encode()).unwrap(), prefs);
            }
        }
        assert!(Preferences::parse(r#"{"version":1,"general":{"display":"unknown"}}"#).is_err());
        assert!(
            Preferences::parse(r#"{"version":1,"general":{"remember_recents":"false"}}"#).is_err()
        );
    }

    #[test]
    fn command_aliases_migrate_and_roundtrip_with_other_preferences() {
        let mut prefs = Preferences::parse(r#"{"version":1}"#).unwrap();
        assert_eq!(prefs.commands, crate::command_router::Aliases::default());
        prefs
            .commands
            .set_aliases(
                crate::command_router::Command::Settings,
                "folders, preferences",
            )
            .unwrap();
        prefs
            .commands
            .set_trigger(crate::command_router::Command::Settings, "p")
            .unwrap();
        prefs.light = true;
        prefs.general.check_updates = false;
        assert_eq!(Preferences::parse(&prefs.encode()).unwrap(), prefs);
        // Overlapping entries load and are flagged rather than discarded.
        let overlapping =
            Preferences::parse(r#"{"version":1,"commands":{"settings":{"trigger":"f"}}}"#).unwrap();
        assert!(
            overlapping
                .commands
                .conflict(crate::command_router::Command::Settings, "f")
                .is_some()
        );
        assert!(Preferences::parse(r#"{"version":1,"commands":{"files":["f"]}}"#).is_err());
    }

    #[test]
    fn command_picker_visibility_is_persisted() {
        assert!(
            Preferences::parse(r#"{"version":1}"#)
                .unwrap()
                .show_command_picker
        );
        let prefs = Preferences::parse(r#"{"version":1,"show_command_picker":false}"#).unwrap();
        let saved: serde_json::Value = serde_json::from_str(&prefs.encode()).unwrap();
        assert_eq!(saved["show_command_picker"], false);
        assert!(Preferences::parse(r#"{"version":1,"show_command_picker":"false"}"#).is_err());
    }

    #[test]
    fn native_encoding_is_layout_independent() {
        assert_eq!(
            native_hotkey(&Keystroke::parse("ctrl-win-f24").unwrap()),
            Some((10, 135))
        );
        assert_eq!(
            native_hotkey(&Keystroke::parse("alt-space").unwrap()),
            Some((1, 32))
        );
        assert_eq!(
            native_hotkey(&Keystroke::parse("ctrl-shift-f12").unwrap()),
            Some((6, 123))
        );
    }

    #[test]
    fn matching_ignores_typed_text_and_loading_allows_swapped_shortcuts() {
        let preferences = Preferences::parse(
            r#"{"version":1,"keybindings":{"actions":"ctrl-f","file_search":"ctrl-k"}}"#,
        )
        .unwrap();
        let mut key = Keystroke::parse("ctrl-f").unwrap();
        key.key_char = Some("f".into());
        assert!(preferences.matches(Shortcut::Actions, &key));
        assert!(!preferences.matches(Shortcut::FileSearch, &key));
        assert!(native_hotkey(&Keystroke::parse("fn-ctrl-f").unwrap()).is_none());
    }

    #[test]
    fn theme_families_persist_independently_and_legacy_settings_migrate() {
        for light_theme in crate::theme::ThemeFamily::ALL {
            for dark_theme in crate::theme::ThemeFamily::ALL {
                for light in [false, true] {
                    let preferences = Preferences {
                        light_theme,
                        dark_theme,
                        light,
                        ..Preferences::default()
                    };
                    assert_eq!(
                        Preferences::parse(&preferences.encode()).unwrap(),
                        preferences
                    );
                }
            }
        }
        let legacy = Preferences::parse(r#"{"version":1,"appearance":"light"}"#).unwrap();
        assert_eq!(legacy.theme_for(true), crate::theme::ThemeFamily::Hayai);
        assert_eq!(legacy.theme_for(false), crate::theme::ThemeFamily::Hayai);
        assert!(Preferences::parse(r#"{"version":1,"theme":"unknown"}"#).is_err());
        let legacy = Preferences::parse(r#"{"version":1,"theme":"hayai"}"#).unwrap();
        assert_eq!(legacy.theme_for(true), legacy.theme_for(false));
    }
}
