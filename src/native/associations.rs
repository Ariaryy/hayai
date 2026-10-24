use super::shell::{com_release, com_vtbl_slot, extract_icon_by_index};
use super::{open_with, wide_null};
use crate::native::AssociatedApp;
use std::ffi::c_void;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::thread;
use windows_sys::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree};
use windows_sys::Win32::UI::Shell::Common::ITEMIDLIST;
use windows_sys::Win32::UI::Shell::{ILFree, SHParseDisplayName};
use windows_sys::core::{GUID, HRESULT};

struct ComObject(*mut c_void);
impl Drop for ComObject {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                com_release(self.0);
            }
        }
    }
}
struct Pidl(*mut ITEMIDLIST);
impl Drop for Pidl {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                ILFree(self.0);
            }
        }
    }
}

// Shell handlers and their dialogs require an STA. Keep COM pointers within
// that thread and return only owned strings/pixels to GPUI's worker pool.
fn in_shell_sta<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Option<T> {
    thread::Builder::new()
        .name("file-associations".into())
        .spawn(move || unsafe {
            if CoInitializeEx(null(), COINIT_APARTMENTTHREADED as u32) < 0 {
                return None;
            }
            struct Apartment;
            impl Drop for Apartment {
                fn drop(&mut self) {
                    unsafe {
                        windows_sys::Win32::System::Com::CoUninitialize();
                    }
                }
            }
            let _apartment = Apartment;
            Some(work())
        })
        .ok()?
        .join()
        .ok()
        .flatten()
}

unsafe fn handler_string(handler: *mut c_void, slot: usize) -> Option<String> {
    unsafe {
        let get: unsafe extern "system" fn(*mut c_void, *mut *mut u16) -> HRESULT =
            com_vtbl_slot(handler, slot);
        let mut text = null_mut();
        let hr = get(handler, &mut text);
        if text.is_null() {
            return None;
        }
        let mut len = 0;
        while *text.add(len) != 0 {
            len += 1;
        }
        let value =
            (hr >= 0).then(|| String::from_utf16_lossy(std::slice::from_raw_parts(text, len)));
        CoTaskMemFree(text as *const c_void);
        value
    }
}

unsafe fn visit_handlers(path: &Path, mut visit: impl FnMut(*mut c_void) -> bool) {
    unsafe {
        let Some(extension) = path.extension() else {
            return;
        };
        let extension = wide_null(&format!(".{}", extension.to_string_lossy()));
        let mut enumeration = null_mut();
        let hr = windows_sys::Win32::UI::Shell::SHAssocEnumHandlers(
            extension.as_ptr(),
            windows_sys::Win32::UI::Shell::ASSOC_FILTER_RECOMMENDED,
            &mut enumeration,
        );
        let enumeration = ComObject(enumeration);
        if hr < 0 || enumeration.0.is_null() {
            return;
        }
        let next: unsafe extern "system" fn(
            *mut c_void,
            u32,
            *mut *mut c_void,
            *mut u32,
        ) -> HRESULT = com_vtbl_slot(enumeration.0, 3);
        for _ in 0..64 {
            let mut handler = null_mut();
            let mut fetched = 0;
            let hr = next(enumeration.0, 1, &mut handler, &mut fetched);
            if handler.is_null() {
                break;
            }
            let handler = ComObject(handler);
            if hr < 0 || fetched == 0 || !visit(handler.0) {
                break;
            }
        }
    }
}

pub fn associated_apps(path: &Path) -> Vec<AssociatedApp> {
    let path = path.to_owned();
    in_shell_sta(move || unsafe {
        use windows_sys::Win32::UI::Shell::{
            ASSOCSTR_APPID, ASSOCSTR_EXECUTABLE, ASSOCSTR_FRIENDLYAPPNAME,
        };
        let default_exe = association_string(&path, ASSOCSTR_EXECUTABLE);
        let default_appid = association_string(&path, ASSOCSTR_APPID);
        let default_name = association_string(&path, ASSOCSTR_FRIENDLYAPPNAME);
        let mut default_found = false;
        let mut apps: Vec<AssociatedApp> = Vec::new();
        visit_handlers(&path, |handler| {
            let Some(id) = handler_string(handler, 3) else {
                return true;
            };
            if apps.iter().any(|app| app.id == id) {
                return true;
            }
            let name = handler_string(handler, 4).unwrap_or_else(|| id.clone());
            let is_default = default_exe
                .as_ref()
                .is_some_and(|value| value.eq_ignore_ascii_case(&id))
                || default_appid
                    .as_ref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(&id))
                || default_name
                    .as_ref()
                    .is_some_and(|value| value.eq_ignore_ascii_case(&name));
            // Still scan metadata after filling the menu, so a default returned
            // later by the enumerator can replace the last suggestion.
            if apps.len() == 6 {
                if !is_default {
                    return true;
                }
                apps.pop();
            }
            let get_icon: unsafe extern "system" fn(
                *mut c_void,
                *mut *mut u16,
                *mut i32,
            ) -> HRESULT = com_vtbl_slot(handler, 5);
            let mut icon_path = null_mut();
            let mut index = 0;
            let hr = get_icon(handler, &mut icon_path, &mut index);
            let icon = if !icon_path.is_null() {
                let mut len = 0;
                while *icon_path.add(len) != 0 {
                    len += 1;
                }
                let location = String::from_utf16_lossy(std::slice::from_raw_parts(icon_path, len));
                CoTaskMemFree(icon_path as *const c_void);
                if hr >= 0 {
                    extract_icon_by_index(Path::new(&location), index)
                } else {
                    None
                }
            } else {
                None
            };
            let app = AssociatedApp { id, name, icon };
            if is_default {
                apps.insert(0, app);
                default_found = true;
            } else {
                apps.push(app);
            }
            apps.len() < 6 || !default_found
        });
        apps
    })
    .unwrap_or_default()
}

