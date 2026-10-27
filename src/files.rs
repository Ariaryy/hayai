//! File search via scry daemon (`scryd`), keyword ` f `.
//!
//! Debounced provider: `background_search` runs `native::scry_query`
//! on the background pool (never inline in `search`, which stays cheap for
//! the empty-query case the pipeline calls synchronously).

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gpui::App;

use crate::commands::{BackgroundSearch, CommandAction, CommandItem, CommandProvider, IconSource};
use crate::native;
use crate::settings::FileSearch;

const MAX_RESULTS: u32 = 64;
/// Fetch extra ranked candidates so filtering noisy directories can still
/// fill the visible results without changing Scry's relevance order.
const SEARCH_CANDIDATES: u32 = 256;
/// Scoped matches are filtered client-side against the exact directory the
/// user entered. Keep a wider candidate set so common path components do not
/// crowd out valid descendants before that filter runs.
const SCOPED_SEARCH_CANDIDATES: u32 = 512;
/// How many recently-opened files/folders we persist across runs.
const MAX_RECENTS: usize = 30;

/// Most-recently-opened files/folders (via file-mode Enter), newest first.
/// Mirrors `apps::Catalog`'s recents so the empty-query view of file mode
/// (keyword typed, nothing after it yet) has something useful to show
/// instead of a blank list.
pub struct FileRecents {
    recents: Vec<PathBuf>,
}

#[derive(Clone)]
pub struct FileRecentsGlobal {
    recents: Rc<RefCell<FileRecents>>,
}

impl FileRecentsGlobal {
    pub fn install(cx: &mut App) {
        cx.set_global(FileRecentsGlobal {
            recents: Rc::new(RefCell::new(FileRecents::load())),
        });
    }

    pub fn clone_handle(&self) -> Rc<RefCell<FileRecents>> {
        self.recents.clone()
    }
}

impl gpui::Global for FileRecentsGlobal {}

impl FileRecents {
    fn load() -> Self {
        let mut recents = Vec::new();
        if crate::history::enabled()
            && let Some(file) = recents_file()
            && let Ok(contents) = std::fs::read_to_string(&file)
        {
            recents = contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(PathBuf::from)
                .collect();
        }
        Self { recents }
    }

    pub fn clear(&mut self) {
        self.recents.clear();
    }

    pub fn recents(&self) -> &[PathBuf] {
        &self.recents
    }

    /// Record `path` as most-recently-opened and hand back the serialized
    /// recents contents for the caller to persist off the UI thread (same
    /// pattern as `Catalog::mark_launched_path`).
    pub fn mark_opened(&mut self, path: PathBuf) -> String {
        self.recents.retain(|existing| existing != &path);
        self.recents.insert(0, path);
        self.recents.truncate(MAX_RECENTS);
        self.recents
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn recents_file() -> Option<PathBuf> {
    let app_data = std::env::var("APPDATA").ok()?;
    Some(
        PathBuf::from(app_data)
            .join("hayai")
            .join("file_recents.txt"),
    )
}

/// Directories that are almost never what someone means when they search by
/// name (build output, package caches, VCS internals) — excluded from results
/// unless the user's own query explicitly references the directory term.
const NOISY_DIRS: &[&str] = &[
    "node_modules",
    ".git",
    "target",
    "dist",
    "build",
    "__pycache__",
    ".cache",
    "vendor",
    ".next",
    ".venv",
];

fn is_noisy_path(path: &Path, query: &str) -> bool {
    let query_lower = query.to_lowercase();
    path.components().any(|component| {
        let Some(name) = component.as_os_str().to_str() else {
            return false;
        };
        let name = name.to_lowercase();
        NOISY_DIRS.contains(&name.as_str()) && !query_lower.contains(&name)
    })
}

fn path_to_item(path: &Path) -> CommandItem {
    CommandItem {
        recent: false,
        id: path.to_string_lossy().into_owned(),
        title: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
        subtitle: path.parent().map(|p| p.to_string_lossy().into_owned()),
        file_size: None,
        calculation_detail: None,
        icon: IconSource::Path(path.to_path_buf()),
        action: CommandAction::OpenFile(path.to_path_buf()),
    }
}

/// Listing of the user's home folder, displayed after recent entries on
/// file-mode entry, or on its own when recents are hidden. All local
/// directory reads and metadata checks run in the provider's background job.
fn directory_listing(dir: &Path, query: &str, options: FileSearch) -> Vec<CommandItem> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| !options.hide_noisy_folders || !is_noisy_path(path, query))
        .collect();

