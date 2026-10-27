//! User-facing triggers and aliases are separate from internal provider query
//! prefixes. A trigger is the short key sequence typed after Space; aliases are
//! complete names that also select the command. Overlapping entries across
//! commands are saved and flagged, but stay inactive until the user resolves
//! them, so the router never guesses between two destinations.

use crate::ui_text::settings as text;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Files,
    Settings,
    Calculator,
}

impl Command {
    pub const ALL: [Self; 3] = [Self::Files, Self::Settings, Self::Calculator];

    pub fn id(self) -> &'static str {
        match self {
            Self::Files => "files",
            Self::Settings => "settings",
            Self::Calculator => "calculate",
        }
    }

    pub fn icon(self) -> &'static str {
        use crate::assets::icons;
        match self {
            Self::Files => icons::SETTINGS_FILES,
            Self::Settings => icons::SETTINGS_GENERAL,
            Self::Calculator => icons::CALCULATOR,
        }
    }

    fn default_trigger(self) -> &'static str {
        match self {
            Self::Files => "f",
            Self::Settings => "s",
            Self::Calculator => "=",
        }
    }
}

pub const MAX_ALIASES: usize = 8;

#[derive(Clone, Debug, PartialEq, Eq)]
struct Route {
    trigger: String,
    aliases: Vec<String>,
}

impl Route {
    fn entries(&self) -> impl Iterator<Item = &str> {
        std::iter::once(self.trigger.as_str()).chain(self.aliases.iter().map(String::as_str))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aliases {
    routes: [Route; Command::ALL.len()],
}

impl Default for Aliases {
    fn default() -> Self {
        Self {
            routes: Command::ALL.map(|command| Route {
                trigger: command.default_trigger().to_owned(),
                aliases: vec![command.id().to_owned()],
            }),
        }
    }
}

fn normalize_trigger(trigger: &str) -> anyhow::Result<String> {
    let trigger = trigger.trim().to_ascii_lowercase();
    anyhow::ensure!(
        (1..=4).contains(&trigger.len()) && trigger.bytes().all(|byte| byte.is_ascii_graphic()),
        text::TRIGGER_FORMAT
    );
    Ok(trigger)
}

fn normalize_aliases<'a>(names: impl Iterator<Item = &'a str>) -> anyhow::Result<Vec<String>> {
    let mut aliases: Vec<String> = names
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .take(MAX_ALIASES + 1) // One extra entry detects overflow without an unbounded list.
        .collect();
    anyhow::ensure!(aliases.len() <= MAX_ALIASES, text::ALIAS_LIMIT);
    for name in &aliases {
        anyhow::ensure!(
            name.len() <= 32
                && name.as_bytes()[0].is_ascii_alphabetic()
                && name
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-'),
            text::ALIAS_FORMAT
        );
    }
    aliases.sort();
    aliases.dedup();
    Ok(aliases)
}

/// Typing either entry would act on the other's command first.
fn overlaps(a: &str, b: &str) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

impl Aliases {
    pub fn trigger(&self, command: Command) -> &str {
        &self.routes[command as usize].trigger
    }

    pub fn aliases(&self, command: Command) -> &[String] {
        &self.routes[command as usize].aliases
    }

    pub fn entries(&self, command: Command) -> impl Iterator<Item = &str> {
        self.routes[command as usize].entries()
    }

    pub fn set_trigger(&mut self, command: Command, trigger: &str) -> anyhow::Result<()> {
        self.routes[command as usize].trigger = normalize_trigger(trigger)?;
        Ok(())
    }

    pub fn set_aliases(&mut self, command: Command, names: &str) -> anyhow::Result<()> {
        self.routes[command as usize].aliases = normalize_aliases(names.split(','))?;
        Ok(())
    }

    /// The first other command whose trigger or alias overlaps `entry`.
    pub fn conflict(&self, command: Command, entry: &str) -> Option<(Command, &str)> {
        Command::ALL
            .into_iter()
            .filter(|other| *other != command)
            .find_map(|other| {
                self.entries(other)
                    .find(|candidate| overlaps(entry, candidate))
                    .map(|candidate| (other, candidate))
            })
    }

    fn active(&self, command: Command, entry: &str) -> bool {
        self.conflict(command, entry).is_none()
    }

    /// Whether `typed` could still become an active trigger or alias. Text that
    /// cannot is an ordinary search that merely began with Space.
    pub fn could_match(&self, typed: &str) -> bool {
        let typed = typed.to_ascii_lowercase();
        !typed.contains(char::is_whitespace)
            && Command::ALL.into_iter().any(|command| {
                self.entries(command)
                    .any(|entry| entry.starts_with(&typed) && self.active(command, entry))
            })
    }

    pub fn resolve<'a>(&self, input: &'a str) -> Option<(Command, &'a str)> {
        if input.starts_with(char::is_whitespace) {
            return None;
        }
        // A complete alias wins over the trigger it may start with, so
        // "files report" keeps its query. Triggers activate key by key.
        let token = input.split_whitespace().next()?;
        for command in Command::ALL {
            if self
                .aliases(command)
                .iter()
                .any(|name| name.eq_ignore_ascii_case(token) && self.active(command, name))
            {
                return Some((command, input.get(token.len()..)?.trim_start()));
            }
        }
        for command in Command::ALL {
            let trigger = self.trigger(command);
            if self.active(command, trigger)
                && input
                    .get(..trigger.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(trigger))
            {
                return Some((command, input[trigger.len()..].trim_start()));
            }
        }
        None
    }

