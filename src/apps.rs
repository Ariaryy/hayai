//! Application catalog: discover installed apps from the Start Menu, fuzzy
//! search them, lazily extract their icons, and track most-recently-launched.
//!
//! Kept deliberately lightweight (plain `Vec` scan + subsequence scoring, icons
//! decoded on demand and cached) to honour the low-RAM / high-perf goal.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{App, RenderImage};

use crate::native;

/// Max rows shown in the results list (both the recents view and search).
const MAX_RESULTS: usize = 8;
/// How many recents we persist across runs.
const MAX_RECENTS: usize = 30;

pub struct AppEntry {
    pub name: String,
    /// Lowercased name, precomputed for case-insensitive matching.
    name_lower: String,
    /// The `.lnk`/executable path, or (for packaged apps discovered via
    /// `shell:AppsFolder`) a shell parsing string like
    /// `::{4234d49b-...}\PackageFamily!App`. Either form is handed straight
    /// to `ShellExecuteW`, which understands both.
    path: PathBuf,
}

pub struct Catalog {
    apps: Vec<AppEntry>,
    /// Decoded icons keyed by app path, or `Loading` while a background
    /// decode is in flight. `Ready(None)` means "tried and failed" so we
    /// don't repeatedly hit the shell for an icon that won't load.
    icons: HashMap<PathBuf, IconCache>,
    /// Most-recently-launched paths, newest first.
    recents: Vec<PathBuf>,
}

enum IconCache {
    Loading,
    Ready(Option<Arc<RenderImage>>),
}

/// Result of asking the catalog for a row's icon.
pub enum IconRequest {
    /// Already decoded (or already failed) — nothing more to do.
    Ready(Option<Arc<RenderImage>>),
    /// A decode is already in flight; wait for it.
    Loading,
    /// Not started yet — the caller should decode `.0` in the background and
    /// report it back via [`Catalog::set_icon_ready`].
    Load(PathBuf),
}

#[derive(Clone)]
pub struct CatalogGlobal {
    catalog: Rc<RefCell<Catalog>>,
}

impl CatalogGlobal {
    pub fn clone_handle(&self) -> Rc<RefCell<Catalog>> {
        self.catalog.clone()
    }
}

impl gpui::Global for CatalogGlobal {}

impl Catalog {
    /// Build the catalog (scan + load recents) and install it as a global.
    pub fn install(cx: &mut App) {
        let mut catalog = Catalog {
            apps: Vec::new(),
            icons: HashMap::new(),
            recents: Vec::new(),
        };
        catalog.scan();
        catalog.load_recents();
        cx.set_global(CatalogGlobal {
            catalog: Rc::new(RefCell::new(catalog)),
        });
    }

    fn scan(&mut self) {
        let mut seen = HashSet::new();
        for dir in start_menu_dirs() {
            walk(&dir, &mut |path| {
                let is_lnk = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e.eq_ignore_ascii_case("lnk"));
                if !is_lnk {
                    return;
                }
                let Some(name) = path.file_stem().and_then(|s| s.to_str()) else {
                    return;
                };
                if name.is_empty() {
                    return;
                }
                let name_lower = name.to_lowercase();
                // De-dup by name (the per-user and all-users Start Menus often
                // both contain the same shortcut).
                if seen.insert(name_lower.clone()) {
                    self.apps.push(AppEntry {
                        name: name.to_string(),
                        name_lower,
                        path: path.to_path_buf(),
                    });
                }
            });
        }

        // `.lnk` files under the Start Menu miss packaged (MSIX/UWP/Store)
        // apps entirely — they have no shortcut file on disk. `shell:AppsFolder`
        // is the virtual namespace Explorer's own Start Menu search reads, and
        // it's a superset of the `.lnk` scan above, so this only adds apps we
        // haven't already seen (e.g. NanaZip, other Store-distributed apps).
        for (name, path) in native::list_apps_folder() {
            if name.is_empty() {
                continue;
            }
            let name_lower = name.to_lowercase();
            if seen.insert(name_lower.clone()) {
                self.apps.push(AppEntry {
                    name,
                    name_lower,
                    path,
                });
            }
        }