unsafe fn association_string(
    path: &Path,
    kind: windows_sys::Win32::UI::Shell::ASSOCSTR,
) -> Option<String> {
    unsafe {
        let extension = wide_null(&format!(".{}", path.extension()?.to_string_lossy()));
        let mut length = 0;
        windows_sys::Win32::UI::Shell::AssocQueryStringW(
            0,
            kind,
            extension.as_ptr(),
            null(),
            null_mut(),
            &mut length,
        );
        if length == 0 || length > 32768 {
            return None;
        }
        let mut buffer = vec![0; length as usize];
        if windows_sys::Win32::UI::Shell::AssocQueryStringW(
            0,
            kind,
            extension.as_ptr(),
            null(),
            buffer.as_mut_ptr(),
            &mut length,
        ) < 0
        {
            return None;
        }
        let end = buffer
            .iter()
            .position(|ch| *ch == 0)
            .unwrap_or(buffer.len());
        (end > 0).then(|| String::from_utf16_lossy(&buffer[..end]))
    }
}

pub fn open_with_app(path: &Path, id: &str) {
    let path = path.to_owned();
    let id = id.to_owned();
    let fallback = path.clone();
    let opened = in_shell_sta(move || unsafe {
        use windows_sys::Win32::UI::Shell::{
            ILClone, ILFindLastID, ILRemoveLastID, SHCreateDataObject,
        };
        // IDataObject needs the parent PIDL and a single child item. Handler
        // invocation preserves packaged-app support and registered arguments.
        const IID_DATA_OBJECT: GUID = GUID {
            data1: 0x0000010e,
            data2: 0,
            data3: 0,
            data4: [0xc0, 0, 0, 0, 0, 0, 0, 0x46],
        };
        let file = wide_null(&path.to_string_lossy());
        let mut pidl = null_mut();
        let hr = SHParseDisplayName(file.as_ptr(), null_mut(), &mut pidl, 0, null_mut());
        let pidl = Pidl(pidl);
        if hr < 0 || pidl.0.is_null() {
            return false;
        }
        let parent = Pidl(ILClone(pidl.0));
        if parent.0.is_null() || ILRemoveLastID(parent.0) == 0 {
            return false;
        }
        let child = ILFindLastID(pidl.0) as *const ITEMIDLIST;
        let mut data = null_mut();
        let hr = SHCreateDataObject(parent.0, 1, &child, null_mut(), &IID_DATA_OBJECT, &mut data);
        let data = ComObject(data);
        if hr < 0 || data.0.is_null() {
            return false;
        }
        let mut opened = false;
        visit_handlers(&path, |handler| {
            if handler_string(handler, 3).as_deref() != Some(id.as_str()) {
                return true;
            }
            let invoke: unsafe extern "system" fn(*mut c_void, *mut c_void) -> HRESULT =
                com_vtbl_slot(handler, 8);
            opened = invoke(handler, data.0) >= 0;
            false
        });
        opened
    })
    .unwrap_or(false);
    if !opened {
        open_with(&fallback);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn enumerates_text_handlers_without_invoking_apps() {
        let apps = associated_apps(Path::new("association-smoke-test.txt"));
        assert!(apps.len() <= 6);
        for (index, app) in apps.iter().enumerate() {
            assert!(!app.id.is_empty());
            assert!(!app.name.is_empty());
            assert!(apps[..index].iter().all(|other| other.id != app.id));
            if let Some(icon) = &app.icon {
                assert_eq!(
                    icon.bgra.len(),
                    icon.width as usize * icon.height as usize * 4
                );
            }
        }
    }

    #[test]
    fn missing_associations_and_null_outputs_are_safe() {
        drop(ComObject(null_mut()));
        drop(Pidl(null_mut()));
        assert!(associated_apps(Path::new("no-extension")).is_empty());
        let _ = associated_apps(Path::new("test.hayai_unregistered_test_extension"));
    }
}