    // Folders first, alphabetical within each group. This is intentionally a
    // non-recursive listing: Tab enters one level, while typed input invokes
    // the indexed descendant search below.
    paths.sort_by(|a, b| {
        let folder_order = if options.folders_first {
            b.is_dir().cmp(&a.is_dir())
        } else {
            std::cmp::Ordering::Equal
        };
        folder_order.then_with(|| {
            let a_name = a.file_name().map(|n| n.to_string_lossy().to_lowercase());
            let b_name = b.file_name().map(|n| n.to_string_lossy().to_lowercase());
            a_name.cmp(&b_name)
        })
    });
    paths.truncate(MAX_RESULTS as usize);
    paths.iter().map(|path| path_to_item(path)).collect()
}

fn recent_item(path: &Path) -> CommandItem {
    CommandItem {
        recent: true,
        ..path_to_item(path)
    }
}

fn entry_results(recents: &[PathBuf], listing: Vec<CommandItem>) -> Vec<CommandItem> {
    let mut seen = std::collections::HashSet::with_capacity(recents.len());
    let mut items: Vec<_> = recents
        .iter()
        .filter(|path| path.exists() && seen.insert((*path).clone()))
        .map(|path| recent_item(path))
        .collect();
    items.extend(listing.into_iter().filter(
        |item| !matches!(&item.action, CommandAction::OpenFile(path) if seen.contains(path)),
    ));
    items
}

fn home_dir_listing(options: FileSearch) -> Vec<CommandItem> {
    let Ok(home) = std::env::var("USERPROFILE") else {
        return Vec::new();
    };
    directory_listing(Path::new(&home), "", options)
}

fn parse_scoped_query(query: &str) -> Option<(PathBuf, String)> {
    let (scope, remainder) = query.split_once('|')?;
    let scope = PathBuf::from(scope.trim_end_matches('\\'));
    (!scope.as_os_str().is_empty()).then_some((scope, remainder.trim().to_owned()))
}

fn scoped_scry_query(scope: &Path, remainder: &str) -> String {
    let scope = scope.to_string_lossy();
    let scope = scope.trim_end_matches('\\');
    format!("{scope}\\{}", remainder.replace(' ', "\\"))
}

fn matches_scope(parent: &Path, scope: &Path, subfolders: bool) -> bool {
    if subfolders {
        parent.starts_with(scope)
    } else {
        parent == scope
    }
}
pub(crate) fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

pub struct FileSearchProvider {
    recents: Rc<RefCell<FileRecents>>,
    options: FileSearch,
}

impl FileSearchProvider {
    pub fn new(recents: Rc<RefCell<FileRecents>>) -> Self {
        Self {
            recents,
            options: FileSearch::default(),
        }
    }
}

