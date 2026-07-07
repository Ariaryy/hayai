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

use gpui::{App, AppContext, RenderImage};

use crate::native;

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
    /// Install an empty catalog immediately and kick off the (slow) scan on
    /// the background pool so startup never blocks on it; recents load is a
    /// tiny file read and stays synchronous.
    pub fn install(cx: &mut App) {
        let mut catalog = Catalog {
            apps: Vec::new(),
            icons: HashMap::new(),
            recents: Vec::new(),
        };
        catalog.load_recents();
        let handle = Rc::new(RefCell::new(catalog));
        cx.set_global(CatalogGlobal {
            catalog: handle.clone(),
        });

        // The scan walks two directory trees and enumerates shell:AppsFolder
        // via COM — too slow for the startup path. Run it on the background
        // pool and publish the result when it lands.
        cx.spawn(async move |cx| {
            let apps = cx.background_spawn(async move { scan_apps() }).await;
            let _ = cx.update(|cx| {
                cx.global::<CatalogGlobal>()
                    .clone_handle()
                    .borrow_mut()
                    .set_apps(apps);
                let launcher = cx.global::<crate::launcher::LauncherGlobal>().clone_handle();
                launcher.borrow().refresh_results(cx);
            });
        })
        .detach();
    }

    /// Replace the app list with a fresh scan result. Preserves the icon
    /// cache (keyed by path, so still valid) and recents.
    pub fn set_apps(&mut self, apps: Vec<AppEntry>) {
        self.apps = apps;
    }

    /// Return indices (into `apps`) to display for the given query. An empty
    /// query yields the recents-first view of the whole catalog; otherwise
    /// fuzzy-matched apps by score. The results list is virtualized, so there's
    /// no need to cap how many we hand back.
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
        scored.into_iter().map(|(_, index)| index).collect()
    }

    /// Indices for the pre-typing view: recents first, then every remaining
    /// app alphabetically, so the whole catalog is browsable via scrolling.
    fn recent_indices(&self) -> Vec<usize> {
        let mut indices = Vec::with_capacity(self.apps.len());
        let mut used = HashSet::new();

        for path in &self.recents {
            if let Some(index) = self.apps.iter().position(|app| &app.path == path)
                && used.insert(index)
            {
                indices.push(index);
            }
        }
        for index in 0..self.apps.len() {
            if used.insert(index) {
                indices.push(index);
            }
        }
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

    /// Record `index`'s app as most-recently-launched and hand back what the
    /// caller needs to finish the launch off the UI thread: the launch path
    /// and the serialized recents file contents. The actual ShellExecuteW
    /// call and the recents write are too slow for the UI thread (they held
    /// the window open after Enter), so the caller runs them via
    /// `background_spawn`.
    pub fn mark_launched(&mut self, index: usize) -> Option<(PathBuf, String)> {
        let path = self.apps.get(index)?.path.clone();
        self.recents.retain(|existing| existing != &path);
        self.recents.insert(0, path.clone());
        self.recents.truncate(MAX_RECENTS);
        let serialized = self
            .recents
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\n");
        Some((path, serialized))
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

}

/// Write pre-serialized recents contents to disk. Free function on purpose:
/// runs on the background pool, must not touch the (non-Send) catalog.
pub fn write_recents_file(contents: &str) {
    let Some(file) = recents_file() else {
        return;
    };
    if let Some(parent) = file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::write(&file, contents);
}

/// Scan Start Menu shortcuts + `shell:AppsFolder` into a fresh app list.
/// Pure I/O + COM — safe (and intended) to run on a background thread;
/// `native::list_apps_folder` initializes COM on its calling thread itself.
fn scan_apps() -> Vec<AppEntry> {
    let mut apps = Vec::new();
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
                apps.push(AppEntry {
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
            apps.push(AppEntry {
                name,
                name_lower,
                path,
            });
        }
    }

    apps.sort_by(|a, b| a.name_lower.cmp(&b.name_lower));
    apps
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
