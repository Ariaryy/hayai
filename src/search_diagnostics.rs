//! Bounded, privacy-conscious diagnostic events for application catalog
//! initialization and search execution.
//!
//! **Privacy contract:** no user queries, application names, executable paths,
//! usernames, or any personally identifying information are stored. Only counts
//! and durations appear in events.
//!
//! **Reliability contract:** every public entry point absorbs panics from lock
//! poisoning and storage failures so that diagnostics never interfere with
//! normal search operation.
//!
//! **Memory contract:** [`SearchDiagnostics`] retains at most
//! [`MAX_EVENTS`] events; the oldest is discarded on overflow.

use std::collections::VecDeque;
use std::fmt::Write;
use std::sync::Mutex;
use std::time::Instant;

/// Maximum number of events retained in memory at any time.
pub const MAX_EVENTS: usize = 128;

// ---------------------------------------------------------------------------
// Event model
// ---------------------------------------------------------------------------

/// A single diagnostic observation — no user data inside.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchDiagnostic {
    /// What happened.
    pub event: SearchEventKind,
    /// Wall-clock duration of the operation, if measured.
    pub duration_us: Option<u64>,
    /// Number of catalog entries considered during search, if applicable.
    pub candidate_count: Option<usize>,
    /// Number of results produced, if applicable.
    pub result_count: Option<usize>,
}

/// Categories of diagnostic events emitted by catalog and search operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SearchEventKind {
    /// Catalog initialization has started (background scan kicked off).
    CatalogStarted,
    /// Catalog scan completed successfully.
    CatalogCompleted {
        /// Total entries discovered from all sources before deduplication.
        discovered: usize,
        /// Entries available for search after deduplication.
        searchable: usize,
    },
    /// A search query completed.
    SearchCompleted,
    /// A search query took longer than the slow-search threshold.
    SearchSlow,
}

// ---------------------------------------------------------------------------
// Slow-search threshold
// ---------------------------------------------------------------------------

/// Searches that exceed this duration are recorded as [`SearchEventKind::SearchSlow`]
/// in addition to the normal completion event.
pub const SLOW_SEARCH_THRESHOLD_US: u64 = 10_000; // 10 ms

// ---------------------------------------------------------------------------
// Storage
// ---------------------------------------------------------------------------

/// Bounded, thread-safe store for recent diagnostic events.
///
/// Uses a [`Mutex`]-guarded [`VecDeque`] with a fixed capacity. Locking is
/// taken only briefly at event boundaries (never inside candidate loops) so
/// contention is negligible. Lock poisoning is recovered gracefully.
pub struct SearchDiagnostics {
    inner: Mutex<VecDeque<SearchDiagnostic>>,
}

