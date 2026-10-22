//! Host-owned update cadence. Scry owns release selection and installation.
//! Invoked on the background pool; no network work runs in the daemon or search.

use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Read},
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

const MAX_LINE: usize = 16 * 1024;
const CACHE_SECONDS: u64 = 24 * 60 * 60;
const MANUAL_CHECK_SECONDS: u64 = 5 * 60;
const FAILURE_CACHE_SECONDS: u64 = 15 * 60;
const RATE_LIMIT_CACHE_SECONDS: u64 = 60 * 60;
/// Required installs use the release tested with Hayai's locked SDK. Optional
/// improvements still use explicit compatible-release discovery.
pub const REQUIRED_VERSION: &str = "v0.1.1";
static CHECK_LOCK: Mutex<()> = Mutex::new(());
static UPDATING: AtomicBool = AtomicBool::new(false);
pub static DIAGNOSTICS: crate::updates::DiagnosticLog = crate::updates::DiagnosticLog::new();

pub struct Progress {
    pub message: String,
    /// Emitted by the elevated helper after approval, rather than inferred
    /// from human-readable progress text.
    pub active: bool,
}

pub fn is_updating() -> bool {
    UPDATING.load(Ordering::Acquire)
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum State {
    Available,
    Current,
    NoCompatibleRelease,
    Failed,
}

#[derive(Clone, Debug)]
pub struct Discovery {
    pub state: State,
    pub target: Option<String>,
    pub message: String,
}

impl Discovery {
    fn failed(message: impl Into<String>) -> Self {
        let detail = message.into();
        Self {
            state: State::Failed,
            target: None,
            message: check_error_message(&detail, None).into(),
        }
    }
}

fn check_error_message(detail: &str, exit: Option<i32>) -> &'static str {
    if detail.contains("does not include the Scry updater") || detail.contains("older Scry CLI") {
        "Scry Search updates require a newer Hayai build."
    } else if detail.contains("403") || detail.contains("429") {
        "Couldn't check for Scry Search updates right now. Try again later."
    } else if exit == Some(22) {
        "Couldn't verify Scry Search update information. Try again later."
    } else {
        "Couldn't check for Scry Search updates. Check your connection and try again."
    }
}

fn valid_tag(tag: &str) -> bool {
    (2..=65).contains(&tag.len())
        && tag.starts_with('v')
        && tag.as_bytes()[1].is_ascii_digit()
        && tag
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-".contains(&b))
}

fn install_error_message(exit: i32, detail: &str) -> &'static str {
    match exit {
        20 => "Administrator approval was cancelled.",
        21 if detail.contains("403") || detail.contains("429") => {
            "GitHub refused the download. Try again later."
        }
        21 => "Download failed. Check your connection.",
        22 => "Download verification failed. Installation stopped.",
        23 if detail.contains("legacy daemon must be stopped") => {
            "Stop the existing daemon and retry."
        }
        23 if detail.contains("reconcile the query pipe owner") => {
            "File search ownership couldn't be verified."
        }
        23 => "Couldn't safely restart file search. Open details.",
        24 => "Recovery needed. Retry the update.",
        25 => "Daemon didn't become ready. Try again.",
        26 => "This release isn't compatible with your installation.",
        _ => "Installation failed. Open Actions for the log.",
    }
}

fn message(value: &Value) -> Result<String, String> {
    let text = value["message"].as_str().ok_or("Missing Scry message")?;
    if text.len() > 4096 || text.chars().any(|c| c.is_control() && c != '\n') {
        return Err("Invalid Scry message".into());
    }
    Ok(text.to_owned())
}

fn envelope(value: &Value, event: &str) -> Result<(), String> {
    if value["schema"] != 1 || value["event"] != event {
        return Err("Unsupported Scry updater response".into());
    }
    Ok(())
}

fn parse_discovery(value: &Value, exit: i32) -> Result<Discovery, String> {
    envelope(value, "check_update")?;
    if value["exit_code"].as_i64() != Some(exit as i64) {
        return Err("Scry result did not match its process exit code".into());
    }
    let state = match (value["state"].as_str(), exit) {
        (Some("available"), 0) => State::Available,
        (Some("current"), 0) => State::Current,
        (Some("no_compatible_release"), 12) => State::NoCompatibleRelease,
        (Some("failed"), 1 | 21 | 22) => State::Failed,
        _ => return Err("Invalid Scry discovery result".into()),
    };
    let target = if matches!(state, State::Available | State::Current) {
        let tag = value["target_version"]
            .as_str()
            .filter(|tag| valid_tag(tag))
            .ok_or("Invalid Scry release tag")?;
        let detail = &value["detail"];
        if detail["schema"] != 1
            || detail["ipc_version"] != 1
            || detail["index_format"] != 10
            || detail["version"] != tag[1..]
        {
            return Err("Scry selected an unsupported release".into());
        }
        Some(tag.to_owned())
    } else {
        if !value["target_version"].is_null() {
            return Err("Unexpected Scry release tag".into());
        }
        None
    };
    let detail = message(value)?;
    let display_message = if state == State::Failed {
        check_error_message(&detail, Some(exit)).to_owned()
    } else {
        detail
    };
    Ok(Discovery {
        state,
        target,
        message: display_message,
    })
}

