use std::io;
use std::ptr::{null, null_mut};
use std::sync::OnceLock;
use std::thread;

use futures::channel::mpsc::UnboundedSender;

use std::ffi::c_void;
use std::path::{Path, PathBuf};

use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits,
    GetObjectW, ReleaseDC,
};
use windows_sys::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, STGM_READ,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::core::{GUID, HRESULT, IUnknown_Vtbl};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_NOREPEAT, RegisterHotKey, SetActiveWindow, SetFocus, UnregisterHotKey, VK_SPACE,
};
use windows_sys::Win32::UI::Shell::Common::ITEMIDLIST;
use windows_sys::Win32::UI::Shell::{
    ExtractIconExW, ILCombine, ILFree, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE,
    NOTIFYICONDATAW, SHBindToObject, SHCONTF_FOLDERS, SHCONTF_INCLUDEHIDDEN, SHCONTF_NONFOLDERS,
    SHFILEINFOW, SHGFI_ICON, SHGFI_LARGEICON, SHGFI_PIDL, SHGetDesktopFolder, SHGetFileInfoW,
    SHGetNameFromIDList, SHParseDisplayName, SIGDN_DESKTOPABSOLUTEPARSING, SIGDN_NORMALDISPLAY,
    Shell_NotifyIconW, ShellExecuteW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AllowSetForegroundWindow, BringWindowToTop, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyWindow, DispatchMessageW, GWL_EXSTYLE, GetIconInfo, GetMessageW, GetWindowLongPtrW,
    HWND_MESSAGE, HWND_TOPMOST, ICONINFO, IDI_APPLICATION, LoadIconW, MSG, PostQuitMessage,
    RegisterClassW, SW_HIDE, SW_SHOW, SW_SHOWNORMAL, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage, WM_APP,
    WM_DESTROY, WM_HOTKEY, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW, WS_EX_APPWINDOW,
    WS_EX_TOOLWINDOW, WS_OVERLAPPED,
};

use super::{IconImage, NativeCommand};

const HOTKEY_ID: i32 = 1;
const TRAY_ICON_ID: u32 = 1;
const WM_TRAY_ICON: u32 = WM_APP + 1;

// `UnboundedSender` is `Sync` for `Send` payloads and `unbounded_send` takes
// `&self` and never blocks, so no `Mutex` is needed — the send must not block
// the Win32 message loop.
static COMMAND_SENDER: OnceLock<UnboundedSender<NativeCommand>> = OnceLock::new();

pub struct NativeRuntime {
    thread: Option<thread::JoinHandle<()>>,
}