        self.apps.sort_by(|a, b| a.name_lower.cmp(&b.name_lower));
    }

    /// Return indices (into `apps`) to display for the given query. An empty
    /// query yields the recents view; otherwise fuzzy-matched apps by score.
    pub fn search(&self, query: &str) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        if query.is_empty() {
            return self.recent_indices();
        }

        let mut scored: Vec<(i32, usize)> = self
            .apps
            .iter()
            .enumerate()
            .filter_map(|(index, app)| {
                fuzzy_score(&app.name_lower, &query).map(|score| (score, index))
            })
            .collect();
        // Highest score first; break ties alphabetically for stable ordering.
        scored.sort_by(|a, b| {
            b.0.cmp(&a.0)
                .then_with(|| self.apps[a.1].name_lower.cmp(&self.apps[b.1].name_lower))
        });
        scored
            .into_iter()
            .take(MAX_RESULTS)
            .map(|(_, index)| index)
            .collect()
    }

    /// Indices for the pre-typing view: recents first, then padded with
    /// alphabetically-first apps so the launcher isn't empty on a fresh install.
    fn recent_indices(&self) -> Vec<usize> {
        let mut indices = Vec::new();
        let mut used = HashSet::new();

        for path in &self.recents {
            if let Some(index) = self.apps.iter().position(|app| &app.path == path)
                && used.insert(index)
            {
                indices.push(index);
            }
        }
        for index in 0..self.apps.len() {
            if indices.len() >= MAX_RESULTS {
                break;
            }
            if used.insert(index) {
                indices.push(index);
            }
        }
        indices.truncate(MAX_RESULTS);
        indices
    }

    pub fn app(&self, index: usize) -> Option<&AppEntry> {
        self.apps.get(index)
    }

    /// Check the icon cache for `index`'s app.
    ///
    /// On a cache miss this marks the icon as loading and returns
    /// `IconRequest::Load(path)` — decoding touches GDI/COM and is too slow
    /// to do synchronously in the render path (it was blocking keystrokes),
    /// so callers must decode `path` off the main thread and hand the result
    /// back via `set_icon_ready`.
    pub fn request_icon(&mut self, index: usize) -> IconRequest {
        let Some(path) = self.apps.get(index).map(|app| app.path.clone()) else {
            return IconRequest::Ready(None);
        };
        match self.icons.get(&path) {
            Some(IconCache::Ready(icon)) => IconRequest::Ready(icon.clone()),
            Some(IconCache::Loading) => IconRequest::Loading,
            None => {
                self.icons.insert(path.clone(), IconCache::Loading);
                IconRequest::Load(path)
            }
        }
    }

    /// Record the result of a background icon decode started via
    /// `request_icon`.
    pub fn set_icon_ready(&mut self, path: PathBuf, icon: Option<Arc<RenderImage>>) {
        self.icons.insert(path, IconCache::Ready(icon));
    }

    /// Launch the app at `index` and record it as most-recent.
    pub fn launch(&mut self, index: usize) {
        let Some(app) = self.apps.get(index) else {
            return;
        };
        let path = app.path.clone();
        native::launch_path(&path);
        self.record_recent(path);
    }

    fn record_recent(&mut self, path: PathBuf) {
        self.recents.retain(|existing| existing != &path);
        self.recents.insert(0, path);
        self.recents.truncate(MAX_RECENTS);
        self.save_recents();
    }

    fn load_recents(&mut self) {
        let Some(file) = recents_file() else {
            return;
        };
        if let Ok(contents) = std::fs::read_to_string(&file) {
            self.recents = contents
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(PathBuf::from)
                .collect();
        }
    }

    fn save_recents(&self) {
        let Some(file) = recents_file() else {
            return;
        };
        if let Some(parent) = file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let contents = self
            .recents
            .iter()
            .map(|path| path.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(&file, contents);
    }
}

fn start_menu_dirs() -> Vec<PathBuf> {
    const RELATIVE: &str = r"Microsoft\Windows\Start Menu\Programs";
    let mut dirs = Vec::new();
    if let Ok(program_data) = std::env::var("PROGRAMDATA") {
        dirs.push(PathBuf::from(program_data).join(RELATIVE));
    }
    if let Ok(app_data) = std::env::var("APPDATA") {
        dirs.push(PathBuf::from(app_data).join(RELATIVE));
    }
    dirs
}

fn recents_file() -> Option<PathBuf> {
    let app_data = std::env::var("APPDATA").ok()?;
    Some(PathBuf::from(app_data).join("hayai").join("recents.txt"))
}

/// Recursively visit files under `dir`, calling `visit` on each file. The Start
/// Menu tree is shallow, so plain recursion is fine.
fn walk(dir: &Path, visit: &mut impl FnMut(&Path)) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            walk(&path, visit);
        } else {
            visit(&path);
        }
    }
}

/// Subsequence fuzzy match with bonuses for matches at the start, at word
/// boundaries, and for consecutive characters. Returns `None` if `needle` is
/// not a subsequence of `haystack`. Both are expected lowercased.
fn fuzzy_score(haystack: &str, needle: &str) -> Option<i32> {
    if needle.is_empty() {
        return Some(0);
    }

    let hay: Vec<char> = haystack.chars().collect();
    let mut hay_index = 0;
    let mut score = 0;
    let mut prev_matched = false;

    for needle_char in needle.chars() {
        let mut found = false;
        while hay_index < hay.len() {
            let hay_char = hay[hay_index];
            if hay_char == needle_char {
                if hay_index == 0 {
                    score += 10;
                } else if !hay[hay_index - 1].is_alphanumeric() {
                    score += 8; // word boundary
                }
                if prev_matched {
                    score += 5; // consecutive run
                }
                score += 1;
                hay_index += 1;
                prev_matched = true;
                found = true;
                break;
            }
            hay_index += 1;
            prev_matched = false;
        }
        if !found {
            return None;
        }
    }

    if haystack.starts_with(needle) {
        score += 15;
    }
    Some(score)
}