fn cli() -> Result<PathBuf, String> {
    // Never resolve through PATH: packaged Hayai invokes its own bundled CLI.
    let path = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .with_file_name("scry.exe");
    if !path.is_file() {
        return Err(
            "This Hayai build does not include the Scry updater. Update Hayai to continue.".into(),
        );
    }
    Ok(path)
}

/// Bounded NDJSON reader. This temporary reader exists only while the child
/// runs. A bounded channel prevents a noisy child from growing Hayai's memory.
fn run(
    args: &[&str],
    updating: bool,
    progress: impl FnMut(Progress),
) -> Result<(Value, i32), String> {
    let path = cli().inspect_err(|_| {
        if updating {
            UPDATING.store(false, Ordering::Release);
        }
    })?;
    let mut command = Command::new(path);
    DIAGNOSTICS.append(&format!("\nCommand: scry {}\n", args.join(" ")));
    command.args(args);
    let result = run_command_logged(
        command,
        updating,
        Duration::from_secs(if updating { 20 * 60 } else { 60 }),
        progress,
        true,
    );
    if let Err(error) = &result {
        DIAGNOSTICS.append(&format!("\nRunner error: {error}\n"));
    }
    result
}

#[cfg(test)]
fn run_command(
    command: Command,
    updating: bool,
    timeout: Duration,
    progress: impl FnMut(Progress),
) -> Result<(Value, i32), String> {
    run_command_logged(command, updating, timeout, progress, false)
}

