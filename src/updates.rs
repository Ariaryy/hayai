//! Silent update staging, applied only when the user chooses Restart to update.

use std::sync::atomic::{AtomicU8, Ordering};
// One snapshot keeps the two options consistent across native/background work.
static POLICY: AtomicU8 = AtomicU8::new(3);
static DISCOVERED: std::sync::OnceLock<String> = std::sync::OnceLock::new();
#[cfg(debug_assertions)]
static PREVIEW_READY: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub fn configure(general: &crate::settings::General) {
    POLICY.store(
        u8::from(general.check_updates) | (u8::from(general.download_updates) << 1),
        Ordering::Release,
    );
}
fn automatic_policy() -> (bool, bool) {
    let policy = POLICY.load(Ordering::Acquire);
    (policy & 1 != 0, policy & 2 != 0)
}
pub fn available_version() -> Option<String> {
    ready_version().or_else(|| DISCOVERED.get().cloned())
}
pub fn available_result() -> Option<CheckResult> {
    if ready_version().is_some() {
        Some(CheckResult::Ready)
    } else {
        DISCOVERED.get().map(|_| CheckResult::Available)
    }
}

/// Latest operation only, bounded and retained in memory for explicit copying.
pub struct DiagnosticLog(std::sync::Mutex<String>);
impl DiagnosticLog {
    pub const fn new() -> Self {
        Self(std::sync::Mutex::new(String::new()))
    }
    pub fn reset(&self, heading: &str) {
        let mut log = self.0.lock().unwrap_or_else(|error| error.into_inner());
        log.clear();
        log.push_str(heading);
    }
    pub fn append(&self, text: &str) {
        const LIMIT: usize = 64 * 1024;
        let mut log = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if log.len() > LIMIT {
            return;
        }
        let mut end = text.len().min(LIMIT - log.len());
        while !text.is_char_boundary(end) {
            end -= 1;
        }
        log.push_str(&text[..end]);
        if end < text.len() {
            log.push_str("\n[diagnostic output truncated]\n");
        }
    }
    pub fn snapshot(&self) -> String {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

pub static DIAGNOSTICS: DiagnosticLog = DiagnosticLog::new();

#[cfg(all(windows, not(debug_assertions)))]
static READY: std::sync::OnceLock<(velopack::UpdateManager, velopack::VelopackAsset)> =
    std::sync::OnceLock::new();

pub fn ready_version() -> Option<String> {
    #[cfg(debug_assertions)]
    return PREVIEW_READY
        .load(Ordering::Acquire)
        .then(|| format!("{} (preview)", env!("CARGO_PKG_VERSION")));
    #[cfg(all(windows, not(debug_assertions)))]
    return READY.get().map(|(_, asset)| asset.Version.clone());
    #[cfg(all(not(windows), not(debug_assertions)))]
    None
}

/// Start the updater off the UI thread, then let GPUI quit gracefully. Never
/// quit on failure: keeping the launcher usable is more important than updating.
pub fn prepare_restart() -> bool {
    #[cfg(all(windows, not(debug_assertions)))]
    if let Some((manager, asset)) = READY.get() {
        return manager
            .wait_exit_then_apply_updates(asset, true, true, std::iter::empty::<&str>())
            .is_ok();
    }
    false
}

#[cfg_attr(not(all(windows, not(debug_assertions))), allow(dead_code))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckResult {
    Ready,
    Available,
    Current,
    Unavailable,
    Failed,
    Busy,
}

impl CheckResult {
    pub fn message(self) -> &'static str {
        match self {
            Self::Available => crate::ui_text::updates::AVAILABLE_TO_DOWNLOAD,
            Self::Ready => "Update ready. Select the notification to restart.",
            Self::Current => "You're up to date.",
            Self::Unavailable => "Automatic updates require an installed Windows release.",
            Self::Failed => "Couldn't check for updates. Check your connection and try again.",
            Self::Busy => "An update check is already running.",
        }
    }
}

pub fn check() -> CheckResult {
    check_with_policy(false, false)
}

pub fn download() -> CheckResult {
    check_with_policy(false, true)
}

