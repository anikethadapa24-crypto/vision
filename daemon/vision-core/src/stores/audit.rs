//! `audit.sqlite` — append-only, soft-delete log of indexed items
//! (`docs/ARCHITECTURE.md` §5.2). Backs `ListAudit`/`DeleteAudit`.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use vision_proto::AuditEntry;

use crate::error::CoreResult;
use crate::ids;

pub struct AuditStore {
    conn: Mutex<Connection>,
}

pub struct NewAuditEntry {
    pub source: i32,
    pub path: String,
}

impl AuditStore {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_log (
                id TEXT PRIMARY KEY,
                source INTEGER NOT NULL,
                path TEXT NOT NULL,
                indexed_at_unix_ms INTEGER NOT NULL,
                deleted_at_unix_ms INTEGER
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> CoreResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS audit_log (
                id TEXT PRIMARY KEY,
                source INTEGER NOT NULL,
                path TEXT NOT NULL,
                indexed_at_unix_ms INTEGER NOT NULL,
                deleted_at_unix_ms INTEGER
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Append-only: every ingest gets a new row, never an update.
    pub fn append(&self, entry: NewAuditEntry) -> CoreResult<AuditEntry> {
        let id = ids::generate_id("audit");
        let indexed_at_unix_ms = ids::now_unix_ms();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO audit_log (id, source, path, indexed_at_unix_ms, deleted_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4, NULL)",
            params![id, entry.source, entry.path, indexed_at_unix_ms],
        )?;
        Ok(AuditEntry {
            id,
            source: entry.source,
            path: entry.path,
            indexed_at_unix_ms,
        })
    }

    /// Live (non-deleted) entries, newest first.
    pub fn list(&self) -> CoreResult<Vec<AuditEntry>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, source, path, indexed_at_unix_ms FROM audit_log
             WHERE deleted_at_unix_ms IS NULL
             ORDER BY indexed_at_unix_ms DESC",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(AuditEntry {
                id: row.get(0)?,
                source: row.get(1)?,
                path: row.get(2)?,
                indexed_at_unix_ms: row.get(3)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Soft-delete: sets `deleted_at_unix_ms`, never removes the row.
    /// Returns whether a live row was found and marked.
    pub fn soft_delete(&self, id: &str) -> CoreResult<bool> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE audit_log SET deleted_at_unix_ms = ?1
             WHERE id = ?2 AND deleted_at_unix_ms IS NULL",
            params![ids::now_unix_ms(), id],
        )?;
        Ok(updated > 0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use vision_proto::IngestSource;

    fn entry(path: &str) -> NewAuditEntry {
        NewAuditEntry {
            source: IngestSource::Filesystem as i32,
            path: path.to_string(),
        }
    }

    #[test]
    fn append_then_list_round_trips_a_live_entry() {
        let store = AuditStore::open_in_memory().unwrap();
        let appended = store.append(entry("C:\\notes\\a.md")).unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, appended.id);
        assert_eq!(listed[0].path, "C:\\notes\\a.md");
    }

    #[test]
    fn soft_deleted_entries_disappear_from_list_but_the_row_survives() {
        let store = AuditStore::open_in_memory().unwrap();
        let appended = store.append(entry("C:\\notes\\a.md")).unwrap();

        assert!(store.soft_delete(&appended.id).unwrap());
        assert!(store.list().unwrap().is_empty());

        // deleting again reports "nothing to delete", not an error
        assert!(!store.soft_delete(&appended.id).unwrap());
    }

    #[test]
    fn soft_delete_of_unknown_id_reports_false() {
        let store = AuditStore::open_in_memory().unwrap();
        assert!(!store.soft_delete("does-not-exist").unwrap());
    }

    #[test]
    fn list_orders_newest_first() {
        let store = AuditStore::open_in_memory().unwrap();
        store.append(entry("C:\\notes\\first.md")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(2));
        store.append(entry("C:\\notes\\second.md")).unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed[0].path, "C:\\notes\\second.md");
        assert_eq!(listed[1].path, "C:\\notes\\first.md");
    }
}
