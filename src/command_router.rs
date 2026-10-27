//! User-facing aliases are separate from internal provider query prefixes.

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Aliases {
    names: [Vec<String>; Command::ALL.len()],
}

impl Default for Aliases {
    fn default() -> Self {
        Self {
            names: Command::ALL.map(|command| {
                if command == Command::Calculator {
                    vec!["=".into(), "calculate".into()]
                } else {
                    vec![command.id().to_owned()]
                }
            }),
        }
    }
}

impl Aliases {
    pub fn names(&self, command: Command) -> &[String] {
        &self.names[command as usize]
    }

    pub fn set(&mut self, command: Command, names: &str) -> anyhow::Result<()> {
        let mut next = self.clone();
        next.names[command as usize] = names
            .split(',')
            .take(9) // One extra entry detects overflow without an unbounded list.
            .map(|name| name.trim().to_ascii_lowercase())
            .collect();
        next.validate()?;
        *self = next;
        Ok(())
    }

    fn validate(&mut self) -> anyhow::Result<()> {
        use crate::ui_text::settings as text;
        for names in &mut self.names {
            anyhow::ensure!(!names.is_empty() && names.len() <= 8, text::ALIAS_LIMIT);
            for name in names.iter() {
                anyhow::ensure!(
                    name == "="
                        || (!name.is_empty()
                            && name.len() <= 32
                            && name.as_bytes()[0].is_ascii_alphabetic()
                            && name
                                .bytes()
                                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')),
                    text::ALIAS_FORMAT
                );
            }
            names.sort();
            names.dedup();
        }
        for command in Command::ALL {
            for name in self.names(command) {
                for other in Command::ALL.into_iter().filter(|other| *other != command) {
                    anyhow::ensure!(
                        self.names(other)
                            .iter()
                            .all(|candidate| !candidate.starts_with(name)
                                && !name.starts_with(candidate)),
                        text::ALIAS_CONFLICT
                    );
                }
            }
        }
        Ok(())
    }

    /// The shortest prefix identifying a destination, rather than a spelling.
    /// Aliases for the same command can share a trigger. No timer or Enter is
    /// involved, and a complete alias may never shadow another destination.
    pub fn trigger<'a>(&self, command: Command, name: &'a str) -> &'a str {
        for length in 1..=name.len() {
            let prefix = &name[..length]; // Validated aliases are ASCII.
            if Command::ALL
                .into_iter()
                .filter(|other| *other != command)
                .all(|other| {
                    self.names(other)
                        .iter()
                        .all(|candidate| !candidate.starts_with(prefix))
                })
            {
                return prefix;
            }
        }
        unreachable!("aliases are validated on load and edit")
    }

    pub fn triggers(&self, command: Command) -> Vec<&str> {
        let mut triggers: Vec<_> = self
            .names(command)
            .iter()
            .map(|name| self.trigger(command, name))
            .collect();
        triggers.sort_unstable();
        triggers.dedup();
        triggers
    }

    pub fn resolve<'a>(&self, input: &'a str) -> Option<(Command, &'a str)> {
        // Pasting a complete alias plus a query consumes the complete token;
        // ordinary key-by-key typing activates at the displayed short trigger.
        if input.starts_with(char::is_whitespace) {
            return None;
        }
        let token = input.split_whitespace().next()?;
        for command in Command::ALL {
            if self
                .names(command)
                .iter()
                .any(|name| name.eq_ignore_ascii_case(token))
            {
                return Some((command, input.get(token.len()..)?.trim_start()));
            }
        }
        for command in Command::ALL {
            for name in self.names(command) {
                let trigger = self.trigger(command, name);
                if input
                    .get(..trigger.len())
                    .is_some_and(|prefix| prefix.eq_ignore_ascii_case(trigger))
                {
                    return Some((command, input[trigger.len()..].trim_start()));
                }
            }
        }
        None
    }

    pub(super) fn parse(json: &serde_json::Value) -> anyhow::Result<Self> {
        let mut result = Self::default();
        if json.is_null() {
            return Ok(result);
        }
        anyhow::ensure!(json.is_object(), crate::ui_text::settings::ALIAS_FORMAT);
        for command in Command::ALL {
            if let Some(value) = json.get(command.id()) {
                anyhow::ensure!(
                    value
                        .as_array()
                        .is_some_and(|names| !names.is_empty() && names.len() <= 8),
                    crate::ui_text::settings::ALIAS_LIMIT
                );
                result.names[command as usize] = value
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!(crate::ui_text::settings::ALIAS_FORMAT))?
                    .iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(|name| name.trim().to_ascii_lowercase())
                            .ok_or_else(|| anyhow::anyhow!(crate::ui_text::settings::ALIAS_FORMAT))
                    })
                    .collect::<anyhow::Result<_>>()?;
            }
        }
        result.validate()?;
        Ok(result)
    }

    pub(super) fn encode(&self) -> serde_json::Value {
        serde_json::Value::Object(
            Command::ALL
                .into_iter()
                .map(|command| {
                    (
                        command.id().to_owned(),
                        serde_json::json!(self.names(command)),
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
    fn shared_initial_waits_until_commands_can_be_distinguished() {
        let mut aliases = Aliases::default();
        aliases.set(Command::Settings, "favorites").unwrap();
        assert_eq!(aliases.resolve("f"), None);
        assert_eq!(aliases.resolve("fi"), Some((Command::Files, "")));
        assert_eq!(aliases.resolve("fa"), Some((Command::Settings, "")));
    }

    #[test]
    fn aliases_share_destinations_and_preserve_queries() {
        let mut aliases = Aliases::default();
        aliases.set(Command::Files, "files, find").unwrap();
        assert_eq!(aliases.trigger(Command::Files, "files"), "f");
        assert_eq!(
            aliases.resolve("find report"),
            Some((Command::Files, "report"))
        );
        assert_eq!(aliases.resolve("Freport"), Some((Command::Files, "report")));
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
    fn unreachable_aliases_are_rejected_without_changing_the_router() {
        let mut aliases = Aliases::default();
        for invalid in ["f", "files", "files-extra", "", "has space", "=", "日本語"] {
            let before = aliases.clone();
            assert!(
                aliases.set(Command::Settings, invalid).is_err(),
                "{invalid}"
            );
            assert_eq!(aliases, before);
        }
        aliases
            .set(Command::Settings, "Folders, preferences")
            .unwrap();
        assert_eq!(aliases.resolve("f"), None);
        assert_eq!(aliases.resolve("fi"), Some((Command::Files, "")));
        assert_eq!(aliases.resolve("fo"), Some((Command::Settings, "")));
        assert_eq!(Aliases::parse(&aliases.encode()).unwrap(), aliases);
        assert!(Aliases::parse(&serde_json::json!({"files": [false]})).is_err());
        assert!(Aliases::parse(&serde_json::json!({"files": []})).is_err());
    }
}
