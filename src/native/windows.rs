use crate::native::NativeCommand;
use futures::channel::mpsc::UnboundedSender;
use std::path::{Path, PathBuf};
use std::ptr::{null, null_mut};
use std::sync::{OnceLock, mpsc};
use std::{io, thread};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::{
    DWMWA_TRANSITIONS_FORCEDISABLED, DwmFlush, DwmSetWindowAttribute,
};
use windows_sys::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY, MONITORINFO,
    MonitorFromPoint, MonitorFromWindow,
};
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_NOREPEAT, RegisterHotKey, SetActiveWindow, SetFocus, UnregisterHotKey,
};
use windows_sys::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
    ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, AppendMenuW, BringWindowToTop, CreatePopupMenu, CreateWindowExW,
    DefWindowProcW, DestroyMenu, DestroyWindow, DispatchMessageW, GWL_EXSTYLE, GetCursorPos,
    GetForegroundWindow, GetMessageW, GetWindowLongPtrW, GetWindowRect, HWND_MESSAGE, HWND_TOPMOST,
    LoadIconW, MF_STRING, MSG, PostMessageW, PostQuitMessage, RegisterClassW, SW_HIDE, SW_SHOW,
    SW_SHOWNORMAL, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SendMessageW,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, TPM_RETURNCMD,
    TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, WM_APP, WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP,
    WM_NULL, WM_RBUTTONUP, WNDCLASSW, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_OVERLAPPED,
};

#[path = "associations.rs"]
mod associations;
pub use associations::{associated_apps, open_with_app};

const TRAY_ICON_ID: u32 = 1;
const WM_TRAY_ICON: u32 = WM_APP + 1;
mod hotkeys;
use hotkeys::WM_SET_HOTKEY;
pub use hotkeys::set_launcher_hotkey;

// `UnboundedSender` is `Sync` for `Send` payloads and `unbounded_send` takes
// `&self` and never blocks, so no `Mutex` is needed — the send must not block
// the Win32 message loop.
static COMMAND_SENDER: OnceLock<UnboundedSender<NativeCommand>> = OnceLock::new();
static SCRY_SEARCH_SENDER: OnceLock<mpsc::Sender<ScrySearchMessage>> = OnceLock::new();
static SCRY_SESSION_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn reset_scry_search() {
    SCRY_SESSION_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Release);
}

/// Background-only barrier: release idle SDK pipe/mapping handles before the
/// elevated updater reconciles daemon ownership. Never start a worker for this.
pub fn suspend_scry_search() -> bool {
    let Some(sender) = SCRY_SEARCH_SENDER.get() else {
        return true;
    };
    let (reply, response) = mpsc::sync_channel(1);
    sender.send(ScrySearchMessage::Suspend(reply)).is_ok()
        && response
            .recv_timeout(std::time::Duration::from_secs(5))
            .is_ok()
}

/// One search result hit.
pub struct FileHit {
    pub name: String,
    pub parent: PathBuf,
    pub is_folder: bool,
    pub size: u64,
    #[allow(dead_code)]
    pub mtime: u32,
}

pub struct NativeRuntime {
    thread: Option<thread::JoinHandle<()>>,
}

impl NativeRuntime {
    pub fn start(sender: UnboundedSender<NativeCommand>, hotkey: (u32, u32)) -> Self {
        let thread = thread::spawn(move || {
            if let Err(error) = run_message_window(sender, hotkey) {
                eprintln!("Failed to start native Windows runtime: {error}");
            }
        });

        Self {
            thread: Some(thread),
        }
    }
}

impl Drop for NativeRuntime {
    fn drop(&mut self) {
        let _ = self.thread.take();
    }
}

fn hwnd_of(window: &gpui::Window) -> Option<HWND> {
    use raw_window_handle::RawWindowHandle;

    let handle = raw_window_handle::HasWindowHandle::window_handle(window).ok()?;
    if let RawWindowHandle::Win32(handle) = handle.as_raw() {
        Some(handle.hwnd.get() as HWND)
    } else {
        None
    }
}

