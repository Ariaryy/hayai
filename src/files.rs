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

const MAX_RESULTS: u32 = 64;
/// Query extra candidates before applying Hayai's folder-first presentation.
/// Scry ranks relevance without a folder preference, so fetching only the
/// visible count can leave a navigable directory out of the list.
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
        if let Some(file) = recents_file()
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

/// Write pre-serialized recents contents to disk. Free function on purpose:
/// runs on the background pool, must not touch the (non-Send) `FileRecents`.
pub fn write_file_recents_file(contents: &str) {
    let Some(file) = recents_file() else {
        return;
    };
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&file, contents);
}

fn recents_file() -> Option<PathBuf> {
    let app_data = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(app_data).join("hayai").join("file_recents.txt"))
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

fn path_to_item(path: &PathBuf) -> CommandItem {
    CommandItem {
        id: path.to_string_lossy().into_owned(),
        title: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.to_string_lossy().into_owned()),
        subtitle: path.parent().map(|p| p.to_string_lossy().into_owned()),
        icon: IconSource::Path(path.clone()),
        action: CommandAction::OpenFile(path.clone()),
    }
}

/// Listing of the user's home folder, used as the file-mode empty-query
/// fallback when there's no recents history yet (fresh install). Local
/// directory read, not a daemon IPC round trip — cheap enough to run
/// inline on the (synchronous) `search` path.
fn directory_listing(dir: &Path, query: &str) -> Vec<CommandItem> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| !is_noisy_path(path, query))
        .collect();

    // Folders first, alphabetical within each group. This is intentionally a
    // non-recursive listing: Tab enters one level, while typed input invokes
    // the indexed descendant search below.
    paths.sort_by(|a, b| {
        b.is_dir().cmp(&a.is_dir()).then_with(|| {
            let a_name = a.file_name().map(|n| n.to_string_lossy().to_lowercase());
            let b_name = b.file_name().map(|n| n.to_string_lossy().to_lowercase());
            a_name.cmp(&b_name)
        })
    });
    paths.truncate(MAX_RESULTS as usize);
    paths.iter().map(path_to_item).collect()
}

fn home_dir_listing() -> Vec<CommandItem> {
    let Ok(home) = std::env::var("USERPROFILE") else {
        return Vec::new();
    };
    directory_listing(Path::new(&home), "")
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
fn format_size(bytes: u64) -> String {
    if bytes == 0 {
        return String::new();
    }
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
}

impl FileSearchProvider {
    pub fn new(recents: Rc<RefCell<FileRecents>>) -> Self {
        Self { recents }
    }
}

impl CommandProvider for FileSearchProvider {
    fn namespace(&self) -> &'static str {
        "files"
    }

    fn keyword(&self) -> Option<&'static str> {
        Some("f")
    }

    fn wants_debounce(&self) -> bool {
        true
    }

    /// Empty query (keyword typed, nothing after it yet): show recently-
    /// opened files/folders, same as the apps provider's pre-typing recents
    /// view. Falls back to listing the user's home folder when there's no
    /// history yet (fresh install), so file mode is never just blank.
    fn search(&self, query: &str) -> Vec<CommandItem> {
        if !query.is_empty() {
            return Vec::new();
        }
        let recents: Vec<CommandItem> = self
            .recents
            .borrow()
            .recents()
            .iter()
            .filter(|path| path.exists())
            .map(path_to_item)
            .collect();
        if !recents.is_empty() {
            return recents;
        }
        home_dir_listing()
    }

    fn background_search(&self, query: &str) -> Option<BackgroundSearch> {
        if query.is_empty() {
            return None;
        }
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
            if let Some(dir) = browse_dir {
                return directory_listing(&dir, &query);
            }
            match native::scry_query(&scry_query, max_results) {
            None => vec![CommandItem {
                id: "files:not-running".into(),
                title: "Enable file search".into(),
                subtitle: Some("Install and start the elevated Scry Search daemon".into()),
                icon: IconSource::None,
                action: CommandAction::InstallFileSearch,
            }],
            Some(mut hits) => {
                hits.retain(|hit| {
                    let full_path = hit.parent.join(&hit.name);
                    if is_noisy_path(&full_path, &query) {
                        return false;
                    }
                    if let Some((dir, _)) = &scope
                        && (!full_path.starts_with(dir) || full_path == *dir)
                    {
                        return false;
                    }
                    true
                });
                // Folders first, then shallower paths, then alphabetical by name.
                hits.sort_by(|a, b| {
                    b.is_folder
                        .cmp(&a.is_folder)
                        .then_with(|| a.parent.components().count().cmp(&b.parent.components().count()))
                        .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
                });
                hits.truncate(MAX_RESULTS as usize);
                hits
                .into_iter()
                .map(|hit| {
                    let full_path = hit.parent.join(&hit.name);
                    let parent_str = hit.parent.to_string_lossy();
                    let subtitle = if !hit.is_folder && hit.size > 0 {
                        format!("{parent_str} · {}", format_size(hit.size))
                    } else {
                        parent_str.into_owned()
                    };
                    CommandItem {
                        id: full_path.to_string_lossy().into_owned(),
                        title: hit.name,
                        subtitle: Some(subtitle),
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
        }}))
    }
}
