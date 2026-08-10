//! `config.sqlite` — folder/app permission grants (`docs/ARCHITECTURE.md`
//! §5.2). Backs `GetPermissions`/`SetPermission`/`RevokePermission`.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

use vision_proto::{PermissionScope, PermissionScopeType};

use crate::error::CoreResult;

pub struct ConfigStore {
    conn: Mutex<Connection>,
}

impl ConfigStore {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS permissions (
                path TEXT PRIMARY KEY,
                scope_type INTEGER NOT NULL,
                granted INTEGER NOT NULL
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// In-memory store for tests that don't need a file on disk.
    #[cfg(test)]
    pub fn open_in_memory() -> CoreResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS permissions (
                path TEXT PRIMARY KEY,
                scope_type INTEGER NOT NULL,
                granted INTEGER NOT NULL
            );",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn list(&self) -> CoreResult<Vec<PermissionScope>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT path, scope_type, granted FROM permissions")?;
        let rows = stmt.query_map([], |row| {
            Ok(PermissionScope {
                path: row.get(0)?,
                scope_type: row.get(1)?,
                granted: row.get::<_, i64>(2)? != 0,
            })
        })?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    /// Only paths granted with scope FOLDER — what the watcher (M3) should
    /// actually watch.
    pub fn granted_folders(&self) -> CoreResult<Vec<String>> {
        Ok(self
            .list()?
            .into_iter()
            .filter(|p| p.granted && p.scope_type == PermissionScopeType::Folder as i32)
            .map(|p| p.path)
            .collect())
    }

    /// Upsert — grants (or updates the grant state of) a scope.
    pub fn set(&self, scope: &PermissionScope) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO permissions (path, scope_type, granted) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET scope_type = excluded.scope_type, granted = excluded.granted",
            params![scope.path, scope.scope_type, scope.granted as i64],
        )?;
        Ok(())
    }

    /// Removes the grant entirely. Returns whether a row existed.
    pub fn revoke(&self, path: &str) -> CoreResult<bool> {
        let conn = self.conn.lock().unwrap();
        let existed: Option<i64> = conn
            .query_row(
                "SELECT 1 FROM permissions WHERE path = ?1",
                params![path],
                |row| row.get(0),
            )
            .optional()?;
        conn.execute("DELETE FROM permissions WHERE path = ?1", params![path])?;
        Ok(existed.is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn folder_scope(path: &str, granted: bool) -> PermissionScope {
        PermissionScope {
            path: path.to_string(),
            scope_type: PermissionScopeType::Folder as i32,
            granted,
        }
    }

    #[test]
    fn set_then_list_round_trips_a_granted_folder() {
        let store = ConfigStore::open_in_memory().unwrap();
        store.set(&folder_scope("C:\\notes", true)).unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].path, "C:\\notes");
        assert!(listed[0].granted);
    }

    #[test]
    fn set_twice_upserts_rather_than_duplicating() {
        let store = ConfigStore::open_in_memory().unwrap();
        store.set(&folder_scope("C:\\notes", true)).unwrap();
        store.set(&folder_scope("C:\\notes", false)).unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert!(!listed[0].granted);
    }

    #[test]
    fn granted_folders_excludes_revoked_and_app_scopes() {
        let store = ConfigStore::open_in_memory().unwrap();
        store.set(&folder_scope("C:\\notes", true)).unwrap();
        store.set(&folder_scope("C:\\revoked", false)).unwrap();
        store
            .set(&PermissionScope {
                path: "some.app".to_string(),
                scope_type: PermissionScopeType::App as i32,
                granted: true,
            })
            .unwrap();

        assert_eq!(store.granted_folders().unwrap(), vec!["C:\\notes"]);
    }

    #[test]
    fn revoke_removes_the_row_and_reports_whether_one_existed() {
        let store = ConfigStore::open_in_memory().unwrap();
        store.set(&folder_scope("C:\\notes", true)).unwrap();

        assert!(store.revoke("C:\\notes").unwrap());
        assert!(store.list().unwrap().is_empty());
        assert!(!store.revoke("C:\\notes").unwrap(), "already gone");
    }

    #[test]
    fn revoke_on_a_path_that_was_never_granted_reports_false() {
        let store = ConfigStore::open_in_memory().unwrap();
        assert!(!store.revoke("C:\\never-granted").unwrap());
    }
}
