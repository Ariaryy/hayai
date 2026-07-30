//! File search via a running Everything (voidtools) instance, keyword ` f `.
//!
//! Debounced provider: `background_search` runs `native::everything_query`
//! on the background pool (never inline in `search`, which stays cheap for
//! the empty-query case the pipeline calls synchronously).

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

use gpui::App;

use crate::commands::{BackgroundSearch, CommandAction, CommandItem, CommandProvider, IconSource};
use crate::native;

const MAX_RESULTS: u32 = 64;
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
/// name (build output, package caches, VCS internals) — excluded from every
/// query via Everything's `!term` syntax unless the user's own query already
/// mentions the term (so searching *for* "node_modules" still works).
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

/// Append `!term` exclusions for noisy directories the query doesn't already
/// reference, so common build/package junk doesn't mask the result someone
/// actually wants.
fn augment_query(query: &str) -> String {
    let lower = query.to_lowercase();
    let mut augmented = query.to_string();
    for dir in NOISY_DIRS {
        if !lower.contains(dir) {
            augmented.push_str(" !");
            augmented.push_str(dir);
        }
    }
    augmented
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
/// directory read, not an Everything IPC round trip — cheap enough to run
/// inline on the (synchronous) `search` path.
fn home_dir_listing() -> Vec<CommandItem> {
    let Ok(home) = std::env::var("USERPROFILE") else {
        return Vec::new();
    };
    let Ok(entries) = std::fs::read_dir(home) else {
        return Vec::new();
    };
    let mut paths: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    // Folders first (mirrors how file browsers order a directory), then
    // alphabetical within each group.
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
        Some(Box::new(move || match native::everything_query(&augment_query(&query), MAX_RESULTS) {
            None => vec![CommandItem {
                id: "files:not-running".into(),
                title: "Everything not running".into(),
                subtitle: Some("File search needs Everything (voidtools.com)".into()),
                icon: IconSource::None,
                action: CommandAction::ShowText(String::new()),
            }],
            Some(mut hits) => {
                // Shallower paths first as a tie-break — a stable sort keeps
                // Everything's own relevance ordering within each depth, so
                // this only nudges deeply-nested matches down rather than
                // reordering everything.
                hits.sort_by_key(|hit| hit.parent.components().count());
                hits
                .into_iter()
                .map(|hit| {
                    let full_path = hit.parent.join(&hit.name);
                    CommandItem {
                        id: full_path.to_string_lossy().into_owned(),
                        title: hit.name,
                        subtitle: Some(hit.parent.to_string_lossy().into_owned()),
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
        }))
    }
}
