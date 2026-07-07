use std::path::PathBuf;

/// Where a result row's icon comes from. Kept as a *source* (not decoded
/// pixels) so rows stay cheap to produce and the existing lazy background
/// icon-decode cache keeps doing the work.
#[derive(Clone, Debug)]
pub enum IconSource {
    // Unused until a provider without an icon (e.g. calculator) lands.
    #[allow(dead_code)]
    None,
    /// Resolve through the catalog's path-keyed icon cache (apps today;
    /// files later — same extraction machinery).
    Path(PathBuf),
}

// Unused variants below are the extension surface for future providers
// (file search, clipboard history, calculator) — see `ROADMAP.md`.
#[derive(Clone, Debug)]
pub enum CommandAction {
    LaunchApplication(PathBuf),
    #[allow(dead_code)]
    OpenFile(PathBuf),
    #[allow(dead_code)]
    CopyToClipboard(String),
    #[allow(dead_code)]
    ShowText(String),
}

#[derive(Clone, Debug)]
pub struct CommandItem {
    // Unused until a provider needs stable IDs (e.g. dedup, action follow-up).
    #[allow(dead_code)]
    pub id: String,
    pub title: String,
    pub subtitle: Option<String>,
    pub icon: IconSource,
    pub action: CommandAction,
}

pub trait CommandProvider {
    // Unused until a second provider makes namespace disambiguation useful.
    #[allow(dead_code)]
    fn namespace(&self) -> &'static str;
    /// The leading-space mode keyword that routes to this provider
    /// (e.g. `"f"` for ` f <query>`), or None for the default provider.
    fn keyword(&self) -> Option<&'static str> {
        None
    }
    /// Providers that hit disk/IPC/network return true so the pipeline
    /// debounces their queries; in-memory providers stay zero-latency.
    fn wants_debounce(&self) -> bool {
        false
    }
    /// `query` is pre-trimmed of the keyword prefix (and surrounding
    /// whitespace) by the dispatcher.
    fn search(&self, query: &str) -> Vec<CommandItem>;
}