/// Hide the launcher window without destroying it.
///
/// We must never let GPUI's window count reach zero: on Windows GPUI calls
/// `PostQuitMessage(0)` when the last window closes (see
/// `gpui::platform::windows::WindowsPlatform::close_one_window`), which would quit
/// the whole app instead of leaving it resident in the tray. So we `SW_HIDE` the
/// OS window and keep the GPUI window alive for the next toggle.
pub fn hide_launcher_window(window: &gpui::Window) {
    if let Some(hwnd) = hwnd_of(window) {
        unsafe {
            disable_launcher_transitions(hwnd);
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

unsafe fn disable_launcher_transitions(hwnd: HWND) {
    // DWM's show/hide fade can retain the previous appearance independently of
    // GPUI's reset and hidden Present. Disable only this window's transitions,
    // before either visibility change; no compositor wait or timer is needed.
    let disabled: i32 = 1;
    unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_TRANSITIONS_FORCEDISABLED as u32,
            (&disabled as *const i32).cast(),
            std::mem::size_of_val(&disabled) as u32,
        );
    }
}

/// Capture the destination before GPUI can activate the launcher and change
/// GetForegroundWindow. Pass this snapshot through reset painting and UAC return.
#[derive(Clone, Copy)]
pub struct LauncherMonitor(usize);
pub fn launcher_monitor(target: crate::settings::DisplayTarget) -> LauncherMonitor {
    use crate::settings::DisplayTarget;
    let monitor = unsafe {
        match target {
            DisplayTarget::Pointer => {
                let mut cursor = POINT { x: 0, y: 0 };
                if GetCursorPos(&mut cursor) != 0 {
                    MonitorFromPoint(cursor, MONITOR_DEFAULTTONEAREST)
                } else {
                    MonitorFromWindow(null_mut(), MONITOR_DEFAULTTOPRIMARY)
                }
            }
            DisplayTarget::ActiveWindow => {
                MonitorFromWindow(GetForegroundWindow(), MONITOR_DEFAULTTONEAREST)
            }
            DisplayTarget::Primary => MonitorFromWindow(null_mut(), MONITOR_DEFAULTTOPRIMARY),
        }
    };
    LauncherMonitor(monitor as usize)
}

/// Position while hidden, before ShowWindow, preserving the current pixel size.
/// GPUI sized the window for its creation DPI; cross-DPI resizing is unchanged.
unsafe fn reposition_on_monitor(hwnd: HWND, monitor: LauncherMonitor) {
    unsafe {
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..std::mem::zeroed()
        };
        if GetMonitorInfoW(monitor.0 as _, &mut info) == 0 {
            return;
        }
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return;
        }
        let win_w = rect.right - rect.left;
        let win_h = rect.bottom - rect.top;
        let work = info.rcWork; // excludes the taskbar
        let x = work.left + ((work.right - work.left) - win_w) / 2;
        // True vertical center of the work area — the 1/3-down and 2/5-down
        // ratios tried earlier both sat too high per user feedback.
        let y = work.top + ((work.bottom - work.top) - win_h) / 2;
        SetWindowPos(hwnd, HWND_TOPMOST, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE);
    }
}

pub fn focus_launcher_window(window: &gpui::Window, monitor: LauncherMonitor) {
    if let Some(hwnd) = hwnd_of(window) {
        unsafe {
            disable_launcher_transitions(hwnd);
            // Strip WS_EX_APPWINDOW (taskbar entry) and add WS_EX_TOOLWINDOW (no taskbar).
            // Must happen before ShowWindow so the taskbar never sees the window.
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                (ex | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize),
            );

            reposition_on_monitor(hwnd, monitor);
            ShowWindow(hwnd, SW_SHOW);
            // Apply the exstyle change and push to top of Z-order in one call.
            SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_FRAMECHANGED,
            );
            BringWindowToTop(hwnd);
            SetForegroundWindow(hwnd);
            SetActiveWindow(hwnd);
            SetFocus(hwnd);
        }
    }
}

pub fn focus_launcher_after_approval(window: &gpui::Window, monitor: LauncherMonitor) {
    use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId,
    };
    unsafe {
        let current = GetCurrentThreadId();
        let foreground = GetWindowThreadProcessId(GetForegroundWindow(), null_mut());
        // UAC can return foreground ownership to another thread. Share its
        // input queue only for this one activation, then immediately detach.
        // No timer or retained attachment may reclaim focus afterward.
        let attached = foreground != 0
            && foreground != current
            && AttachThreadInput(current, foreground, 1) != 0;
        focus_launcher_window(window, monitor);
        if attached {
            AttachThreadInput(current, foreground, 0);
        }
    }
}

