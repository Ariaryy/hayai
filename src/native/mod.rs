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
    list_apps_folder, launch_path,
};

#[cfg(not(windows))]
pub struct NativeRuntime;

#[cfg(not(windows))]
impl NativeRuntime {
    pub fn start(_sender: std::sync::mpsc::Sender<NativeCommand>) -> Self {
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
pub fn list_apps_folder() -> Vec<(String, std::path::PathBuf)> {
    Vec::new()
}
