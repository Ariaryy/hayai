use serde_json::{Value, json};

/// Search presentation and filtering, independent of recent-history storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FileSearch {
    pub show_recents: bool,
    pub hide_noisy_folders: bool,
    pub search_subfolders: bool,
    pub folders_first: bool,
}

impl Default for FileSearch {
    fn default() -> Self {
        Self {
            show_recents: true,
            hide_noisy_folders: true,
            search_subfolders: true,
            folders_first: true,
        }
    }
}

impl FileSearch {
    pub(super) fn parse(value: &Value) -> anyhow::Result<Self> {
        anyhow::ensure!(
            value.is_null() || value.is_object(),
            "File search settings must be an object"
        );
        let boolean = |key: &str| -> anyhow::Result<bool> {
            match &value[key] {
                Value::Null => Ok(true),
                Value::Bool(enabled) => Ok(*enabled),
                _ => anyhow::bail!("Invalid file search setting: {key}"),
            }
        };
        Ok(Self {
            show_recents: boolean("show_recents")?,
            hide_noisy_folders: boolean("hide_noisy_folders")?,
            search_subfolders: boolean("search_subfolders")?,
            folders_first: boolean("folders_first")?,
        })
    }

    pub(super) fn encode(self) -> Value {
        json!({"show_recents": self.show_recents, "hide_noisy_folders": self.hide_noisy_folders,
            "search_subfolders": self.search_subfolders, "folders_first": self.folders_first})
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preferences_preserve_defaults_and_explicit_choices() {
        assert_eq!(
            FileSearch::parse(&Value::Null).unwrap(),
            FileSearch::default()
        );
        let options = FileSearch {
            show_recents: false,
            hide_noisy_folders: false,
            search_subfolders: false,
            folders_first: false,
        };
        assert_eq!(FileSearch::parse(&options.encode()).unwrap(), options);
        assert!(FileSearch::parse(&json!({"search_subfolders": "false"})).is_err());
    }
}