pub fn render_hidden_launcher_frame(window: &gpui::Window) {
    if let Some(hwnd) = hwnd_of(window) {
        // Pinned gpui_windows/events.rs: WM_GPUI_FORCE_UPDATE_WINDOW is
        // WM_USER + 5 and draws/presents even while hidden, bypassing inactive
        // frame throttling. Post after the reset so the current window/entity
        // updates have unwound before GPUI re-enters them to paint.
        // One event per hide, never a wait or visibility gate on show.
        const WM_GPUI_FORCE_UPDATE_WINDOW: u32 = 0x0400 + 5;
        unsafe {
            PostMessageW(hwnd, WM_GPUI_FORCE_UPDATE_WINDOW, 0, 0);
        }
    }
}

pub fn launcher_has_foreground(window: &gpui::Window) -> bool {
    hwnd_of(window).is_some_and(|hwnd| unsafe { GetForegroundWindow() == hwnd })
}

/// Capture the HWND now, but invoke only after the GPUI window update unwinds.
pub fn hidden_launcher_frame_flush(window: &gpui::Window) -> impl FnOnce() + use<> {
    let hwnd = hwnd_of(window);
    move || {
        if let Some(hwnd) = hwnd {
            // The pinned backend forces both drawing and presentation for this
            // message, even when hidden. Sending outside WindowHandle::update
            // avoids re-entry and finishes the reset surface before SW_SHOW.
            unsafe {
                SendMessageW(hwnd, 0x0400 + 5, 0, 0);
                // Present(0) submits asynchronously. Do not expose DWM's old
                // surface while the reset surface is still queued on the GPU.
                // This is a compositor fence, not a guessed delay or retry loop.
                DwmFlush();
            }
        }
    }
}

const TRAY_MENU_TOGGLE: usize = 1;
const TRAY_MENU_QUIT: usize = 2;

/// Show the tray icon's right-click context menu and return the chosen
/// command id (0 if dismissed without a selection). `PostMessageW(WM_NULL)`
/// after `TrackPopupMenu` is the documented workaround for the menu failing
/// to dismiss when the click lands outside it.
unsafe fn show_tray_menu(hwnd: HWND) -> u32 {
    unsafe {
        let menu = CreatePopupMenu();
        if menu.is_null() {
            return 0;
        }
        let toggle_label = wide_null("Toggle Hayai");
        let quit_label = wide_null("Quit");
        AppendMenuW(menu, MF_STRING, TRAY_MENU_TOGGLE, toggle_label.as_ptr());
        AppendMenuW(menu, MF_STRING, TRAY_MENU_QUIT, quit_label.as_ptr());

        let mut cursor = POINT { x: 0, y: 0 };
        GetCursorPos(&mut cursor);
        SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            cursor.x,
            cursor.y,
            0,
            hwnd,
            null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        chosen as u32
    }
}

/// Launch a file/shortcut the way Explorer would (double-click semantics).
///
/// `ShellExecuteW` with a null verb resolves `.lnk` shortcuts, file
/// associations, etc., so we can pass a Start Menu `.lnk` path directly.
pub fn launch_path(path: &Path) {
    let file = wide_null(&path.to_string_lossy());
    unsafe {
        // Now called from gpui's background pool: ShellExecuteW wants COM
        // initialized on its calling thread (per its docs); repeat calls on
        // an already-initialized thread are harmless, same pattern as
        // resolve_shortcut_info above.
        let _ = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
        ShellExecuteW(
            null_mut_hwnd(),
            null(),
            file.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
    }
}

struct ScrySearchRequest {
    query: String,
    max_results: u32,
    reply: mpsc::SyncSender<Option<Vec<FileHit>>>,
}

enum ScrySearchMessage {
    Query(ScrySearchRequest),
    Suspend(mpsc::SyncSender<()>),
}

/// Open an executable through Shell's `runas` verb, which lets Windows own
/// the UAC prompt rather than attempting to elevate Hayai itself.
pub fn launch_path_as_admin(path: &Path) {
    let verb = wide_null("runas");
    let file = wide_null(&path.to_string_lossy());

    // Like `launch_path`, this runs on GPUI's background pool. ShellExecuteW
    // may need COM on its calling thread; repeated apartment initialization is
    // harmless for an already-initialized thread.
    let _ = unsafe { CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) };
    unsafe {
        ShellExecuteW(
            null_mut_hwnd(),
            verb.as_ptr(),
            file.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        );
    }
}