impl CommandProvider for FileSearchProvider {
    fn configure(&mut self, preferences: &crate::settings::Preferences) {
        self.options = preferences.file_search;
    }
    fn namespace(&self) -> &'static str {
        "files"
    }

    fn keyword(&self) -> Option<&'static str> {
        Some("f")
    }

    fn wants_debounce(&self) -> bool {
        true
    }

    /// Immediate file-mode preview from paths already in memory. Do not stat
    /// paths or enumerate directories here: the background job validates these
    /// entries and supplies the home-folder fallback after compatibility checks.
    fn search(&self, query: &str) -> Vec<CommandItem> {
        if !query.is_empty() || !self.options.show_recents {
            return Vec::new();
        }
        self.recents
            .borrow()
            .recents()
            .iter()
            .map(|path| recent_item(path))
            .collect()
    }

    fn background_search(&self, query: &str) -> Option<BackgroundSearch> {
        let options = self.options;
        let empty_recents = query.is_empty().then(|| {
            if options.show_recents {
                self.recents.borrow().recents().to_vec()
            } else {
                Vec::new()
            }
        });
        let query = query.to_string();
        let scope = parse_scoped_query(&query);
        let browse_dir = scope
            .as_ref()
            .and_then(|(dir, remainder)| remainder.is_empty().then(|| dir.clone()));
        let scry_query = scope
            .as_ref()
            .map(|(dir, remainder)| scoped_scry_query(dir, remainder))
            .unwrap_or_else(|| query.clone());
        let max_results = if scope.is_some() {
            SCOPED_SEARCH_CANDIDATES
        } else {
            SEARCH_CANDIDATES
        };

        Some(Box::new(move || {
            if crate::scry_updates::is_updating() {
                return vec![CommandItem {
                    recent: false,
                    id: "files:compatibility:updating".into(),
                    title: "Updating Scry Search".into(),
                    subtitle: Some(
                        "File search will be available after the daemon has been verified.".into(),
                    ),
                    file_size: None,
                    calculation_detail: None,
                    icon: IconSource::None,
                    action: CommandAction::CheckFileSearchUpdates,
                }];
            }
            let compatibility = native::scry_compatibility();
            if let Some(item) = compatibility_item(compatibility) {
                return vec![item];
            }
            if compatibility == crate::scry::Compatibility::Unavailable {
                return vec![unavailable_item()];
            }
            if let Some(recents) = empty_recents {
                return entry_results(&recents, home_dir_listing(options));
            }
            if let Some(dir) = browse_dir {
                return directory_listing(&dir, &query, options);
            }
            match native::scry_query(&scry_query, max_results) {
                None => vec![unavailable_item()],
                Some(mut hits) => {
                    hits.retain(|hit| {
                        let full_path = hit.parent.join(&hit.name);
                        if options.hide_noisy_folders && is_noisy_path(&full_path, &query) {
                            return false;
                        }
                        if let Some((dir, _)) = &scope
                            && (!matches_scope(&hit.parent, dir, options.search_subfolders)
                                || full_path == *dir)
                        {
                            return false;
                        }
                        true
                    });
                    // Scry already ranks these by relevance. Sorting by path
                    // depth puts Prefetch artifacts ahead of an exact Code.exe
                    // match under AppData. `retain` above preserves that order;
                    // only directory browsing uses folders-first sorting.
                    hits.truncate(MAX_RESULTS as usize);
                    hits.into_iter()
                        .map(|hit| {
                            let full_path = hit.parent.join(&hit.name);
                            let parent_str = hit.parent.to_string_lossy();
                            let subtitle = parent_str.into_owned();
                            CommandItem {
                                recent: false,
                                id: full_path.to_string_lossy().into_owned(),
                                title: hit.name,
                                subtitle: Some(subtitle),
                                file_size: (!hit.is_folder).then_some(hit.size),
                                calculation_detail: None,
                                // TODO: per-extension icon dedup — each hit currently
                                // grows the catalog's path-keyed icon cache by one
                                // entry; fine at 64 results/keystroke, revisit if
                                // that ever changes.
                                icon: IconSource::Path(full_path.clone()),
                                action: CommandAction::OpenFile(full_path),
                            }
                        })
                        .collect()
                }
            }
        }))
    }
}

fn unavailable_item() -> CommandItem {
    let installed = native::scry_is_installed();
    CommandItem {
        recent: false,
        id: "files:compatibility:unavailable".into(),
        title: if installed {
            "Scry Search is unavailable"
        } else {
            "Set up Scry Search"
        }
        .into(),
        subtitle: Some(
            if installed {
                "Check for a compatible Scry Search release to restore file search."
            } else {
                "Check for a compatible Scry Search release to enable file search."
            }
            .into(),
        ),
        file_size: None,
        calculation_detail: None,
        icon: IconSource::None,
        action: CommandAction::CheckFileSearchUpdates,
    }
}

