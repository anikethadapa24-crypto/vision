//! Proves the M3 filesystem watcher end to end: granting a folder permission
//! and then writing a real file into it produces a blob + audit entry
//! within a few seconds, with no RPC call involved — the watcher drives
//! the same `vision_core::ingest::run` pipeline on its own.

use std::sync::Arc;
use std::time::Duration;

use vision_core::Engine;
use vision_proto::{PermissionScope, PermissionScopeType};

#[tokio::test]
async fn saving_a_file_in_a_granted_folder_is_indexed_without_any_rpc_call() {
    let dir = tempfile::tempdir().unwrap();
    let watched_folder = dir.path().join("watched");
    std::fs::create_dir_all(&watched_folder).unwrap();

    let engine = Arc::new(Engine::open(&dir.path().join("data")).unwrap());
    engine
        .config
        .set(&PermissionScope {
            path: watched_folder.to_string_lossy().to_string(),
            scope_type: PermissionScopeType::Folder as i32,
            granted: true,
        })
        .unwrap();

    let _watcher_handle = vision_daemon::watcher::spawn(engine.clone());

    // The watcher reconciles watched folders against the permission store
    // on a fixed interval rather than instantly — give it time to pick up
    // the grant before writing the file it's supposed to catch.
    tokio::time::sleep(Duration::from_millis(2500)).await;

    let file_path = watched_folder.join("note.md");
    std::fs::write(&file_path, "caught by the filesystem watcher").unwrap();

    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut audit_entries = Vec::new();
    while tokio::time::Instant::now() < deadline {
        audit_entries = engine.audit.list().unwrap();
        if !audit_entries.is_empty() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(200)).await;
    }

    assert_eq!(audit_entries.len(), 1, "watcher never indexed the new file");
    assert!(audit_entries[0].path.ends_with("note.md"));
}