impl NativeRuntime {
    pub fn start(sender: UnboundedSender<NativeCommand>) -> Self {
        let thread = thread::spawn(move || {
            if let Err(error) = run_message_window(sender) {
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
            ShowWindow(hwnd, SW_HIDE);
        }
    }
}

pub fn focus_launcher_window(window: &gpui::Window) {
    if let Some(hwnd) = hwnd_of(window) {
        unsafe {
            // Strip WS_EX_APPWINDOW (taskbar entry) and add WS_EX_TOOLWINDOW (no taskbar).
            // Must happen before ShowWindow so the taskbar never sees the window.
            let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            SetWindowLongPtrW(
                hwnd,
                GWL_EXSTYLE,
                (ex | WS_EX_TOOLWINDOW as isize) & !(WS_EX_APPWINDOW as isize),
            );

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

/// Launch a file/shortcut the way Explorer would (double-click semantics).
///
/// `ShellExecuteW` with a null verb resolves `.lnk` shortcuts, file
/// associations, etc., so we can pass a Start Menu `.lnk` path directly.
pub fn launch_path(path: &Path) {
    let file = wide_null(&path.to_string_lossy());
    unsafe {
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

const CLSID_SHELL_LINK: GUID = GUID::from_u128(0x00021401_0000_0000_C000_000000000046);
const IID_ISHELL_LINK_W: GUID = GUID::from_u128(0x000214F9_0000_0000_C000_000000000046);
const IID_IPERSIST_FILE: GUID = GUID::from_u128(0x0000010B_0000_0000_C000_000000000046);

/// Function-pointer slots we care about on `IShellLinkW`'s vtable, addressed
/// by absolute index (3 `IUnknown` slots + the interface's own method order
/// from `shobjidl_core.h`). We only ever need `GetPath` (slot 3) and
/// `GetIconLocation` (slot 16), so indexing avoids declaring the dozen
/// intervening methods (`GetIDList`, `SetDescription`, etc.) just to keep a
/// `#[repr(C)]` struct's layout correct.
const SHELL_LINK_GET_PATH: usize = 3;
const SHELL_LINK_GET_ICON_LOCATION: usize = 16;

type GetPathFn = unsafe extern "system" fn(
    this: *mut c_void,
    psz_file: *mut u16,
    cch: i32,
    pfd: *mut c_void,
    fflags: u32,
) -> HRESULT;

type GetIconLocationFn = unsafe extern "system" fn(
    this: *mut c_void,
    psz_icon_path: *mut u16,
    cch: i32,
    pi_icon: *mut i32,
) -> HRESULT;

unsafe fn com_vtbl_slot<F: Copy>(this: *mut c_void, index: usize) -> F {
    unsafe {
        let vtbl = *(this as *const *const usize);
        let slot = *vtbl.add(index);
        std::mem::transmute_copy::<usize, F>(&slot)
    }
}

#[repr(C)]
struct IPersistFileVtbl {
    base: IUnknown_Vtbl,
    get_class_id: unsafe extern "system" fn(this: *mut c_void, pclassid: *mut GUID) -> HRESULT,
    is_dirty: unsafe extern "system" fn(this: *mut c_void) -> HRESULT,
    load: unsafe extern "system" fn(this: *mut c_void, psz_file_name: *const u16, dw_mode: u32) -> HRESULT,
}

unsafe fn com_query_interface(this: *mut c_void, iid: &GUID) -> Option<*mut c_void> {
    unsafe {
        let vtbl = *(this as *const *const IUnknown_Vtbl);
        let mut out: *mut c_void = null_mut();
        let hr = ((*vtbl).QueryInterface)(this, iid, &mut out);
        (hr >= 0 && !out.is_null()).then_some(out)
    }
}

unsafe fn com_release(this: *mut c_void) {
    unsafe {
        let vtbl = *(this as *const *const IUnknown_Vtbl);
        ((*vtbl).Release)(this);
    }
}

/// What a `.lnk` shortcut tells us about where to draw its icon from.
struct ShortcutInfo {
    /// The file/exe the shortcut actually launches.
    target: Option<PathBuf>,
    /// An explicit icon override (file, icon index), if the shortcut set one.
    ///
    /// Squirrel/Electron-style installers (Discord, WhatsApp, most Chrome PWA
    /// shortcuts) point `target` at a small updater/proxy exe that has no
    /// icon of its own, and instead set this to the app's real `.ico`/exe.
    /// Falling back to `target`'s icon for these shows nothing, so this must
    /// be tried first.
    icon_location: Option<(PathBuf, i32)>,
}

/// Inspect a `.lnk` shortcut via `IShellLinkW` + `IPersistFile`. Returns
/// `None` for non-shortcuts or on any COM failure, so callers can fall back
/// to treating `path` as a plain file.
fn resolve_shortcut_info(path: &Path) -> Option<ShortcutInfo> {
    let is_lnk = path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("lnk"));
    if !is_lnk {
        return None;
    }

    unsafe {
        // Safe to call repeatedly / after another component already
        // initialized COM on this thread; we only care that some apartment
        // exists before CoCreateInstance.
        let _ = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);

        let mut shell_link: *mut c_void = null_mut();
        let hr = CoCreateInstance(
            &CLSID_SHELL_LINK,
            null_mut(),
            CLSCTX_INPROC_SERVER,
            &IID_ISHELL_LINK_W,
            &mut shell_link,
        );
        if hr < 0 || shell_link.is_null() {
            return None;
        }

        let info = (|| {
            let persist_file = com_query_interface(shell_link, &IID_IPERSIST_FILE)?;
            let wide_path = wide_null(&path.to_string_lossy());
            let vtbl = *(persist_file as *const *const IPersistFileVtbl);
            let hr = ((*vtbl).load)(persist_file, wide_path.as_ptr(), STGM_READ);
            com_release(persist_file);
            if hr < 0 {
                return None;
            }

            let get_path: GetPathFn = com_vtbl_slot(shell_link, SHELL_LINK_GET_PATH);
            let mut path_buffer = [0u16; 260]; // MAX_PATH
            let hr = get_path(
                shell_link,
                path_buffer.as_mut_ptr(),
                path_buffer.len() as i32,
                null_mut(),
                0,
            );
            let target = (hr >= 0)
                .then(|| utf16_buffer_to_path(&path_buffer))
                .flatten();

            let get_icon_location: GetIconLocationFn =
                com_vtbl_slot(shell_link, SHELL_LINK_GET_ICON_LOCATION);
            let mut icon_buffer = [0u16; 260];
            let mut icon_index: i32 = 0;
            let hr = get_icon_location(
                shell_link,
                icon_buffer.as_mut_ptr(),
                icon_buffer.len() as i32,
                &mut icon_index,
            );
            let icon_location = (hr >= 0)
                .then(|| utf16_buffer_to_path(&icon_buffer))
                .flatten()
                .map(|path| (path, icon_index));

            Some(ShortcutInfo {
                target,
                icon_location,
            })
        })();

        com_release(shell_link);
        info
    }
}

fn utf16_buffer_to_path(buffer: &[u16]) -> Option<PathBuf> {
    let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    (len > 0).then(|| PathBuf::from(String::from_utf16_lossy(&buffer[..len])))
}

const IID_ISHELL_FOLDER: GUID = GUID::from_u128(0x000214E6_0000_0000_C000_000000000046);

// `IShellFolder`/`IEnumIDList` vtable slots we need, addressed the same way as
// `IShellLinkW` above (absolute index = 3 `IUnknown` slots + method order
// from `shobjidl_core.h`). `IShellFolder::EnumObjects` is method 1, so slot 4;
// `IEnumIDList::Next` is method 0, so slot 3.
const SHELL_FOLDER_ENUM_OBJECTS: usize = 4;
const ENUM_ID_LIST_NEXT: usize = 3;

type EnumObjectsFn = unsafe extern "system" fn(
    this: *mut c_void,
    hwnd: HWND,
    grf_flags: i32,
    enum_id_list: *mut *mut c_void,
) -> HRESULT;

type EnumNextFn = unsafe extern "system" fn(
    this: *mut c_void,
    celt: u32,
    rgelt: *mut *mut ITEMIDLIST,
    fetched: *mut u32,
) -> HRESULT;

/// Enumerate `shell:AppsFolder` — the virtual namespace Explorer's own Start
/// Menu search reads from. Unlike the `.lnk` scan, this also surfaces
/// packaged (MSIX/UWP/Store) apps, which never get a physical shortcut file
/// under the Start Menu folders (e.g. NanaZip, most Store-distributed apps).
///
/// Returns `(display name, launch path)` pairs. The "launch path" is a shell
/// parsing string (like `::{4234d49b-...}\PackageFamily!App`), not a real
/// filesystem path, but `ShellExecuteW` and `SHGetFileInfoW` both accept it
/// the same way they accept a `.lnk` path.
pub fn list_apps_folder() -> Vec<(String, PathBuf)> {
    let mut apps = Vec::new();

    unsafe {
        let _ = CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32);

        let mut desktop_folder: *mut c_void = null_mut();
        if SHGetDesktopFolder(&mut desktop_folder) < 0 || desktop_folder.is_null() {
            return apps;
        }

        let apps_folder_name = wide_null("shell:AppsFolder");
        let mut apps_folder_pidl: *mut ITEMIDLIST = null_mut();
        let hr = SHParseDisplayName(
            apps_folder_name.as_ptr(),
            null_mut(),
            &mut apps_folder_pidl,
            0,
            null_mut(),
        );
        if hr < 0 || apps_folder_pidl.is_null() {
            com_release(desktop_folder);
            return apps;
        }

        let mut apps_folder: *mut c_void = null_mut();
        let hr = SHBindToObject(
            desktop_folder,
            apps_folder_pidl,
            null_mut(),
            &IID_ISHELL_FOLDER,
            &mut apps_folder,
        );
        com_release(desktop_folder);
        if hr < 0 || apps_folder.is_null() {
            ILFree(apps_folder_pidl);
            return apps;
        }

        let enum_objects: EnumObjectsFn = com_vtbl_slot(apps_folder, SHELL_FOLDER_ENUM_OBJECTS);
        let mut enum_list: *mut c_void = null_mut();
        let hr = enum_objects(
            apps_folder,
            null_mut(),
            SHCONTF_FOLDERS | SHCONTF_NONFOLDERS | SHCONTF_INCLUDEHIDDEN,
            &mut enum_list,
        );
        if hr >= 0 && !enum_list.is_null() {
            let next: EnumNextFn = com_vtbl_slot(enum_list, ENUM_ID_LIST_NEXT);
            loop {
                let mut child_pidl: *mut ITEMIDLIST = null_mut();
                let mut fetched: u32 = 0;
                let hr = next(enum_list, 1, &mut child_pidl, &mut fetched);
                if hr < 0 || fetched == 0 || child_pidl.is_null() {
                    break;
                }

                let absolute = ILCombine(apps_folder_pidl, child_pidl);
                ILFree(child_pidl);
                if !absolute.is_null() {
                    if let Some(entry) = app_entry_from_pidl(absolute) {
                        apps.push(entry);
                    }
                    ILFree(absolute);
                }
            }
            com_release(enum_list);
        }

        com_release(apps_folder);
        ILFree(apps_folder_pidl);
    }

    apps
}

unsafe fn app_entry_from_pidl(pidl: *const ITEMIDLIST) -> Option<(String, PathBuf)> {
    unsafe {
        let mut name_ptr: *mut u16 = null_mut();
        if SHGetNameFromIDList(pidl, SIGDN_NORMALDISPLAY, &mut name_ptr) < 0 || name_ptr.is_null()
        {
            return None;
        }
        let name = pwstr_to_string(name_ptr);
        CoTaskMemFree(name_ptr as *const c_void);

        let mut parse_ptr: *mut u16 = null_mut();
        if SHGetNameFromIDList(pidl, SIGDN_DESKTOPABSOLUTEPARSING, &mut parse_ptr) < 0
            || parse_ptr.is_null()
        {
            return None;
        }
        let parsing = pwstr_to_string(parse_ptr);
        CoTaskMemFree(parse_ptr as *const c_void);

        if name.is_empty() || parsing.is_empty() {
            return None;
        }

        // For packaged (MSIX/UWP) apps, `SIGDN_DESKTOPABSOLUTEPARSING` hands
        // back a bare AUMID (e.g. `Publisher.App_hash!App`), not something
        // `ShellExecuteW` can resolve on its own — it needs the `shell:`
        // prefix, same as `explorer.exe shell:AppsFolder\<AUMID>`. Regular
        // (non-packaged) items return a real absolute path already, which
        // must be left alone.
        let launch_path = if Path::new(&parsing).is_absolute() {
            PathBuf::from(parsing)
        } else {
            PathBuf::from(format!("shell:AppsFolder\\{parsing}"))
        };

        Some((name, launch_path))
    }
}

unsafe fn pwstr_to_string(ptr: *mut u16) -> String {
    unsafe {
        let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
        String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len))
    }
}

/// Extract the large icon for a file/shortcut as top-down BGRA pixels.
///
/// For `.lnk` shortcuts, prefers the icon the shortcut explicitly declares
/// (`IconLocation`) over the one on its target: Squirrel/Electron installers
/// commonly point the shortcut at an updater exe with no icon of its own and
/// set the real icon separately. Falls back to the resolved target (avoids
/// `SHGetFileInfoW`'s shortcut-arrow overlay, which is composited into the
/// bitmap and can't otherwise be turned off), then to the original path.
///
/// Runs on the GPUI/main thread and is meant to be called lazily + cached;
/// each call touches GDI and must clean up every handle it creates or we
/// leak kernel objects.
pub fn extract_icon_rgba(path: &Path) -> Option<IconImage> {
    let info = resolve_shortcut_info(path);

    if let Some((icon_path, icon_index)) = info.as_ref().and_then(|info| info.icon_location.as_ref()) {
        if let Some(image) = extract_icon_by_index(icon_path, *icon_index) {
            return Some(image);
        }
    }

    let target = info
        .as_ref()
        .and_then(|info| info.target.as_deref())
        .unwrap_or(path);
    extract_icon_via_shell(target).or_else(|| {
        if target == path {
            None
        } else {
            extract_icon_via_shell(path)
        }
    })
}

/// Extract an icon by index from an ico/exe/dll file, as `ExtractIconExW`
/// (not `SHGetFileInfoW`) understands `IconLocation` strings.
fn extract_icon_by_index(path: &Path, index: i32) -> Option<IconImage> {
    let wide = wide_null(&path.to_string_lossy());
    unsafe {
        let mut large: windows_sys::Win32::UI::WindowsAndMessaging::HICON = std::ptr::null_mut();
        let mut small: windows_sys::Win32::UI::WindowsAndMessaging::HICON = std::ptr::null_mut();
        let extracted = ExtractIconExW(wide.as_ptr(), index, &mut large, &mut small, 1);
        if extracted == 0 || large.is_null() {
            if !small.is_null() {
                DestroyIcon(small);
            }
            return None;
        }

        let image = hicon_to_bgra(large);
        DestroyIcon(large);
        if !small.is_null() {
            DestroyIcon(small);
        }
        image
    }
}

fn extract_icon_via_shell(path: &Path) -> Option<IconImage> {
    let path_str = path.to_string_lossy();
    // `shell:AppsFolder\...` (and other `shell:`-prefixed virtual paths from
    // `list_apps_folder`) aren't real files, so `SHGetFileInfoW` can't resolve
    // them as a plain filename — it needs the PIDL form instead, which also
    // correctly triggers packaged apps' shell icon handler.
    if path_str.len() >= 6 && path_str[..6].eq_ignore_ascii_case("shell:") {
        return extract_icon_via_shell_pidl(&path_str);
    }

    let wide = wide_null(&path_str);
    unsafe {
        let mut info: SHFILEINFOW = std::mem::zeroed();
        let ok = SHGetFileInfoW(
            wide.as_ptr(),
            0,
            &mut info,
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_LARGEICON,
        );
        if ok == 0 || info.hIcon.is_null() {
            return None;
        }

        let image = hicon_to_bgra(info.hIcon);
        DestroyIcon(info.hIcon);
        image
    }
}

fn extract_icon_via_shell_pidl(shell_path: &str) -> Option<IconImage> {
    let wide = wide_null(shell_path);
    unsafe {
        let mut pidl: *mut ITEMIDLIST = null_mut();
        let hr = SHParseDisplayName(wide.as_ptr(), null_mut(), &mut pidl, 0, null_mut());
        if hr < 0 || pidl.is_null() {
            return None;
        }

        let mut info: SHFILEINFOW = std::mem::zeroed();
        let ok = SHGetFileInfoW(
            pidl as *const u16,
            0,
            &mut info,
            std::mem::size_of::<SHFILEINFOW>() as u32,
            SHGFI_PIDL | SHGFI_ICON | SHGFI_LARGEICON,
        );
        ILFree(pidl);
        if ok == 0 || info.hIcon.is_null() {
            return None;
        }

        let image = hicon_to_bgra(info.hIcon);
        DestroyIcon(info.hIcon);
        image
    }
}

unsafe fn hicon_to_bgra(hicon: windows_sys::Win32::UI::WindowsAndMessaging::HICON) -> Option<IconImage> {
    let mut icon_info: ICONINFO = unsafe { std::mem::zeroed() };
    if unsafe { GetIconInfo(hicon, &mut icon_info) } == 0 {
        return None;
    }
    // GetIconInfo hands us two bitmaps we now own and must delete.
    let color_bmp = icon_info.hbmColor;
    let mask_bmp = icon_info.hbmMask;

    let result = (|| {
        if color_bmp.is_null() {
            return None;
        }

        let mut bmp: BITMAP = unsafe { std::mem::zeroed() };
        let got = unsafe {
            GetObjectW(
                color_bmp,
                std::mem::size_of::<BITMAP>() as i32,
                &mut bmp as *mut _ as *mut c_void,
            )
        };
        if got == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
            return None;
        }

        let width = bmp.bmWidth;
        let height = bmp.bmHeight;

        let mut header: BITMAPINFO = unsafe { std::mem::zeroed() };
        header.bmiHeader.biSize = std::mem::size_of::<BITMAPINFOHEADER>() as u32;
        header.bmiHeader.biWidth = width;
        // Negative height requests a top-down DIB (row 0 is the top row),
        // matching the orientation GPUI/`image` expect.
        header.bmiHeader.biHeight = -height;
        header.bmiHeader.biPlanes = 1;
        header.bmiHeader.biBitCount = 32;
        header.bmiHeader.biCompression = BI_RGB as u32;

        let mut pixels = vec![0u8; (width as usize) * (height as usize) * 4];
        let hdc = unsafe { GetDC(null_mut_hwnd()) };
        if hdc.is_null() {
            return None;
        }
        let scanlines = unsafe {
            GetDIBits(
                hdc,
                color_bmp,
                0,
                height as u32,
                pixels.as_mut_ptr() as *mut c_void,
                &mut header,
                DIB_RGB_COLORS,
            )
        };
        unsafe { ReleaseDC(null_mut_hwnd(), hdc) };
        if scanlines == 0 {
            return None;
        }

        // Some icons come back with a fully-zero alpha channel (they relied on
        // the mask bitmap for transparency). Treat that as fully opaque so the
        // icon isn't rendered invisible.
        if pixels.chunks_exact(4).all(|px| px[3] == 0) {
            for px in pixels.chunks_exact_mut(4) {
                px[3] = 255;
            }
        }

        Some(IconImage {
            width: width as u32,
            height: height as u32,
            bgra: pixels,
        })
    })();

    if !color_bmp.is_null() {
        unsafe { DeleteObject(color_bmp) };
    }
    if !mask_bmp.is_null() {
        unsafe { DeleteObject(mask_bmp) };
    }

    result
}

fn run_message_window(sender: UnboundedSender<NativeCommand>) -> io::Result<()> {
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
    register_hotkey(hwnd)?;

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
    data.hIcon = unsafe { LoadIconW(null_mut_hwnd(), IDI_APPLICATION) };
    copy_wide_fixed(&mut data.szTip, "Hayai - Alt+Space");

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

fn register_hotkey(hwnd: HWND) -> io::Result<()> {
    let modifiers = MOD_ALT | MOD_NOREPEAT;
    if unsafe { RegisterHotKey(hwnd, HOTKEY_ID, modifiers, VK_SPACE as u32) } == 0 {
        return Err(io::Error::last_os_error());
    }

    Ok(())
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
        WM_HOTKEY if wparam as i32 == HOTKEY_ID => {
            // Grant our process permission to call SetForegroundWindow from any thread.
            // Must be called here (on the thread that received WM_HOTKEY) while we hold
            // foreground permission. ASFW_ANY (u32::MAX) grants it to the whole process.
            unsafe { AllowSetForegroundWindow(u32::MAX) };
            send(NativeCommand::ToggleLauncher);
            0
        }
        WM_TRAY_ICON => {
            match lparam as u32 {
                WM_LBUTTONUP => send(NativeCommand::ToggleLauncher),
                WM_RBUTTONUP => {
                    send(NativeCommand::Quit);
                    unsafe {
                        DestroyWindow(hwnd);
                    }
                }
                _ => {}
            }
            0
        }
        WM_DESTROY => {
            unsafe {
                UnregisterHotKey(hwnd, HOTKEY_ID);
            }
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
