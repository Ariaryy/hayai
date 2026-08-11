use std::path::PathBuf;

/// Where a result row's icon comes from. Kept as a *source* (not decoded
/// pixels) so rows stay cheap to produce and the existing lazy background
/// icon-decode cache keeps doing the work.
#[derive(Clone, Debug)]
pub enum IconSource {
    /// No icon (for example, a backend-unavailable hint row).
    None,
    /// Resolve through the catalog's path-keyed icon cache (apps today;
    /// files later — same extraction machinery).
    Path(PathBuf),
}

// Unused variants below are the extension surface for future providers
// (clipboard history, calculator) — see `ROADMAP.md`.
#[derive(Clone, Debug)]
pub enum CommandAction {
    LaunchApplication(PathBuf),
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

/// A debounced provider's search job: captures its query, runs on the
/// background pool, and hands back the finished result set. `Send` because
/// the pipeline runs it off the (non-`Send`) `Rc<RefCell<PluginRegistry>>`.
pub type BackgroundSearch = Box<dyn FnOnce() -> Vec<CommandItem> + Send>;

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
    /// NL auto-detection: tried against every provider, in registration
    /// order, before keyword-prefix routing (see `PluginRegistry::dispatch`).
    /// Runs on every keystroke on the UI thread, so implementations must stay
    /// cheap — bail out on a first-character check before attempting any
    /// real parse. `query` is the raw, untrimmed dispatch input.
    fn auto_claim(&self, _query: &str) -> bool {
        false
    }
    /// `query` is pre-trimmed of the keyword prefix (and surrounding
    /// whitespace) by the dispatcher.
    fn search(&self, query: &str) -> Vec<CommandItem>;

    /// Debounced providers (`wants_debounce() == true`) return a job here
    /// capturing `query`; the pipeline runs it on the background pool after
    /// the debounce delay instead of calling `search` inline. Returning
    /// `None` (e.g. for an empty query) falls back to the synchronous
    /// `search` path.
    fn background_search(&self, _query: &str) -> Option<BackgroundSearch> {
        None
    }
}
