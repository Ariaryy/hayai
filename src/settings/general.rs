//! General preferences are independent of theme and shortcut customization.
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DisplayTarget {
    #[default]
    Pointer,
    ActiveWindow,
    Primary,
}
impl DisplayTarget {
    pub const ALL: [Self; 3] = [Self::Pointer, Self::ActiveWindow, Self::Primary];
    pub fn id(self) -> &'static str {
        match self {
            Self::Pointer => "pointer",
            Self::ActiveWindow => "active_window",
            Self::Primary => "primary",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct General {
    pub launch_at_login: bool,
    pub display: DisplayTarget,
    pub remember_recents: bool,
    pub check_updates: bool,
    pub download_updates: bool,
}
impl Default for General {
    fn default() -> Self {
        Self {
            launch_at_login: false,
            display: DisplayTarget::Pointer,
            remember_recents: true,
            check_updates: true,
            download_updates: true,
        }
    }
}
impl General {
    pub(super) fn parse(value: &Value) -> anyhow::Result<Self> {
        anyhow::ensure!(
            value.is_null() || value.is_object(),
            "General settings must be an object"
        );
        let defaults = Self::default();
        let boolean = |key: &str, fallback| -> anyhow::Result<bool> {
            match &value[key] {
                Value::Null => Ok(fallback),
                Value::Bool(v) => Ok(*v),
                _ => anyhow::bail!("Invalid general setting: {key}"),
            }
        };
        let display = match value["display"].as_str() {
            None if value["display"].is_null() => DisplayTarget::Pointer,
            Some("pointer") => DisplayTarget::Pointer,
            Some("active_window") => DisplayTarget::ActiveWindow,
            Some("primary") => DisplayTarget::Primary,
            _ => anyhow::bail!("Unknown launcher display"),
        };
        Ok(Self {
            launch_at_login: boolean("launch_at_login", defaults.launch_at_login)?,
            display,
            remember_recents: boolean("remember_recents", defaults.remember_recents)?,
            check_updates: boolean("check_updates", defaults.check_updates)?,
            download_updates: boolean("download_updates", defaults.download_updates)?,
        })
    }
    pub(super) fn encode(&self) -> Value {
        json!({ "launch_at_login": self.launch_at_login, "display": self.display.id(), "remember_recents": self.remember_recents, "check_updates": self.check_updates, "download_updates": self.download_updates })
    }
}