/// Show Windows' application picker, off the UI thread. A dedicated STA keeps
/// the shell dialog's apartment independent of GPUI's reusable worker threads.
pub fn open_with(path: &Path) {
    let file = wide_null(&path.to_string_lossy());
    let _ = std::thread::Builder::new()
        .name("open-with".into())
        .spawn(move || unsafe {
            use windows_sys::Win32::UI::Shell::{OAIF_EXEC, OPENASINFO, SHOpenWithDialog};
            let result = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);
            if result < 0 {
                return;
            }
            let info = OPENASINFO {
                pcszFile: file.as_ptr(),
                pcszClass: null(),
                // Required to open the chosen file rather than show a Settings hint.
                oaifInFlags: OAIF_EXEC,
            };
            let _ = SHOpenWithDialog(null_mut_hwnd(), &info);
            windows_sys::Win32::System::Com::CoUninitialize();
        });
}

/// Launch the bundled daemon setup through UAC. Returns false when the release
/// payload is incomplete; the setup script owns task registration and startup.
pub fn install_scry_daemon() -> bool {
    let Ok(executable) = std::env::current_exe() else {
        return false;
    };
    let Some(directory) = executable.parent() else {
        return false;
    };
    let script = directory.join("install-daemon.ps1");
    if !script.is_file() || !directory.join("scryd.exe").is_file() {
        return false;
    }

    let verb = wide_null("runas");
    let powershell = wide_null("powershell.exe");
    let parameters = wide_null(&format!(
        "-NoProfile -ExecutionPolicy Bypass -File \"{}\"",
        script.display()
    ));
    let working_directory = wide_null(&directory.to_string_lossy());
    let result = unsafe {
        ShellExecuteW(
            null_mut_hwnd(),
            verb.as_ptr(),
            powershell.as_ptr(),
            parameters.as_ptr(),
            working_directory.as_ptr(),
            SW_SHOWNORMAL,
        )
    };
    result as usize > 32
}

/// Match the per-user location used by Scry's bundled install-daemon.ps1.
/// Called only after a failed daemon query, on the background pool.
pub fn scry_is_installed() -> bool {
    std::env::var_os("LOCALAPPDATA").is_some_and(|root| {
        PathBuf::from(root)
            .join("Programs/Scry Search/bin/scryd.exe")
            .is_file()
    })
}

pub fn start_scry_daemon() -> bool {
    if crate::scry_updates::is_updating() {
        return false;
    }
    use std::os::windows::process::CommandExt;
    // Starting an installed daemon must not escalate or open the installer:
    // Hayai stays focused and shows readiness progress in its existing window.
    std::process::Command::new("schtasks.exe")
        .args(["/Run", "/TN", "Scry Search Daemon"])
        .creation_flags(0x08000000) // CREATE_NO_WINDOW: no console flash.
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

/// Query a running `scryd` daemon instance over its named pipe IPC or
/// shared memory index. Blocking; call ONLY from the background pool, never
/// the GPUI thread.
///
/// Returns `None` if `scryd` is not running or the query fails.
pub fn scry_query(query: &str, max_results: u32) -> Option<Vec<FileHit>> {
    if crate::scry_updates::is_updating() {
        return None;
    }
    let (reply, response) = mpsc::sync_channel(1);
    scry_search_sender()
        .send(ScrySearchMessage::Query(ScrySearchRequest {
            query: query.to_owned(),
            max_results,
            reply,
        }))
        .ok()?;
    response
        .recv_timeout(std::time::Duration::from_secs(5))
        .ok()
        .flatten()
}

fn scry_search_sender() -> &'static mpsc::Sender<ScrySearchMessage> {
    SCRY_SEARCH_SENDER.get_or_init(|| {
        let (sender, receiver) = mpsc::channel();
        thread::Builder::new()
            .name("hayai-scry-search".into())
            .spawn(move || run_scry_search(receiver))
            .expect("failed to start Scry search worker");
        sender
    })
}