fn check_with_policy(automatic: bool, force_download: bool) -> CheckResult {
    if automatic && !automatic_policy().0 {
        return CheckResult::Unavailable;
    }
    if cfg!(debug_assertions) {
        if !force_download && !automatic_policy().1 && ready_version().is_none() {
            let _ = DISCOVERED.set(format!("{} (preview)", env!("CARGO_PKG_VERSION")));
            return CheckResult::Available;
        }
        #[cfg(debug_assertions)]
        PREVIEW_READY.store(true, Ordering::Release);
        DIAGNOSTICS.reset("Hayai update check: debug preview\nNo network check or installation was performed.\nResult: update ready (simulated)\n");
        return CheckResult::Ready;
    }
    #[cfg(all(windows, not(debug_assertions)))]
    {
        use std::sync::atomic::{AtomicBool, Ordering};
        use velopack::{UpdateCheck, UpdateManager, sources::HttpSource};
        static CHECKING: AtomicBool = AtomicBool::new(false);
        if CHECKING.swap(true, Ordering::AcqRel) {
            return CheckResult::Busy;
        }
        // The guard also clears on panic; checks must never remain stuck busy.
        struct CheckGuard;
        impl Drop for CheckGuard {
            fn drop(&mut self) {
                CHECKING.store(false, Ordering::Release);
            }
        }
        let _guard = CheckGuard;
        DIAGNOSTICS.reset(concat!(
            "Hayai ",
            env!("CARGO_PKG_VERSION"),
            " update check\nSource: https://github.com/Ariaryy/hayai/releases/latest/download\n"
        ));
        let source = HttpSource::new("https://github.com/Ariaryy/hayai/releases/latest/download");
        let manager = match UpdateManager::new(source, None, None) {
            Ok(manager) => manager,
            Err(error) => {
                DIAGNOSTICS.append(&format!("Create update manager failed:\n{error:#?}\n"));
                return CheckResult::Unavailable;
            }
        };
        if manager.get_is_portable() {
            DIAGNOSTICS.append("Result: portable build, automatic updates unavailable\n");
            return CheckResult::Unavailable;
        }
        if let Some(asset) = manager.get_update_pending_restart() {
            DIAGNOSTICS.append(&format!("Result: {} pending restart\n", asset.Version));
            let _ = READY.set((manager, asset));
            return CheckResult::Ready;
        }
        match manager.check_for_updates() {
            Ok(UpdateCheck::UpdateAvailable(update)) => {
                // Re-read after network discovery: disabling startup checks or
                // downloads must also govern work already in flight.
                let (checks, downloads) = automatic_policy();
                if automatic && !checks {
                    return CheckResult::Unavailable;
                }
                if !force_download && !downloads {
                    let _ = DISCOVERED.set(update.TargetFullRelease.Version.clone());
                    DIAGNOSTICS.append("Result: update available; automatic download disabled\n");
                    return CheckResult::Available;
                }
                if let Err(error) = manager.download_updates(&update, None) {
                    DIAGNOSTICS.append(&format!("Download failed:\n{error:#?}\n"));
                    return CheckResult::Failed;
                }
                DIAGNOSTICS.append(&format!(
                    "Result: {} downloaded and ready\n",
                    update.TargetFullRelease.Version
                ));
                let _ = READY.set((manager, update.TargetFullRelease));
                CheckResult::Ready
            }
            Ok(_) => {
                DIAGNOSTICS.append("Result: up to date\n");
                CheckResult::Current
            }
            Err(error) => {
                DIAGNOSTICS.append(&format!("Update check failed:\n{error:#?}\n"));
                CheckResult::Failed
            }
        }
    }
    #[cfg(not(all(windows, not(debug_assertions))))]
    CheckResult::Unavailable
}

pub fn start(notifications: futures::channel::mpsc::UnboundedSender<crate::native::NativeCommand>) {
    if !automatic_policy().0 {
        return;
    }
    #[cfg(debug_assertions)]
    {
        let _ = check_with_policy(true, false);
        let _ = notifications.unbounded_send(crate::native::NativeCommand::UpdateReady);
    }
    #[cfg(all(windows, not(debug_assertions)))]
    let _ = std::thread::Builder::new()
        .name("hayai-update".into())
        .spawn(move || {
            if matches!(
                check_with_policy(true, false),
                CheckResult::Ready | CheckResult::Available
            ) && automatic_policy().0
            {
                let _ = notifications.unbounded_send(crate::native::NativeCommand::UpdateReady);
            }
        });
    #[cfg(all(not(windows), not(debug_assertions)))]
    let _ = notifications;
}

#[cfg(test)]
mod tests {
    #[test]
    fn diagnostic_log_preserves_original_text_and_bounds_retained_memory() {
        let log = super::DiagnosticLog::new();
        let original = "{\"message\":\"Exception calling GetResponse: (403) Forbidden\"}\r\n";
        log.reset("check\n");
        log.append(original);
        assert_eq!(log.snapshot(), format!("check\n{original}"));
        log.reset("next check\n");
        log.append(&"é".repeat(40_000));
        let snapshot = log.snapshot();
        assert!(snapshot.len() < 65_600);
        assert!(snapshot.contains("[diagnostic output truncated]"));
        assert!(!snapshot.contains("GetResponse"));
    }
}
