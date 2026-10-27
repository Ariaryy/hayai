#[derive(Clone, Copy, Debug)]
pub enum NativeCommand {
    UpdateReady,
    ToggleLauncher,
    ShortcutPressed,
    ShortcutUnavailable,
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

pub struct AssociatedApp {
    pub id: String,
    pub name: String,
    pub icon: Option<IconImage>,
}

#[cfg(windows)]
mod windows;

#[cfg(windows)]
mod scry_status;

#[cfg(windows)]
pub use scry_status::probe as scry_compatibility;
#[cfg(windows)]
pub use scry_status::{invalidate as invalidate_scry_status, version as scry_version};

#[cfg(not(windows))]
pub fn scry_version() -> Option<String> {
    None
}
#[cfg(not(windows))]
pub fn invalidate_scry_status() {}

#[cfg(not(windows))]
pub fn scry_compatibility() -> crate::scry::Compatibility {
    crate::scry::Compatibility::Unavailable
}

#[cfg(windows)]
pub use windows::{
    NativeRuntime, associated_apps, extract_icon_rgba, focus_launcher_after_approval,
    focus_launcher_window, hidden_launcher_frame_flush, hide_launcher_window, https_get,
    install_scry_daemon, launch_path, launch_path_as_admin, launcher_has_foreground,
    launcher_monitor, list_apps_folder, local_now, local_now_seconds, open_with, open_with_app,
    render_hidden_launcher_frame, reset_scry_search, scry_is_installed, scry_query,
    set_launcher_hotkey, start_scry_daemon, suspend_scry_search, system_currency_code,
    system_timezone_name, timezone_local_to_utc, timezone_utc_to_local,
};

#[cfg(not(windows))]
pub struct NativeRuntime;

#[cfg(not(windows))]
impl NativeRuntime {
    pub fn start(
        _sender: futures::channel::mpsc::UnboundedSender<NativeCommand>,
        _hotkey: (u32, u32),
    ) -> Self {
        eprintln!(
            "Hayai native tray and global hotkey support is currently implemented on Windows."
        );
        Self
    }
}

#[cfg(not(windows))]
pub fn set_launcher_hotkey(_hotkey: (u32, u32)) -> anyhow::Result<()> {
    Ok(())
}

#[cfg(not(windows))]
#[derive(Clone, Copy)]
pub struct LauncherMonitor;
#[cfg(not(windows))]
pub fn launcher_monitor(_: crate::settings::DisplayTarget) -> LauncherMonitor {
    LauncherMonitor
}

#[cfg(not(windows))]
pub fn focus_launcher_window(_window: &gpui::Window, _: LauncherMonitor) {}

#[cfg(not(windows))]
pub fn focus_launcher_after_approval(_window: &gpui::Window, _: LauncherMonitor) {}

#[cfg(not(windows))]
pub fn hide_launcher_window(_window: &gpui::Window) {}

#[cfg(not(windows))]
pub fn render_hidden_launcher_frame(_window: &gpui::Window) {}

#[cfg(not(windows))]
pub fn hidden_launcher_frame_flush(_window: &gpui::Window) -> impl FnOnce() + use<> {
    || {}
}

#[cfg(not(windows))]
pub fn extract_icon_rgba(_path: &std::path::Path) -> Option<IconImage> {
    None
}

#[cfg(not(windows))]
pub fn launch_path(_path: &std::path::Path) {}

#[cfg(not(windows))]
pub fn launch_path_as_admin(_path: &std::path::Path) {}

#[cfg(not(windows))]
pub fn open_with(_path: &std::path::Path) {}

#[cfg(not(windows))]
pub fn associated_apps(_path: &std::path::Path) -> Vec<AssociatedApp> {
    Vec::new()
}

#[cfg(not(windows))]
pub fn open_with_app(_path: &std::path::Path, _id: &str) {}

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
pub fn reset_scry_search() {}

#[cfg(not(windows))]
pub fn suspend_scry_search() -> bool {
    true
}

#[cfg(not(windows))]
pub fn install_scry_daemon() -> bool {
    false
}

#[cfg(not(windows))]
pub fn scry_is_installed() -> bool {
    false
}

#[cfg(not(windows))]
pub fn start_scry_daemon() -> bool {
    false
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
pub fn system_timezone_name() -> Option<String> {
    None
}

#[cfg(not(windows))]
pub fn timezone_local_to_utc(
    _key_name: &str,
    _date_time: (i32, u32, u32, u32, u32),
) -> Option<(i32, u32, u32, u32, u32)> {
    None
}

#[cfg(not(windows))]
pub fn timezone_utc_to_local(
    _key_name: &str,
    _date_time: (i32, u32, u32, u32, u32),
) -> Option<(i32, u32, u32, u32, u32)> {
    None
}

#[cfg(not(windows))]
pub fn local_now() -> (i32, u32, u32, u32, u32) {
    (1970, 1, 1, 0, 0)
}

#[cfg(not(windows))]
pub fn local_now_seconds() -> (i32, u32, u32, u32, u32, u32) {
    (1970, 1, 1, 0, 0, 0)
}

#[cfg(not(windows))]
pub fn launcher_has_foreground(window: &gpui::Window) -> bool {
    window.is_window_active()
}
