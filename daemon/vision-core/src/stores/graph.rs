//! **Interim stand-in for the Kùzu graph DB** decided in
//! `docs/ARCHITECTURE.md` §9.1. Real Kùzu integration (Document/Folder node
//! types, typed edges, entity/relationship extraction) is M4/M9 work and is
//! tracked, not silently skipped — see `docs/TASKS.md`'s Parking Lot.
//!
//! What's here today is deliberately minimal: one `documents` table, no
//! Folder nodes, no edges, no `GetGraph` RPC (nothing consumes it yet since
//! today's client is the REPL, not the graph visualizer). This is just
//! enough to give ingested content a stable `document_id` that `SourceRef`
//! can cite and that the vector store can join back to a path + timestamp.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use crate::error::CoreResult;
use crate::ids;

pub struct GraphStore {
    conn: Mutex<Connection>,
}

pub struct NewDocument {
    pub path: String,
    pub blob_hash: Option<String>,
    pub source: i32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DocumentRecord {
    pub id: String,
    pub path: String,
    pub blob_hash: Option<String>,
    pub source: i32,
    pub created_at_unix_ms: i64,
}

impl GraphStore {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                blob_hash TEXT,
                source INTEGER NOT NULL,
                created_at_unix_ms INTEGER NOT NULL
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
            "CREATE TABLE IF NOT EXISTS documents (
                id TEXT PRIMARY KEY,
                path TEXT NOT NULL UNIQUE,
                blob_hash TEXT,
                source INTEGER NOT NULL,
                created_at_unix_ms INTEGER NOT NULL
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Upsert keyed by path's deterministic id (`ids::document_id_for_path`)
    /// — re-ingesting the same file updates the existing node rather than
    /// creating a duplicate. Returns the document id.
    pub fn upsert_document(&self, doc: NewDocument) -> CoreResult<String> {
        let id = ids::document_id_for_path(&doc.path);
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO documents (id, path, blob_hash, source, created_at_unix_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                blob_hash = excluded.blob_hash,
                source = excluded.source,
                created_at_unix_ms = excluded.created_at_unix_ms",
            params![id, doc.path, doc.blob_hash, doc.source, ids::now_unix_ms()],
        )?;
        Ok(id)
    }

    /// Every document, for the Graph Explorer (`GetGraph` RPC) — a flat
    /// scan since there's no scoping concept yet on this stand-in.
    pub fn list_all(&self) -> CoreResult<Vec<DocumentRecord>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, path, blob_hash, source, created_at_unix_ms FROM documents ORDER BY created_at_unix_ms",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok(DocumentRecord {
                id: row.get(0)?,
                path: row.get(1)?,
                blob_hash: row.get(2)?,
                source: row.get(3)?,
                created_at_unix_ms: row.get(4)?,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    pub fn get(&self, id: &str) -> CoreResult<Option<DocumentRecord>> {
        let conn = self.conn.lock().unwrap();
        let record = conn
            .query_row(
                "SELECT id, path, blob_hash, source, created_at_unix_ms FROM documents WHERE id = ?1",
                params![id],
                |row| {
                    Ok(DocumentRecord {
                        id: row.get(0)?,
                        path: row.get(1)?,
                        blob_hash: row.get(2)?,
                        source: row.get(3)?,
                        created_at_unix_ms: row.get(4)?,
                    })
                },
            )
            .optional()?;
        Ok(record)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(path: &str) -> NewDocument {
        NewDocument {
            path: path.to_string(),
            blob_hash: Some("deadbeef".to_string()),
            source: 1,
        }
    }

    #[test]
    fn upsert_then_get_round_trips_a_document() {
        let store = GraphStore::open_in_memory().unwrap();
        let id = store.upsert_document(doc("C:\\notes\\a.md")).unwrap();

        let record = store.get(&id).unwrap().unwrap();
        assert_eq!(record.path, "C:\\notes\\a.md");
        assert_eq!(record.blob_hash.as_deref(), Some("deadbeef"));
    }

    #[test]
    fn re_ingesting_the_same_path_upserts_rather_than_duplicating() {
        let store = GraphStore::open_in_memory().unwrap();
        let first_id = store.upsert_document(doc("C:\\notes\\a.md")).unwrap();
        let second_id = store
            .upsert_document(NewDocument {
                path: "C:\\notes\\a.md".to_string(),
                blob_hash: Some("newhash".to_string()),
                source: 1,
            })
            .unwrap();

        assert_eq!(first_id, second_id);
        let record = store.get(&first_id).unwrap().unwrap();
        assert_eq!(record.blob_hash.as_deref(), Some("newhash"));
    }

    #[test]
    fn get_of_unknown_id_returns_none() {
        let store = GraphStore::open_in_memory().unwrap();
        assert!(store.get("no-such-id").unwrap().is_none());
    }
}