fn run_scry_search(receiver: mpsc::Receiver<ScrySearchMessage>) {
    let mut session: Option<scry_client::SearchSession> = None;
    let mut epoch = SCRY_SESSION_EPOCH.load(std::sync::atomic::Ordering::Acquire);
    let mut active: Option<mpsc::SyncSender<Option<Vec<FileHit>>>> = None;
    loop {
        let request = if active.is_some() {
            match receiver.recv_timeout(std::time::Duration::from_millis(8)) {
                Ok(request) => Some(request),
                Err(mpsc::RecvTimeoutError::Timeout) => None,
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
            }
        } else {
            match receiver.recv() {
                Ok(request) => Some(request),
                Err(_) => break,
            }
        };

        let request = match request {
            Some(ScrySearchMessage::Suspend(reply)) => {
                session = None;
                if let Some(active) = active.take() {
                    let _ = active.try_send(None);
                }
                let _ = reply.try_send(());
                continue;
            }
            Some(ScrySearchMessage::Query(request)) => Some(request),
            None => None,
        };
        if let Some(request) = request {
            // Requests already queued before UPDATING was set must not reopen
            // a connection after the suspension barrier acknowledged closure.
            if crate::scry_updates::is_updating() {
                let _ = request.reply.try_send(None);
                continue;
            }
            let current_epoch = SCRY_SESSION_EPOCH.load(std::sync::atomic::Ordering::Acquire);
            if current_epoch != epoch {
                // An updater can replace the daemon while a shared-index
                // mapping still exists. Drop it before accepting a new query.
                session = None;
                epoch = current_epoch;
            }
            if let Some(reply) = active.take() {
                let _ = reply.try_send(Some(Vec::new()));
            }
            if session.is_none() {
                session = scry_client::SearchSession::connect(request.max_results).ok();
            }
            let Some(current) = session.as_mut() else {
                let _ = request.reply.try_send(None);
                continue;
            };
            current.set_limit(request.max_results);
            let kind = scry_query_kind(&request.query);
            if current.submit(kind, &request.query).is_err() {
                session = None;
                let _ = request.reply.try_send(None);
                continue;
            }
            active = Some(request.reply);
        }

        let Some(current) = session.as_mut() else {
            continue;
        };
        match current.poll_latest() {
            Ok(Some(entries)) => {
                if let Some(reply) = active.take() {
                    let _ = reply.try_send(Some(entries_to_hits(entries)));
                }
            }
            Ok(None) => {}
            Err(_) => {
                session = None;
                if let Some(reply) = active.take() {
                    let _ = reply.try_send(None);
                }
            }
        }
    }
}

fn scry_query_kind(query: &str) -> scry_client::QueryKind {
    if query.bytes().any(|b| matches!(b, b'*' | b'?')) {
        scry_client::QueryKind::Wildcard
    } else {
        scry_client::QueryKind::PathTerms
    }
}

fn entries_to_hits(entries: Vec<scry_client::ResultEntry>) -> Vec<FileHit> {
    entries
        .into_iter()
        .map(|entry| {
            let path = PathBuf::from(&entry.path);
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| entry.path.clone());
            let parent = path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from(""));
            FileHit {
                name,
                parent,
                is_folder: entry.is_dir,
                size: entry.size,
                mtime: entry.mtime,
            }
        })
        .collect()
}