fn run_command_logged(
    mut command: Command,
    updating: bool,
    timeout: Duration,
    mut progress: impl FnMut(Progress),
    record: bool,
) -> Result<(Value, i32), String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000); // CREATE_NO_WINDOW; Scry owns UAC.
    }
    let mut child = command.spawn().map_err(|e| {
        if updating {
            UPDATING.store(false, Ordering::Release);
        }
        format!("Couldn't launch Scry updater: {e}")
    })?;
    let stdout = child.stdout.take().expect("stdout was configured as piped");
    let (send, receive) = mpsc::sync_channel(8);
    let reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut total = 0usize;
        loop {
            let mut line = Vec::new();
            let read = reader
                .by_ref()
                .take((MAX_LINE + 1) as u64)
                .read_until(b'\n', &mut line);
            if record {
                DIAGNOSTICS.append(&String::from_utf8_lossy(&line));
            }
            match read {
                Ok(0) => break,
                Ok(n) if n <= MAX_LINE && total + n <= 1024 * 1024 => {
                    total += n;
                    if send
                        .send(
                            serde_json::from_slice::<Value>(&line)
                                .map_err(|_| "Invalid Scry JSON".to_owned()),
                        )
                        .is_err()
                    {
                        // Keep draining after a caller timeout so an elevated
                        // transaction cannot block while reporting progress.
                        let _ = std::io::copy(&mut reader, &mut std::io::sink());
                        break;
                    }
                }
                _ => {
                    let _ = send.send(Err("Scry output exceeded its limit".into()));
                    let _ = std::io::copy(&mut reader, &mut std::io::sink());
                    break;
                }
            }
        }
    });
    let deadline = Instant::now() + timeout;
    let result = (|| {
        let mut terminal = None;
        loop {
            match receive.recv_timeout(Duration::from_millis(100)) {
                Ok(value) => {
                    let value = value?;
                    if value["event"] == "progress" && updating && terminal.is_none() {
                        envelope(&value, "progress")?;
                        if !matches!(value["state"].as_str(), Some("elevating" | "updating")) {
                            return Err("Unknown Scry progress state".into());
                        }
                        progress(Progress {
                            message: message(&value)?,
                            active: value["state"] == "updating",
                        });
                    } else if terminal.replace(value).is_some() {
                        return Err("Scry emitted multiple results".into());
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => break,
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if Instant::now() >= deadline {
                return Err("Scry updater timed out. Check its status before retrying.".into());
            }
        }
        // EOF is not completion: verify the child also exited within the bound.
        let exit = loop {
            if let Some(exit) = child.try_wait().map_err(|e| e.to_string())? {
                break exit;
            }
            if Instant::now() >= deadline {
                return Err("Scry updater did not finish".into());
            }
            std::thread::sleep(Duration::from_millis(25));
        };
        if record {
            DIAGNOSTICS.append(&format!("\nProcess exit code: {:?}\n", exit.code()));
        }
        Ok((
            terminal
                .ok_or("Scry did not return a result. This build may include an older Scry CLI.")?,
            exit.code().ok_or("Scry terminated unexpectedly")?,
        ))
    })();
    if result.is_err() {
        if updating {
            // Never terminate an installation transaction. Retain the busy
            // guard until it exits, and drain its pipe in the temporary reader.
            drop(receive);
            std::thread::spawn(move || {
                let _ = child.wait();
                let _ = reader.join();
                crate::native::invalidate_scry_status();
                crate::native::reset_scry_search();
                UPDATING.store(false, Ordering::Release);
            });
        } else {
            let _ = child.kill();
            let _ = child.wait();
            drop(receive);
            if reader.is_finished() {
                let _ = reader.join();
            }
        }
    } else {
        let _ = reader.join();
    }
    result
}

fn cache_path() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA")
        .map(|root| PathBuf::from(root).join("hayai").join("scry-updates.json"))
}

fn cached_discovery(
    value: &Value,
    current: &str,
    channel: &str,
    now: u64,
    force: bool,
) -> Option<Discovery> {
    if value["current_version"] != current || value["channel"] != channel {
        return None;
    }
    let age = now.checked_sub(value["checked_at"].as_u64()?)?;
    let raw = &value["result"];
    let exit = i32::try_from(raw["exit_code"].as_i64()?).ok()?;
    let result = parse_discovery(raw, exit).ok()?;
    let lifetime = if result.state == State::Failed {
        let detail = raw["message"].as_str()?;
        if detail.contains("403") || detail.contains("429") {
            RATE_LIMIT_CACHE_SECONDS
        } else {
            FAILURE_CACHE_SECONDS
        }
    } else if force {
        MANUAL_CHECK_SECONDS
    } else {
        CACHE_SECONDS
    };
    (age < lifetime).then_some(result)
}

pub fn check(force: bool) -> Discovery {
    let _guard = CHECK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    DIAGNOSTICS.reset(concat!(
        "Hayai ",
        env!("CARGO_PKG_VERSION"),
        " / Scry update check\n"
    ));
    let _ = crate::native::scry_compatibility();
    // Unknown legacy versions must not default to the bundled CLI's version.
    let current = crate::native::scry_version().unwrap_or_else(|| "0.0.0".into());
    let channel = if env!("CARGO_PKG_VERSION").contains('-') {
        "prerelease"
    } else {
        "stable"
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let cache = cache_path();
    if let Some(path) = &cache
        && let Ok(file) = std::fs::File::open(path)
        && let Ok(value) = serde_json::from_reader::<_, Value>(file.take((MAX_LINE + 1) as u64))
        && let Some(result) = cached_discovery(&value, &current, channel, now, force)
    {
        DIAGNOSTICS.append(&format!("Source: cached discovery\n{}\n", value["result"]));
        return result;
    }
    let result = run(
        &[
            "check-update",
            "--json",
            "--compatible-ipc",
            "1",
            "--compatible-index",
            "10",
            "--channel",
            channel,
            "--current-version",
            &current,
        ],
        false,
        |_| {},
    );
    match result {
        Ok((value, exit)) => match parse_discovery(&value, exit) {
            Ok(result) => {
                if let Some(path) = cache {
                    let _ = std::fs::create_dir_all(path.parent().unwrap());
                    let data = json!({"current_version":current,"channel":channel,"checked_at":now,"result":value});
                    let _ = std::fs::write(path, data.to_string());
                }
                result
            }
            Err(error) => {
                DIAGNOSTICS.append(&format!("Validation error: {error}\n"));
                Discovery::failed(error)
            }
        },
        Err(error) => {
            DIAGNOSTICS.append(&format!("Check failed: {error}\n"));
            Discovery::failed(error)
        }
    }
}

pub fn install(tag: &str, progress: impl FnMut(Progress)) -> Result<String, String> {
    if !valid_tag(tag) {
        return Err("Invalid Scry target version".into());
    }
    if UPDATING.swap(true, Ordering::AcqRel) {
        return Err("A Scry update is already running".into());
    }
    DIAGNOSTICS.reset(concat!(
        "Hayai ",
        env!("CARGO_PKG_VERSION"),
        " / Scry update installation\n"
    ));
    if !crate::native::suspend_scry_search() {
        UPDATING.store(false, Ordering::Release);
        DIAGNOSTICS.append("Search worker did not acknowledge suspension within 5 seconds. Updater was not launched.\n");
        return Err("Couldn't close the file search connection. Try the update again.".into());
    }
    let result = run(&["update", "--json", "--version", tag], true, progress);
    // On transport errors run() retains the guard until the child exits.
    let (value, exit) = result?;
    struct BusyGuard;
    impl Drop for BusyGuard {
        fn drop(&mut self) {
            UPDATING.store(false, Ordering::Release);
        }
    }
    let _busy = BusyGuard;
    crate::native::invalidate_scry_status();
    crate::native::reset_scry_search();
    if let Some(path) = cache_path() {
        let _ = std::fs::remove_file(path);
    }
    envelope(&value, "result")?;
    if value["exit_code"].as_i64() != Some(exit as i64) {
        return Err("Scry update exit code mismatch".into());
    }
    let message = message(&value)?;
    match (value["state"].as_str(), exit) {
        (Some("updated" | "recovered"), 0) => {}
        (Some("failed" | "recovery_required" | "cancelled"), 1 | 20..=26) => {
            return Err(install_error_message(exit, &message).into());
        }
        _ => return Err("Invalid Scry update result".into()),
    }
    let (status, exit) = run(&["status", "--json", "--version", tag], false, |_| {})?;
    envelope(&status, "status")?;
    if exit != 0
        || status["exit_code"] != 0
        || status["state"] != "ready"
        || status["detail"]["daemon_version"] != tag[1..]
        || status["detail"]["ipc_version"] != 1
        || status["detail"]["index_format"] != 10
    {
        return Err(
            "Scry did not confirm the requested release. Check for updates again before retrying."
                .into(),
        );
    }
    Ok(message)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_cache_respects_freshness_manual_spacing_and_context() {
        let entry = json!({"current_version":"0.1.0", "channel":"stable", "checked_at":1000,
            "result":available()});
        assert!(cached_discovery(&entry, "0.1.0", "stable", 1299, true).is_some());
        assert!(cached_discovery(&entry, "0.1.0", "stable", 1300, true).is_none());
        assert!(cached_discovery(&entry, "0.1.0", "stable", 87399, false).is_some());
        assert!(cached_discovery(&entry, "0.1.0", "stable", 87400, false).is_none());
        assert!(cached_discovery(&entry, "0.2.0", "stable", 1000, false).is_none());
        assert!(cached_discovery(&entry, "0.1.0", "prerelease", 1000, false).is_none());
        assert!(cached_discovery(&entry, "0.1.0", "stable", 999, false).is_none());
    }

    #[test]
    fn failed_discovery_backoff_survives_manual_refresh_without_an_available_target() {
        let mut entry = json!({"current_version":"0.1.0", "channel":"stable", "checked_at":1000,
            "result":{"schema":1,"event":"check_update","state":"failed","target_version":null,
                "message":"HTTP 403 Forbidden", "exit_code":21}});
        let result = cached_discovery(&entry, "0.1.0", "stable", 4599, true).unwrap();
        assert_eq!(result.state, State::Failed);
        assert!(result.target.is_none());
        assert!(cached_discovery(&entry, "0.1.0", "stable", 4600, true).is_none());
        entry["result"]["message"] = json!("Connection failed");
        assert!(cached_discovery(&entry, "0.1.0", "stable", 1899, true).is_some());
        assert!(cached_discovery(&entry, "0.1.0", "stable", 1900, true).is_none());
        entry["result"]["target_version"] = json!("v0.2.0");
        assert!(cached_discovery(&entry, "0.1.0", "stable", 1000, false).is_none());
    }
    #[test]
    fn installation_errors_explain_the_failure_without_exposing_exceptions() {
        let message = install_error_message(21, "Exception calling GetResponse: (403) Forbidden");
        assert!(message.contains("GitHub refused"));
        assert!(!message.contains("GetResponse"));
        assert!(install_error_message(20, "").contains("cancelled"));
        assert!(
            install_error_message(
                23,
                "This legacy daemon must be stopped before updating; graceful stop is unsupported"
            )
            .contains("Stop the existing daemon")
        );
        assert_eq!(
            install_error_message(
                23,
                "Could not reconcile the query pipe owner within the retry budget"
            ),
            "File search ownership couldn't be verified."
        );
        assert!(!install_error_message(23, "Another daemon owns the query pipe").contains("Stop"));
        for exit in [1, 20, 21, 22, 23, 24, 25, 26] {
            assert!(install_error_message(exit, "").chars().count() <= 55);
        }
    }
    #[test]
    fn required_install_target_matches_the_locked_sdk_tag() {
        let client = include_str!("../Cargo.lock")
            .split("[[package]]")
            .find(|package| package.contains("name = \"scry-client\""))
            .expect("locked Scry client");
        assert!(client.contains(&format!("?tag={REQUIRED_VERSION}#")));
        assert!(valid_tag(REQUIRED_VERSION));
    }
    #[test]
    fn check_failures_never_display_raw_powershell_exceptions() {
        let result = json!({"schema":1,"event":"check_update","state":"failed","target_version":null,
            "message":"Exception calling GetResponse: (403) Forbidden", "exit_code":21});
        let parsed = parse_discovery(&result, 21).unwrap();
        assert_eq!(parsed.state, State::Failed);
        assert_eq!(
            parsed.message,
            "Couldn't check for Scry Search updates right now. Try again later."
        );
        assert!(!parsed.message.contains("GetResponse"));
        assert_eq!(
            Discovery::failed("This Hayai build does not include the Scry updater").message,
            "Scry Search updates require a newer Hayai build."
        );
    }
    fn available() -> Value {
        json!({"schema":1,"event":"check_update","state":"available","target_version":"v0.2.0","message":"Update available","exit_code":0,"detail":{"schema":1,"version":"0.2.0","ipc_version":1,"index_format":10}})
    }
    #[cfg(windows)]
    fn fixture(script: &str, timeout: Duration) -> Result<(Value, i32), String> {
        let shell = PathBuf::from(std::env::var_os("SystemRoot").unwrap())
            .join("System32/WindowsPowerShell/v1.0/powershell.exe");
        let mut command = Command::new(shell);
        command.args([
            "-NoLogo",
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            script,
        ]);
        run_command(command, false, timeout, |_| {})
    }
    #[cfg(windows)]
    #[test]
    fn child_protocol_requires_one_bounded_result_and_a_real_exit() {
        let output = available().to_string();
        let (value, exit) = fixture(
            &format!("[Console]::WriteLine('{output}'); exit 0"),
            Duration::from_secs(10),
        )
        .unwrap();
        assert_eq!(
            parse_discovery(&value, exit).unwrap().state,
            State::Available
        );
        assert!(
            fixture(
                &format!("[Console]::WriteLine('{output}'); [Console]::WriteLine('{output}')"),
                Duration::from_secs(10)
            )
            .is_err()
        );
        assert!(fixture("exit 0", Duration::from_secs(10)).is_err());
        assert!(
            fixture(
                "[Console]::WriteLine(('x' * 17000))",
                Duration::from_secs(10)
            )
            .is_err()
        );
        assert!(fixture("Start-Sleep -Seconds 5", Duration::from_millis(150)).is_err());
    }
    #[test]
    fn discovery_rejects_incompatible_or_unconfirmed_targets() {
        assert_eq!(
            parse_discovery(&available(), 0).unwrap().state,
            State::Available
        );
        for (field, bad) in [
            ("ipc_version", json!(2)),
            ("index_format", json!(11)),
            ("schema", json!(2)),
            ("version", json!("0.3.0")),
        ] {
            let mut value = available();
            value["detail"][field] = bad;
            assert!(parse_discovery(&value, 0).is_err());
        }
        assert!(parse_discovery(&available(), 21).is_err());
        let mut value = available();
        value["target_version"] = json!("v../../x");
        assert!(parse_discovery(&value, 0).is_err());
    }
    #[test]
    fn additive_fields_are_allowed_but_unknown_schema_is_not() {
        let mut value = available();
        value["extra"] = json!({"anything":true});
        assert!(parse_discovery(&value, 0).is_ok());
        value["schema"] = json!(2);
        assert!(parse_discovery(&value, 0).is_err());
    }
    #[test]
    fn current_can_still_include_a_candidate_tag() {
        let mut value = available();
        value["state"] = json!("current");
        assert_eq!(parse_discovery(&value, 0).unwrap().state, State::Current);
        value["state"] = json!("no_compatible_release");
        value["exit_code"] = json!(12);
        value["target_version"] = Value::Null;
        assert_eq!(
            parse_discovery(&value, 12).unwrap().state,
            State::NoCompatibleRelease
        );
    }
}
