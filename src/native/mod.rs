#[derive(Clone, Copy, Debug)]
pub enum NativeCommand {
    ToggleLauncher,
    Quit,
}

/// A decoded icon as tightly-packed top-down BGRA bytes (4 bytes/pixel).
///
/// BGRA is what GPUI's `RenderImage` expects, so callers can hand `bgra`
/// straight to `image::RgbaImage::from_raw` without a channel swap.
pub struct IconImage {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
pub use windows::{
    NativeRuntime, extract_icon_rgba, focus_launcher_window, hide_launcher_window,
    https_get, launch_path, launch_path_as_admin, list_apps_folder, local_now, scry_query,
    system_currency_code,
};

#[cfg(not(windows))]
pub struct NativeRuntime;

#[cfg(not(windows))]
impl NativeRuntime {
    pub fn start(_sender: futures::channel::mpsc::UnboundedSender<NativeCommand>) -> Self {
        eprintln!(
            "Hayai native tray and global hotkey support is currently implemented on Windows."
        );
        Self
    }
}

#[cfg(not(windows))]
pub fn focus_launcher_window(_window: &gpui::Window) {}

#[cfg(not(windows))]
pub fn hide_launcher_window(_window: &gpui::Window) {}

#[cfg(not(windows))]
pub fn extract_icon_rgba(_path: &std::path::Path) -> Option<IconImage> {
    None
}

#[cfg(not(windows))]
pub fn launch_path(_path: &std::path::Path) {}

#[cfg(not(windows))]
pub fn launch_path_as_admin(_path: &std::path::Path) {}

#[cfg(not(windows))]
pub fn list_apps_folder() -> Vec<(String, std::path::PathBuf)> {
    Vec::new()
}

/// One search result hit.
#[cfg(not(windows))]
pub struct FileHit {
    pub name: String,
    pub parent: std::path::PathBuf,
    pub is_folder: bool,
    pub size: u64,
    #[allow(dead_code)]
    pub mtime: u32,
}

#[cfg(not(windows))]
pub fn scry_query(_query: &str, _max_results: u32) -> Option<Vec<FileHit>> {
    None
}

#[cfg(not(windows))]
pub fn https_get(_host: &str, _path: &str) -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn system_currency_code() -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn local_now() -> (i32, u32, u32, u32, u32) {
    (1970, 1, 1, 0, 0)
}