    pub(super) fn parse(json: &serde_json::Value) -> anyhow::Result<Self> {
        let mut result = Self::default();
        if json.is_null() {
            return Ok(result);
        }
        anyhow::ensure!(json.is_object(), text::ALIAS_FORMAT);
        for command in Command::ALL {
            let Some(route) = json.get(command.id()) else {
                continue;
            };
            anyhow::ensure!(route.is_object(), text::ALIAS_FORMAT);
            if let Some(trigger) = route.get("trigger") {
                let trigger = trigger
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!(text::TRIGGER_FORMAT))?;
                result.set_trigger(command, trigger)?;
            }
            if let Some(names) = route.get("aliases") {
                let names = names
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!(text::ALIAS_FORMAT))?
                    .iter()
                    .map(|name| {
                        name.as_str()
                            .ok_or_else(|| anyhow::anyhow!(text::ALIAS_FORMAT))
                    })
                    .collect::<anyhow::Result<Vec<_>>>()?;
                result.routes[command as usize].aliases = normalize_aliases(names.into_iter())?;
            }
        }
        Ok(result)
    }

    pub(super) fn encode(&self) -> serde_json::Value {
        serde_json::Value::Object(
            Command::ALL
                .into_iter()
                .map(|command| {
                    (
                        command.id().to_owned(),
                        serde_json::json!({
                            "trigger": self.trigger(command),
                            "aliases": self.aliases(command),
                        }),
                    )
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn triggers_activate_on_the_first_key_and_aliases_on_the_whole_word() {
        let mut aliases = Aliases::default();
        aliases.set_aliases(Command::Files, "files, find").unwrap();
        assert_eq!(aliases.resolve("f"), Some((Command::Files, "")));
        assert_eq!(aliases.resolve("Freport"), Some((Command::Files, "report")));
        assert_eq!(
            aliases.resolve("find report"),
            Some((Command::Files, "report"))
        );
        assert_eq!(aliases.resolve("s"), Some((Command::Settings, "")));
        assert_eq!(aliases.resolve(""), None);
        assert_eq!(aliases.resolve("unknown"), None);
        assert_eq!(aliases.resolve("日"), None);
        assert_eq!(aliases.resolve(" files report"), None);
        assert_eq!(aliases.resolve("=2+2"), Some((Command::Calculator, "2+2")));
        assert_eq!(
            aliases.resolve("calculate 2+2"),
            Some((Command::Calculator, "2+2"))
        );
    }

    #[test]
    fn text_that_cannot_become_a_command_is_a_search() {
        let aliases = Aliases::default();
        for possible in ["f", "fi", "FILES", "calc", "="] {
            assert!(aliases.could_match(possible), "{possible}");
        }
        for plain in ["chrome", "files x", "fx", "settingsx"] {
            assert!(!aliases.could_match(plain), "{plain}");
        }
    }

    #[test]
    fn triggers_can_be_changed() {
        let mut aliases = Aliases::default();
        aliases.set_trigger(Command::Settings, "x").unwrap();
        aliases.set_trigger(Command::Calculator, "c").unwrap();
        assert_eq!(aliases.resolve("x"), Some((Command::Settings, "")));
        assert_eq!(aliases.resolve("s"), None);
        assert_eq!(aliases.resolve("c1+1"), Some((Command::Calculator, "1+1")));
        for invalid in ["", "has space", "toolong", "日"] {
            assert!(aliases.set_trigger(Command::Files, invalid).is_err());
        }
        assert_eq!(aliases.trigger(Command::Files), "f");
    }

    #[test]
    fn conflicts_are_kept_flagged_and_inactive() {
        let mut aliases = Aliases::default();
        aliases.set_trigger(Command::Settings, "f").unwrap();
        assert_eq!(
            aliases.conflict(Command::Settings, "f"),
            Some((Command::Files, "f"))
        );
        assert_eq!(
            aliases.conflict(Command::Files, "f"),
            Some((Command::Settings, "f"))
        );
        // Neither destination is guessed while both claim the key.
        assert_eq!(aliases.resolve("f"), None);
        assert_eq!(aliases.resolve("files"), None);
        aliases.set_trigger(Command::Settings, "t").unwrap();
        assert_eq!(aliases.resolve("f"), Some((Command::Files, "")));

        // An alias starting with another command's trigger is unreachable.
        aliases.set_aliases(Command::Settings, "folders").unwrap();
        assert_eq!(
            aliases.conflict(Command::Settings, "folders"),
            Some((Command::Files, "f"))
        );
        assert_eq!(aliases.resolve("folders"), None);
        assert_eq!(aliases.resolve("f"), None);
    }

    #[test]
    fn aliases_are_normalized_and_roundtrip() {
        let mut aliases = Aliases::default();
        aliases
            .set_aliases(Command::Settings, " Preferences, prefs,, prefs ")
            .unwrap();
        assert_eq!(aliases.aliases(Command::Settings), ["preferences", "prefs"]);
        aliases.set_aliases(Command::Settings, "").unwrap();
        assert!(aliases.aliases(Command::Settings).is_empty());
        for invalid in ["has space", "1abc", "日本語", "a, b, c, d, e, f, g, h, i"] {
            assert!(
                aliases.set_aliases(Command::Files, invalid).is_err(),
                "{invalid}"
            );
        }
        aliases.set_aliases(Command::Settings, "options").unwrap();
        aliases.set_trigger(Command::Settings, "o").unwrap();
        assert_eq!(Aliases::parse(&aliases.encode()).unwrap(), aliases);
        assert!(Aliases::parse(&serde_json::json!({"files": ["files"]})).is_err());
        assert!(Aliases::parse(&serde_json::json!({"files": {"aliases": [false]}})).is_err());
        assert!(Aliases::parse(&serde_json::json!({"files": {"trigger": ""}})).is_err());
    }
}
