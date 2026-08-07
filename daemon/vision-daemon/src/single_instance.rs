//! Single-instance enforcement per `docs/ARCHITECTURE.md` §7's "single
//! writer" principle: only one daemon may hold the graph/vector stores
//! open at a time, so a second launch must refuse to start rather than
//! risk two writers corrupting shared state.
//!
//! `ARCHITECTURE.md` §7 describes the second instance as proxying to the
//! first rather than just exiting; that's not implemented yet — tracked
//! in `docs/TASKS.md` §4 (Parking Lot). For now the second instance
//! refuses to start with a clear error.

use std::fs::{File, TryLockError};
use std::io;
use std::path::{Path, PathBuf};

/// Holds the lock for as long as it's alive. The OS releases the
/// underlying file lock automatically when `_file` closes — including on
/// process crash — so a stale lock can never wedge future launches.
pub struct InstanceLock {
    _file: File,
}

#[derive(Debug)]
pub enum AcquireError {
    /// Another instance already holds the lock.
    AlreadyRunning,
    Io(io::Error),
}

impl std::fmt::Display for AcquireError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcquireError::AlreadyRunning => {
                write!(f, "another vision-daemon instance is already running")
            }
            AcquireError::Io(err) => write!(f, "failed to acquire single-instance lock: {err}"),
        }
    }
}

impl std::error::Error for AcquireError {}

pub fn lock_file_path(data_dir: &Path) -> PathBuf {
    data_dir.join("daemon.lock")
}

/// Acquires the single-instance lock under `data_dir`, creating `data_dir`
/// if needed. Non-blocking: returns immediately with
/// [`AcquireError::AlreadyRunning`] if another instance holds it, rather
/// than waiting.
pub fn acquire(data_dir: &Path) -> Result<InstanceLock, AcquireError> {
    std::fs::create_dir_all(data_dir).map_err(AcquireError::Io)?;
    let file = File::create(lock_file_path(data_dir)).map_err(AcquireError::Io)?;

    match file.try_lock() {
        Ok(()) => Ok(InstanceLock { _file: file }),
        Err(TryLockError::WouldBlock) => Err(AcquireError::AlreadyRunning),
        Err(TryLockError::Error(err)) => Err(AcquireError::Io(err)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "vision-single-instance-test-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn second_acquire_on_the_same_dir_is_refused_while_the_first_is_held() {
        let dir = scratch_dir("refused-while-held");

        let first = acquire(&dir).expect("first acquire should succeed");
        let second = acquire(&dir);

        assert!(matches!(second, Err(AcquireError::AlreadyRunning)));

        drop(first);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn lock_is_released_and_reacquirable_after_the_holder_drops() {
        let dir = scratch_dir("reacquirable-after-drop");

        let first = acquire(&dir).expect("first acquire should succeed");
        drop(first);

        let second = acquire(&dir);
        assert!(second.is_ok(), "lock should be free once the holder dropped");

        let _ = std::fs::remove_dir_all(&dir);
    }
}