impl SearchDiagnostics {
    /// Create a new, empty store.
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(VecDeque::new()),
        }
    }

    /// Record a diagnostic event. Silently discards the event if the mutex is
    /// poisoned or if any unexpected failure occurs — diagnostics must never
    /// crash the launcher.
    pub fn record(&self, event: SearchDiagnostic) {
        let Ok(mut queue) = self.inner.lock().map_err(|e| e.into_inner()) else {
            return;
        };
        if queue.len() >= MAX_EVENTS {
            queue.pop_front();
        }
        queue.push_back(event);
    }

    /// Return a point-in-time snapshot of all retained events, oldest first.
    /// Does not mutate the stored events.
    pub fn snapshot(&self) -> Vec<SearchDiagnostic> {
        self.inner
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .cloned()
            .collect()
    }

    /// Format recent events into a plain-text log suitable for clipboard copying.
    pub fn format_snapshot(&self) -> String {
        let events = self.snapshot();
        if events.is_empty() {
            return "No search diagnostics recorded.\n".to_string();
        }
        let mut out = String::new();
        for event in events {
            match event.event {
                SearchEventKind::CatalogStarted => {
                    let _ = writeln!(out, "Catalog scan started");
                }
                SearchEventKind::CatalogCompleted {
                    discovered,
                    searchable,
                } => {
                    let duration_ms = event.duration_us.map_or(0.0, |us| us as f64 / 1_000.0);
                    let _ = writeln!(
                        out,
                        "Catalog scan completed: {discovered} discovered, {searchable} searchable ({duration_ms:.2} ms)"
                    );
                }
                SearchEventKind::SearchCompleted => {
                    let duration_ms = event.duration_us.map_or(0.0, |us| us as f64 / 1_000.0);
                    let candidates = event.candidate_count.unwrap_or(0);
                    let results = event.result_count.unwrap_or(0);
                    let _ = writeln!(
                        out,
                        "Search completed: {candidates} candidates, {results} results ({duration_ms:.2} ms)"
                    );
                }
                SearchEventKind::SearchSlow => {
                    let duration_ms = event.duration_us.map_or(0.0, |us| us as f64 / 1_000.0);
                    let candidates = event.candidate_count.unwrap_or(0);
                    let results = event.result_count.unwrap_or(0);
                    let _ = writeln!(
                        out,
                        "Search slow: {candidates} candidates, {results} results ({duration_ms:.2} ms)"
                    );
                }
            }
        }
        out
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Record a catalog-started event on `store`.
pub fn record_catalog_started(store: &SearchDiagnostics) {
    store.record(SearchDiagnostic {
        event: SearchEventKind::CatalogStarted,
        duration_us: None,
        candidate_count: None,
        result_count: None,
    });
}

/// Record a successful catalog scan on `store`.
pub fn record_catalog_completed(
    store: &SearchDiagnostics,
    start: Instant,
    discovered: usize,
    searchable: usize,
) {
    let duration_us = start.elapsed().as_micros().try_into().unwrap_or(u64::MAX);
    store.record(SearchDiagnostic {
        event: SearchEventKind::CatalogCompleted {
            discovered,
            searchable,
        },
        duration_us: Some(duration_us),
        candidate_count: None,
        result_count: None,
    });
}

/// Record a completed search on `store`. Also records a [`SearchEventKind::SearchSlow`]
/// event when the duration exceeds [`SLOW_SEARCH_THRESHOLD_US`].
pub fn record_search_completed(
    store: &SearchDiagnostics,
    start: Instant,
    candidate_count: usize,
    result_count: usize,
) {
    let duration_us = start.elapsed().as_micros().try_into().unwrap_or(u64::MAX);
    store.record(SearchDiagnostic {
        event: SearchEventKind::SearchCompleted,
        duration_us: Some(duration_us),
        candidate_count: Some(candidate_count),
        result_count: Some(result_count),
    });
    if duration_us >= SLOW_SEARCH_THRESHOLD_US {
        store.record(SearchDiagnostic {
            event: SearchEventKind::SearchSlow,
            duration_us: Some(duration_us),
            candidate_count: Some(candidate_count),
            result_count: Some(result_count),
        });
    }
}

// ---------------------------------------------------------------------------
// Process-wide singleton
// ---------------------------------------------------------------------------

/// Process-wide diagnostic store for application search and catalog events.
///
/// Accessible from both the GPUI UI thread (via [`AppsProvider::search`]) and
/// the background pool (via [`scan_apps`]) without additional synchronization:
/// the inner [`Mutex`] serializes all access.
pub static DIAGNOSTICS: SearchDiagnostics = SearchDiagnostics::new();

/// Return formatted search diagnostic logs.
pub fn diagnostic_snapshot() -> String {
    DIAGNOSTICS.format_snapshot()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn make_store() -> SearchDiagnostics {
        SearchDiagnostics::new()
    }

    // --- FR-01, FR-02: catalog events ----------------------------------------

    #[test]
    fn catalog_started_records_event() {
        let store = make_store();
        record_catalog_started(&store);
        let snap = store.snapshot();
        assert_eq!(snap.len(), 1);
        assert_eq!(snap[0].event, SearchEventKind::CatalogStarted);
        assert!(snap[0].duration_us.is_none());
    }

    #[test]
    fn catalog_completed_records_counts_and_duration() {
        let store = make_store();
        let t = Instant::now();
        record_catalog_completed(&store, t, 100, 95);
        let snap = store.snapshot();
        assert_eq!(snap.len(), 1);
        assert!(matches!(
            &snap[0].event,
            SearchEventKind::CatalogCompleted {
                discovered: 100,
                searchable: 95
            }
        ));
        assert!(snap[0].duration_us.is_some());
    }

    // --- FR-04, FR-05: search events -----------------------------------------

    #[test]
    fn search_completed_records_timing_and_counts() {
        let store = make_store();
        let t = Instant::now();
        record_search_completed(&store, t, 742, 8);
        let snap = store.snapshot();
        let completed = snap
            .iter()
            .filter(|e| e.event == SearchEventKind::SearchCompleted)
            .collect::<Vec<_>>();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].candidate_count, Some(742));
        assert_eq!(completed[0].result_count, Some(8));
        assert!(completed[0].duration_us.is_some());
    }

    #[test]
    fn slow_search_emitted_through_record_search_completed() {
        let store = make_store();
        // Passing an Instant from far enough in the past (e.g. 20ms ago) ensures
        // record_search_completed itself emits both SearchCompleted and SearchSlow.
        let past = Instant::now()
            .checked_sub(std::time::Duration::from_micros(
                SLOW_SEARCH_THRESHOLD_US + 5_000,
            ))
            .unwrap();
        record_search_completed(&store, past, 150, 4);
        let snap = store.snapshot();
        assert_eq!(snap.len(), 2);
        assert_eq!(snap[0].event, SearchEventKind::SearchCompleted);
        assert_eq!(snap[1].event, SearchEventKind::SearchSlow);
        assert_eq!(snap[1].candidate_count, Some(150));
        assert_eq!(snap[1].result_count, Some(4));
        assert!(snap[1].duration_us.unwrap() >= SLOW_SEARCH_THRESHOLD_US);
    }

    #[test]
    fn fast_search_does_not_emit_slow_event() {
        let store = make_store();
        // Recording with Instant::now() will measure < 10ms and not emit SearchSlow
        record_search_completed(&store, Instant::now(), 5, 1);
        let snap = store.snapshot();
        assert!(
            snap.iter()
                .any(|e| e.event == SearchEventKind::SearchCompleted)
        );
        assert!(!snap.iter().any(|e| e.event == SearchEventKind::SearchSlow));
    }

    // --- Format snapshot -----------------------------------------------------

    #[test]
    fn format_snapshot_includes_all_event_types() {
        let store = make_store();
        record_catalog_started(&store);
        record_catalog_completed(&store, Instant::now(), 50, 45);
        record_search_completed(&store, Instant::now(), 45, 3);
        let past = Instant::now()
            .checked_sub(std::time::Duration::from_micros(
                SLOW_SEARCH_THRESHOLD_US + 1_000,
            ))
            .unwrap();
        record_search_completed(&store, past, 45, 1);

        let text = store.format_snapshot();
        assert!(text.contains("Catalog scan started"));
        assert!(text.contains("Catalog scan completed: 50 discovered, 45 searchable"));
        assert!(text.contains("Search completed: 45 candidates, 3 results"));
        assert!(text.contains("Search slow: 45 candidates, 1 results"));
    }

    #[test]
    fn format_snapshot_handles_empty() {
        let store = make_store();
        assert_eq!(store.format_snapshot(), "No search diagnostics recorded.\n");
    }

    // --- FR-08: bounded storage ----------------------------------------------

    #[test]
    fn collection_never_exceeds_capacity() {
        let store = make_store();
        for _ in 0..MAX_EVENTS + 50 {
            record_catalog_started(&store);
        }
        assert!(store.snapshot().len() <= MAX_EVENTS);
    }

    #[test]
    fn oldest_event_is_discarded_on_overflow() {
        let store = make_store();
        for _ in 0..MAX_EVENTS {
            record_catalog_started(&store);
        }
        record_catalog_completed(&store, Instant::now(), 1, 1);
        let snap = store.snapshot();
        assert_eq!(snap.len(), MAX_EVENTS);
        assert!(matches!(
            snap.last().unwrap().event,
            SearchEventKind::CatalogCompleted { .. }
        ));
        let completed_count = snap
            .iter()
            .filter(|e| matches!(e.event, SearchEventKind::CatalogCompleted { .. }))
            .count();
        assert_eq!(completed_count, 1);
    }

    // --- FR-09: snapshot is non-mutating -------------------------------------

    #[test]
    fn snapshot_does_not_mutate_stored_events() {
        let store = make_store();
        record_catalog_started(&store);
        let snap1 = store.snapshot();
        let snap2 = store.snapshot();
        assert_eq!(snap1.len(), snap2.len());
        assert_eq!(snap1[0].event, snap2[0].event);
    }

    // --- NFR-03: privacy guarantee -------------------------------------------

    #[test]
    fn events_contain_no_query_text() {
        let store = make_store();
        record_catalog_started(&store);
        record_catalog_completed(&store, Instant::now(), 10, 10);
        record_search_completed(&store, Instant::now(), 10, 2);
        for event in store.snapshot() {
            let debug = format!("{event:?}");
            assert!(!debug.contains("query"), "query text found in: {debug}");
            assert!(!debug.contains("path="), "path text found in: {debug}");
        }
    }

    // --- NFR-04: diagnostic failures are non-fatal ---------------------------

    #[test]
    fn poisoned_mutex_does_not_panic() {
        use std::sync::Arc;
        let store = Arc::new(make_store());
        let store_clone = Arc::clone(&store);
        let _ = std::panic::catch_unwind(move || {
            let _guard = store_clone.inner.lock().unwrap();
            panic!("intentional poison");
        });
        record_catalog_started(&store);
        let _ = store.snapshot();
        let _ = store.format_snapshot();
    }

    // --- NFR-05: multiple writers --------------------------------------------

    #[test]
    fn concurrent_recording_does_not_corrupt_state() {
        use std::sync::Arc;
        use std::thread;

        let store = Arc::new(make_store());
        let threads: Vec<_> = (0..8)
            .map(|_| {
                let s = Arc::clone(&store);
                thread::spawn(move || {
                    for _ in 0..32 {
                        record_catalog_started(&s);
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().expect("thread panicked");
        }
        assert!(store.snapshot().len() <= MAX_EVENTS);
    }
}