fn compatibility_item(status: crate::scry::Compatibility) -> Option<CommandItem> {
    use crate::scry::Compatibility;
    let (id, title, description) = match status {
        Compatibility::Ready | Compatibility::Unavailable => return None,
        Compatibility::UpdateRequired => (
            "files:compatibility:update",
            "Scry Search update required",
            "This version of Scry Search is too old for Hayai. Update Scry Search to continue.",
        ),
        Compatibility::NewerDaemon => (
            "files:compatibility:newer",
            "Hayai update required",
            "Scry Search uses a newer protocol or shared-index format. Update Hayai to continue.",
        ),
        Compatibility::InvalidResponse => (
            "files:compatibility:error",
            "Couldn't verify Scry Search",
            "The daemon returned an invalid status response. Reopen file search to retry.",
        ),
    };
    Some(CommandItem {
        recent: false,
        id: id.into(),
        title: title.into(),
        subtitle: Some(description.into()),
        file_size: None,
        calculation_detail: None,
        icon: IconSource::None,
        action: match status {
            Compatibility::UpdateRequired => CommandAction::CheckFileSearchUpdates,
            Compatibility::NewerDaemon => CommandAction::CheckForUpdates,
            _ => CommandAction::ShowText(String::new()),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_recents_do_not_replace_the_home_listing() {
        let root = std::env::temp_dir().join(format!("hayai-entry-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let recent = root.join("recent.txt");
        let other = root.join("other.txt");
        std::fs::write(&recent, "").unwrap();
        std::fs::write(&other, "").unwrap();
        let items = entry_results(
            &[recent.clone(), root.join("missing.txt")],
            vec![path_to_item(&other), path_to_item(&recent)],
        );
        assert!(items[0].recent);
        assert!(!items[1].recent);
        assert_eq!(
            items
                .iter()
                .map(|item| item.id.as_str())
                .collect::<Vec<_>>(),
            [recent.to_str().unwrap(), other.to_str().unwrap()]
        );
        std::fs::remove_file(recent).unwrap();
        std::fs::remove_file(other).unwrap();
        std::fs::remove_dir(root).unwrap();
    }

    #[test]
    fn immediate_preview_preserves_in_memory_recents_without_disk_validation() {
        let paths = vec![
            PathBuf::from("hayai-preview-missing/newest.txt"),
            PathBuf::from("hayai-preview-missing/older.txt"),
        ];
        let mut provider = FileSearchProvider::new(Rc::new(RefCell::new(FileRecents {
            recents: paths.clone(),
        })));
        let preview = provider.search("");
        assert_eq!(preview.len(), paths.len());
        for (item, path) in preview.iter().zip(&paths) {
            assert!(matches!(&item.action, CommandAction::OpenFile(actual) if actual == path));
        }
        assert!(provider.search("typed query").is_empty());
        let mut preferences = crate::settings::Preferences::default();
        preferences.file_search.show_recents = false;
        provider.configure(&preferences);
        assert!(provider.search("").is_empty());
        assert_eq!(provider.recents.borrow().recents(), paths);
        preferences.file_search.show_recents = true;
        provider.configure(&preferences);
        assert_eq!(provider.search("").len(), paths.len());
    }

    #[test]
    fn empty_recents_leave_home_folder_work_to_the_background_job() {
        let provider = FileSearchProvider::new(Rc::new(RefCell::new(FileRecents {
            recents: Vec::new(),
        })));
        assert!(provider.search("").is_empty());
        assert!(provider.background_search("").is_some());
    }

    #[test]
    fn scoped_search_can_match_direct_children_or_descendants() {
        let scope = Path::new("scope");
        assert!(matches_scope(scope, scope, false));
        assert!(!matches_scope(&scope.join("nested"), scope, false));
        assert!(matches_scope(&scope.join("nested"), scope, true));
        assert!(!matches_scope(Path::new("elsewhere"), scope, true));
    }

    #[test]
    fn browsing_respects_folder_order_and_build_folder_visibility() {
        let nonce = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("hayai-browse-{}-{nonce}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        std::fs::create_dir(root.join("z-folder")).unwrap();
        std::fs::create_dir(root.join("node_modules")).unwrap();
        std::fs::write(root.join("a-file"), "").unwrap();
        let names = |options| {
            directory_listing(&root, "", options)
                .into_iter()
                .map(|item| item.title)
                .collect::<Vec<_>>()
        };
        let mut options = FileSearch::default();
        assert_eq!(names(options), ["z-folder", "a-file"]);
        options.folders_first = false;
        assert_eq!(names(options), ["a-file", "z-folder"]);
        options.hide_noisy_folders = false;
        assert_eq!(names(options), ["a-file", "node_modules", "z-folder"]);
        std::fs::remove_file(root.join("a-file")).unwrap();
        std::fs::remove_dir(root.join("z-folder")).unwrap();
        std::fs::remove_dir(root.join("node_modules")).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
