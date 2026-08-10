//! Filesystem Watcher (`docs/ROADMAP.md` M3): watches only permitted
//! folders (`docs/ARCHITECTURE.md` §6.1 Flow A) and feeds create/modify
//! events into the same `vision_core::ingest::run` pipeline the
//! `IngestEvent` RPC uses — one path into storage, matching §1's "single
//! writer" principle.
//!
//! Permissions can change at runtime (grant/revoke via the RPCs), so the
//! set of watched folders is reconciled against `ConfigStore` on a fixed
//! interval rather than watched once at startup.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use tokio::sync::mpsc;
use tokio::time::Instant;

use vision_core::Engine;
use vision_proto::IngestSource;

const RECONCILE_INTERVAL: Duration = Duration::from_secs(2);

/// A single logical save (e.g. `std::fs::write`, or a text editor's
/// create-then-write-then-close sequence) routinely fires several raw OS
/// events on Windows in quick succession — observed directly in this
/// crate's own watcher integration test. Every event for a path resets its
/// deadline (a *trailing* debounce); only once a path has gone quiet for
/// this long do we actually read and index it. This matters for
/// correctness, not just for avoiding duplicate audit rows: indexing on
/// the *first* event of a burst risks reading a file that's been created
/// but not yet fully written — an earlier "leading" debounce (process the
/// first event, ignore the rest) did exactly that and produced 0-chunk
/// ingests for non-empty files.
const DEBOUNCE_WINDOW: Duration = Duration::from_millis(500);
const DEBOUNCE_CHECK_INTERVAL: Duration = Duration::from_millis(100);

/// Spawns the watcher as a detached background task. The returned handle
/// is for tests/shutdown coordination — the daemon itself just lets it run
/// for the process lifetime.
pub fn spawn(engine: Arc<Engine>) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run(engine))
}

async fn run(engine: Arc<Engine>) {
    let (tx, mut rx) = mpsc::unbounded_channel::<PathBuf>();

    let mut watcher = match notify::recommended_watcher(move |res: notify::Result<Event>| {
        let Ok(event) = res else { return };
        if !matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
            return;
        }
        for path in event.paths {
            if path.is_file() {
                // An unbounded channel send only fails if the receiver was
                // dropped, i.e. the watcher task itself is gone — nothing
                // useful to do with that from inside a notify callback.
                let _ = tx.send(path);
            }
        }
    }) {
        Ok(w) => w,
        Err(e) => {
            eprintln!("vision-daemon: failed to start filesystem watcher: {e}");
            return;
        }
    };

    let mut watched: HashSet<PathBuf> = HashSet::new();
    // Paths with a pending event, and when that event last fired. A path
    // is only actually ingested once it's gone quiet for `DEBOUNCE_WINDOW`.
    let mut pending: HashMap<PathBuf, Instant> = HashMap::new();
    let mut reconcile_tick = tokio::time::interval(RECONCILE_INTERVAL);
    let mut debounce_tick = tokio::time::interval(DEBOUNCE_CHECK_INTERVAL);

    loop {
        tokio::select! {
            _ = reconcile_tick.tick() => {
                reconcile(&engine, &mut watcher, &mut watched);
            }
            _ = debounce_tick.tick() => {
                let now = Instant::now();
                let due: Vec<PathBuf> = pending
                    .iter()
                    .filter(|(_, &last_event)| now.duration_since(last_event) >= DEBOUNCE_WINDOW)
                    .map(|(path, _)| path.clone())
                    .collect();
                for path in due {
                    pending.remove(&path);
                    ingest_one(&engine, path).await;
                }
            }
            Some(path) = rx.recv() => {
                pending.insert(path, Instant::now());
            }
        }
    }
}

async fn ingest_one(engine: &Arc<Engine>, path: PathBuf) {
    let engine = engine.clone();
    let display_path = path.clone();
    let result = tokio::task::spawn_blocking(move || {
        vision_core::ingest::run(&engine, &path, IngestSource::Filesystem as i32)
    })
    .await;

    match result {
        Ok(Ok(outcome)) => eprintln!(
            "vision-daemon: indexed {} ({} chunk(s))",
            display_path.display(),
            outcome.chunks_indexed
        ),
        Ok(Err(e)) => eprintln!(
            "vision-daemon: failed to ingest {}: {e}",
            display_path.display()
        ),
        Err(e) => eprintln!("vision-daemon: ingest task panicked: {e}"),
    }
}

/// Diffs the currently-granted folders against what the watcher is
/// currently watching and applies the delta. Cheap at prototype scale — a
/// handful of granted folders, checked every couple seconds.
fn reconcile(engine: &Engine, watcher: &mut RecommendedWatcher, watched: &mut HashSet<PathBuf>) {
    let granted: HashSet<PathBuf> = match engine.config.granted_folders() {
        Ok(paths) => paths.into_iter().map(PathBuf::from).collect(),
        Err(e) => {
            eprintln!("vision-daemon: failed to read permissions for watcher reconcile: {e}");
            return;
        }
    };

    for path in granted.difference(watched) {
        match watcher.watch(path, RecursiveMode::Recursive) {
            Ok(()) => eprintln!("vision-daemon: now watching {}", path.display()),
            Err(e) => eprintln!("vision-daemon: failed to watch {}: {e}", path.display()),
        }
    }
    for path in watched.difference(&granted) {
        if let Err(e) = watcher.unwatch(path) {
            eprintln!("vision-daemon: failed to unwatch {}: {e}", path.display());
        } else {
            eprintln!("vision-daemon: stopped watching {}", path.display());
        }
    }

    *watched = granted;
}
