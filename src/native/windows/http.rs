use super::wide_null;
use std::ffi::c_void;
use std::ptr::{null, null_mut};

/// Blocking HTTPS GET over WinHTTP. Used for the currency rate fetch
/// (~daily, on the background pool) — never called from the UI thread.
/// `path` must include the leading '/'. Returns the response body decoded
/// as UTF-8, or `None` on any connection/protocol failure.
pub fn https_get(host: &str, path: &str) -> Option<String> {
    use windows_sys::Win32::Networking::WinHttp::{
        INTERNET_DEFAULT_HTTPS_PORT, WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY, WINHTTP_FLAG_SECURE,
        WinHttpCloseHandle, WinHttpConnect, WinHttpOpen, WinHttpOpenRequest,
        WinHttpReceiveResponse, WinHttpSendRequest,
    };

    unsafe {
        let agent = wide_null("hayai/1.0");
        let session = WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_AUTOMATIC_PROXY,
            null(),
            null(),
            0,
        );
        if session.is_null() {
            return None;
        }

        let host_wide = wide_null(host);
        let connect = WinHttpConnect(session, host_wide.as_ptr(), INTERNET_DEFAULT_HTTPS_PORT, 0);
        if connect.is_null() {
            WinHttpCloseHandle(session);
            return None;
        }

        let path_wide = wide_null(path);
        let verb = wide_null("GET");
        let request = WinHttpOpenRequest(
            connect,
            verb.as_ptr(),
            path_wide.as_ptr(),
            null(),
            null(),
            null(),
            WINHTTP_FLAG_SECURE,
        );
        if request.is_null() {
            WinHttpCloseHandle(connect);
            WinHttpCloseHandle(session);
            return None;
        }

        let sent = WinHttpSendRequest(request, null(), 0, null_mut(), 0, 0, 0);
        let body = if sent != 0 && WinHttpReceiveResponse(request, null_mut()) != 0 {
            read_winhttp_body(request)
        } else {
            None
        };

        WinHttpCloseHandle(request);
        WinHttpCloseHandle(connect);
        WinHttpCloseHandle(session);
        body
    }
}

unsafe fn read_winhttp_body(request: *mut c_void) -> Option<String> {
    use windows_sys::Win32::Networking::WinHttp::{WinHttpQueryDataAvailable, WinHttpReadData};

    let mut bytes = Vec::new();
    loop {
        let mut available: u32 = 0;
        if unsafe { WinHttpQueryDataAvailable(request, &mut available) } == 0 {
            return None;
        }
        if available == 0 {
            break;
        }
        let mut buffer = vec![0u8; available as usize];
        let mut read: u32 = 0;
        if unsafe {
            WinHttpReadData(
                request,
                buffer.as_mut_ptr() as *mut c_void,
                available,
                &mut read,
            )
        } == 0
        {
            return None;
        }
        buffer.truncate(read as usize);
        bytes.extend_from_slice(&buffer);
        if read == 0 {
            break;
        }
    }
    String::from_utf8(bytes).ok()
}