fn run_message_window(
    sender: UnboundedSender<NativeCommand>,
    hotkey: (u32, u32),
) -> io::Result<()> {
    let _ = COMMAND_SENDER.set(sender);

    let class_name = wide_null("HayaiNativeMessageWindow");
    let window_name = wide_null("Hayai");

    let hinstance = unsafe { GetModuleHandleW(null()) };
    if hinstance.is_null() {
        return Err(io::Error::last_os_error());
    }

    let window_class = WNDCLASSW {
        lpfnWndProc: Some(window_proc),
        hInstance: hinstance,
        lpszClassName: class_name.as_ptr(),
        ..Default::default()
    };

    let atom = unsafe { RegisterClassW(&window_class) };
    if atom == 0 {
        return Err(io::Error::last_os_error());
    }

    let hwnd = unsafe {
        CreateWindowExW(
            0,
            class_name.as_ptr(),
            window_name.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            HWND_MESSAGE,
            null_mut_hwnd(),
            hinstance,
            null(),
        )
    };
    if hwnd.is_null() {
        return Err(io::Error::last_os_error());
    }
    add_tray_icon(hwnd)?;
    // A registration conflict must not take down tray access to Settings.
    hotkeys::install(hwnd, hotkey);

    let mut message = MSG::default();
    loop {
        let result = unsafe { GetMessageW(&mut message, null_mut_hwnd(), 0, 0) };
        if result == -1 {
            break Err(io::Error::last_os_error());
        }
        if result == 0 {
            break Ok(());
        }

        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}

fn add_tray_icon(hwnd: HWND) -> io::Result<()> {
    let mut data = notify_icon_data(hwnd);
    data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
    data.uCallbackMessage = WM_TRAY_ICON;
    // Resource 1 is embedded at build time. LoadIconW returns a shared handle,
    // so it must not be destroyed by the tray cleanup path.
    // Win32 MAKEINTRESOURCE encodes the numeric ID in the pointer address;
    // this is not a dereferenceable pointer (dangling::<u16>() would encode 2).
    data.hIcon = unsafe { LoadIconW(GetModuleHandleW(null()), std::ptr::without_provenance(1)) };
    if data.hIcon.is_null() {
        return Err(io::Error::last_os_error());
    }
    copy_wide_fixed(&mut data.szTip, "Hayai");

    if unsafe { Shell_NotifyIconW(NIM_ADD, &data) } == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
}

fn remove_tray_icon(hwnd: HWND) {
    let data = notify_icon_data(hwnd);
    unsafe {
        Shell_NotifyIconW(NIM_DELETE, &data);
    }
}

fn send(command: NativeCommand) {
    if let Some(sender) = COMMAND_SENDER.get() {
        let _ = sender.unbounded_send(command);
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match message {
        WM_SET_HOTKEY => hotkeys::replace(hwnd, wparam as u32, lparam as u32),
        WM_HOTKEY if hotkeys::is_active(wparam as i32) => {
            // Grant our process permission to call SetForegroundWindow from any thread.
            // Must be called here (on the thread that received WM_HOTKEY) while we hold
            // foreground permission. ASFW_ANY (u32::MAX) grants it to the whole process.
            unsafe { AllowSetForegroundWindow(u32::MAX) };
            send(NativeCommand::ShortcutPressed);
            0
        }
        WM_TRAY_ICON => {
            match lparam as u32 {
                WM_LBUTTONUP => send(NativeCommand::ToggleLauncher),
                WM_RBUTTONUP => match unsafe { show_tray_menu(hwnd) } as usize {
                    TRAY_MENU_TOGGLE => send(NativeCommand::ToggleLauncher),
                    TRAY_MENU_QUIT => {
                        send(NativeCommand::Quit);
                        unsafe {
                            DestroyWindow(hwnd);
                        }
                    }
                    _ => {}
                },
                _ => {}
            }
            0
        }

        WM_DESTROY => {
            hotkeys::unregister(hwnd);
            remove_tray_icon(hwnd);
            unsafe {
                PostQuitMessage(0);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

fn notify_icon_data(hwnd: HWND) -> NOTIFYICONDATAW {
    NOTIFYICONDATAW {
        cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ICON_ID,
        ..Default::default()
    }
}

fn copy_wide_fixed(target: &mut [u16], value: &str) {
    let wide = value.encode_utf16().take(target.len().saturating_sub(1));
    for (slot, code_unit) in target.iter_mut().zip(wide) {
        *slot = code_unit;
    }
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn null_mut_hwnd() -> HWND {
    std::ptr::null_mut()
}

#[cfg(test)]
mod search_worker_tests {
    use super::*;

    #[test]
    fn suspension_is_acknowledged_without_a_search_or_daemon() {
        let (send, receive) = mpsc::channel();
        let worker = thread::spawn(move || run_scry_search(receive));
        // An idle worker used to wait forever for a query before releasing
        // its session. Suspension must wake it independently of search.
        for _ in 0..2 {
            let (reply, response) = mpsc::sync_channel(1);
            send.send(ScrySearchMessage::Suspend(reply)).unwrap();
            response
                .recv_timeout(std::time::Duration::from_secs(2))
                .unwrap();
        }
        drop(send);
        worker.join().unwrap();
    }
}

mod shell;
pub use shell::{extract_icon_rgba, list_apps_folder};
mod http;
pub use http::https_get;
mod locale;
pub use locale::{
    local_now, local_now_seconds, system_currency_code, system_timezone_name,
    timezone_local_to_utc, timezone_utc_to_local,
};
