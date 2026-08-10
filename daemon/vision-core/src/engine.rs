//! Bundles every store + the blob root into the one object the RPC
//! handlers (`service.rs`) and the filesystem watcher (`vision-daemon`)
//! both operate on. Constructed once per daemon process
//! (`docs/ARCHITECTURE.md` §1 "single writer").

use std::fs;
use std::path::Path;

use crate::blob::BlobStore;
use crate::error::CoreResult;
use crate::stores::audit::AuditStore;
use crate::stores::config::ConfigStore;
use crate::stores::graph::GraphStore;
use crate::stores::vectors::VectorStore;

pub struct Engine {
    pub config: ConfigStore,
    pub audit: AuditStore,
    pub graph: GraphStore,
    pub vectors: VectorStore,
    pub blobs: BlobStore,
}

impl Engine {
    /// Opens (creating if needed) every store under `base_dir`, matching
    /// the layout in `docs/ARCHITECTURE.md` §5.1.
    pub fn open(base_dir: &Path) -> CoreResult<Self> {
        fs::create_dir_all(base_dir)?;
        let graph_dir = base_dir.join("graph");
        let vectors_dir = base_dir.join("vectors");
        fs::create_dir_all(&graph_dir)?;
        fs::create_dir_all(&vectors_dir)?;

        Ok(Self {
            config: ConfigStore::open(&base_dir.join("config.sqlite"))?,
            audit: AuditStore::open(&base_dir.join("audit.sqlite"))?,
            graph: GraphStore::open(&graph_dir.join("graph.sqlite"))?,
            vectors: VectorStore::open(&vectors_dir.join("vectors.sqlite"))?,
            blobs: BlobStore::open(base_dir.join("blobs"))?,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_creates_every_store_file_under_base_dir() {
        let dir = tempfile::tempdir().unwrap();
        let _engine = Engine::open(dir.path()).unwrap();

        assert!(dir.path().join("config.sqlite").exists());
        assert!(dir.path().join("audit.sqlite").exists());
        assert!(dir.path().join("graph").join("graph.sqlite").exists());
        assert!(dir.path().join("vectors").join("vectors.sqlite").exists());
        assert!(dir.path().join("blobs").is_dir());
    }

    #[test]
    fn reopening_the_same_base_dir_preserves_prior_data() {
        let dir = tempfile::tempdir().unwrap();
        {
            let engine = Engine::open(dir.path()).unwrap();
            engine
                .config
                .set(&vision_proto::PermissionScope {
                    path: "C:\\notes".to_string(),
                    scope_type: vision_proto::PermissionScopeType::Folder as i32,
                    granted: true,
                })
                .unwrap();
        }

        let engine = Engine::open(dir.path()).unwrap();
        assert_eq!(engine.config.list().unwrap().len(), 1);
    }
}
