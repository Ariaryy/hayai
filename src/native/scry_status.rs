//! A separate, bounded pipe probe with Hayai's corroborated legacy detection.
//! No worker thread or new runtime dependency; called on the background pool.

use crate::scry::{self, Compatibility};
use std::{
    ffi::c_void,
    fs::{File, OpenOptions},
    io::{Read, Write},
    os::windows::io::AsRawHandle,
    sync::Mutex,
    time::{Duration, Instant},
};

#[link(name = "kernel32")]
unsafe extern "system" {
    fn SetNamedPipeHandleState(
        pipe: *mut c_void,
        mode: *const u32,
        count: *const u32,
        timeout: *const u32,
    ) -> i32;
    fn PeekNamedPipe(
        pipe: *mut c_void,
        buffer: *mut c_void,
        size: u32,
        read: *mut u32,
        available: *mut u32,
        remaining: *mut u32,
    ) -> i32;
    fn GetNamedPipeServerProcessId(pipe: *mut c_void, pid: *mut u32) -> i32;
}

static CACHE: Mutex<Option<(u32, Instant, Compatibility)>> = Mutex::new(None);
static VERSION: Mutex<Option<String>> = Mutex::new(None);

pub fn version() -> Option<String> {
    VERSION
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
}

pub fn invalidate() {
    *CACHE.lock().unwrap_or_else(|error| error.into_inner()) = None;
    *VERSION.lock().unwrap_or_else(|error| error.into_inner()) = None;
}

enum ReplyError {
    SilentTimeout,
    Invalid,
    Unavailable,
}

fn connect() -> Option<(File, u32)> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(r"\\.\pipe\scry")
        .ok()?;
    let mut pid = 0;
    let mode = 1u32; // PIPE_NOWAIT: an unresponsive legacy daemon cannot strand a pool thread.
    unsafe {
        if SetNamedPipeHandleState(
            file.as_raw_handle(),
            &mode,
            std::ptr::null(),
            std::ptr::null(),
        ) == 0
            || GetNamedPipeServerProcessId(file.as_raw_handle(), &mut pid) == 0
        {
            return None;
        }
    }
    Some((file, pid))
}

fn exchange(file: &mut File, kind: u8, pattern: &[u8]) -> Result<Vec<u8>, ReplyError> {
    let mut request = Vec::with_capacity(14 + pattern.len());
    request.extend((10u32 + pattern.len() as u32).to_le_bytes());
    request.push(kind);
    request.extend([0u8; 5]); // limit=0, relevance order=0
    request.extend((pattern.len() as u32).to_le_bytes());
    request.extend(pattern);
    file.write_all(&request)
        .map_err(|_| ReplyError::Unavailable)?;
    let deadline = Instant::now() + Duration::from_millis(250);
    let mut header = [0u8; 4];
    let mut response = [0u8; 85];
    loop {
        let mut available = 0;
        let mut read = 0;
        let ok = unsafe {
            PeekNamedPipe(
                file.as_raw_handle(),
                header.as_mut_ptr().cast(),
                4,
                &mut read,
                &mut available,
                std::ptr::null_mut(),
            )
        };
        if ok == 0 {
            return Err(ReplyError::Unavailable);
        }
        if read == 4 {
            let length = u32::from_le_bytes(header) as usize;
            if length > scry::MAX_STATUS_BYTES {
                return Err(ReplyError::Invalid);
            }
            if available as usize >= length + 4 {
                file.read_exact(&mut header)
                    .map_err(|_| ReplyError::Unavailable)?;
                file.read_exact(&mut response[..length])
                    .map_err(|_| ReplyError::Unavailable)?;
                return Ok(response[..length].to_vec());
            }
        }
        if Instant::now() >= deadline {
            return Err(if available == 0 {
                ReplyError::SilentTimeout
            } else {
                ReplyError::Invalid
            });
        }
        std::thread::sleep(Duration::from_millis(8));
    }
}

pub fn probe() -> Compatibility {
    let Some((mut file, pid)) = connect() else {
        *VERSION.lock().unwrap_or_else(|error| error.into_inner()) = None;
        return Compatibility::Unavailable;
    };
    let mut cache = CACHE.lock().unwrap_or_else(|error| error.into_inner());
    if let Some((cached_pid, checked, result)) = *cache
        && cached_pid == pid
        && checked.elapsed() < Duration::from_secs(10)
    {
        return result;
    }
    *VERSION.lock().unwrap_or_else(|error| error.into_inner()) = None;
    let result = match exchange(&mut file, 6, scry::STATUS_MAGIC) {
        Ok(response) => {
            *VERSION.lock().unwrap_or_else(|error| error.into_inner()) =
                scry::application_version(&response).map(str::to_owned);
            scry::classify_response(&response)
        }
        Err(ReplyError::Invalid) => Compatibility::InvalidResponse,
        Err(ReplyError::Unavailable) => Compatibility::Unavailable,
        Err(ReplyError::SilentTimeout) => {
            // A dedicated second connection confirms the same process still
            // answers an ordinary zero-result query. Neither probe touches the
            // interactive search connection, even on pre-handshake releases.
            let Some((mut health, healthy_pid)) = connect() else {
                return Compatibility::Unavailable;
            };
            if healthy_pid != pid {
                return Compatibility::Unavailable;
            }
            match exchange(&mut health, 0, b"") {
                Ok(response) => scry::classify_missing_handshake(&response),
                Err(_) => Compatibility::Unavailable,
            }
        }
    };
    // Only stable classifications are cached; transient failures retry on the
    // next request. A daemon restart changes PID and bypasses the cache.
    if matches!(
        result,
        Compatibility::Ready | Compatibility::UpdateRequired | Compatibility::NewerDaemon
    ) {
        *cache = Some((pid, Instant::now(), result));
    }
    result
}
