//! Recent-item writes capture a generation before leaving the UI thread.
//! Clearing invalidates queued writes; the disk lock is only acquired by workers.
use std::{
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

pub enum Kind {
    Apps,
    Files,
}
impl Kind {
    fn path(&self) -> Option<PathBuf> {
        let name = match self {
            Self::Apps => "recents.txt",
            Self::Files => "file_recents.txt",
        };
        Some(
            PathBuf::from(std::env::var_os("APPDATA")?)
                .join("hayai")
                .join(name),
        )
    }
}
struct Store {
    enabled: AtomicBool,
    generation: AtomicU64,
    disk: Mutex<()>,
}
impl Store {
    const fn new() -> Self {
        Self {
            enabled: AtomicBool::new(true),
            generation: AtomicU64::new(0),
            disk: Mutex::new(()),
        }
    }
    fn configure(&self, enabled: bool) {
        if self.enabled.load(Ordering::SeqCst) == enabled {
            return;
        }
        self.enabled.store(false, Ordering::SeqCst);
        self.generation.fetch_add(1, Ordering::SeqCst);
        self.enabled.store(enabled, Ordering::SeqCst);
    }
    fn write(&self, generation: u64, path: &Path, contents: &str) -> std::io::Result<()> {
        let _guard = self.disk.lock().unwrap_or_else(|e| e.into_inner());
        if !self.enabled.load(Ordering::SeqCst)
            || generation != self.generation.load(Ordering::SeqCst)
        {
            return Ok(());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, contents)
    }
    fn clear(&self, paths: impl IntoIterator<Item = PathBuf>) -> std::io::Result<()> {
        let _guard = self.disk.lock().unwrap_or_else(|e| e.into_inner());
        for path in paths {
            match std::fs::remove_file(path) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => return Err(e),
            }
        }
        Ok(())
    }
}
static STORE: Store = Store::new();
pub fn enabled() -> bool {
    STORE.enabled.load(Ordering::SeqCst)
}
pub fn configure(enabled: bool) {
    STORE.configure(enabled);
}

pub struct PendingWrite {
    generation: u64,
    contents: String,
}
pub fn prepare(contents: String) -> Option<PendingWrite> {
    let generation = STORE.generation.load(Ordering::SeqCst);
    enabled().then_some(PendingWrite {
        generation,
        contents,
    })
}
impl PendingWrite {
    pub fn save(self, kind: Kind) {
        if let Some(path) = kind.path() {
            let _ = STORE.write(self.generation, &path, &self.contents);
        }
    }
}
pub fn clear() -> std::io::Result<()> {
    STORE.clear(
        [Kind::Apps, Kind::Files]
            .into_iter()
            .filter_map(|kind| kind.path()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clearing_rejects_writes_queued_before_disable_and_after_reenable() {
        let store = Store::new();
        let root = std::env::temp_dir().join(format!("hayai-history-{}", std::process::id()));
        let path = root.join("recents.txt");
        let old = store.generation.load(Ordering::SeqCst);
        store.write(old, &path, "old").unwrap();
        store.configure(false);
        store.clear([path.clone()]).unwrap();
        store.write(old, &path, "late").unwrap();
        assert!(!path.exists());
        store.configure(true);
        store.write(old, &path, "late after reenable").unwrap();
        assert!(!path.exists());
        store
            .write(store.generation.load(Ordering::SeqCst), &path, "new")
            .unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "new");
        store.clear([path]).unwrap();
        std::fs::remove_dir(root).unwrap();
    }
}
