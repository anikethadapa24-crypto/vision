//! Content-addressed blob store (`docs/ARCHITECTURE.md` §5.2): extracted
//! text keyed by its own SHA-256 hash, never the original file bytes.

use std::fs;
use std::path::PathBuf;

use sha2::{Digest, Sha256};

use crate::error::CoreResult;
use crate::ids::hex_encode;

pub struct BlobStore {
    root: PathBuf,
}

impl BlobStore {
    pub fn open(root: PathBuf) -> CoreResult<Self> {
        fs::create_dir_all(&root)?;
        Ok(Self { root })
    }

    /// Writes `content` under its SHA-256 hash and returns the hash (hex).
    /// A no-op if the blob already exists — content-addressing makes writes
    /// naturally idempotent.
    pub fn write(&self, content: &str) -> CoreResult<String> {
        let hash = hex_encode(&Sha256::digest(content.as_bytes()));
        let path = self.path_for(&hash);
        if !path.exists() {
            fs::write(&path, content.as_bytes())?;
        }
        Ok(hash)
    }

    pub fn read(&self, hash: &str) -> CoreResult<String> {
        Ok(fs::read_to_string(self.path_for(hash))?)
    }

    fn path_for(&self, hash: &str) -> PathBuf {
        self.root.join(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_round_trips_the_content() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path().join("blobs")).unwrap();

        let hash = store.write("hello vision").unwrap();
        assert_eq!(store.read(&hash).unwrap(), "hello vision");
    }

    #[test]
    fn identical_content_hashes_to_the_same_blob() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path().join("blobs")).unwrap();

        let a = store.write("same text").unwrap();
        let b = store.write("same text").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn different_content_hashes_differently() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path().join("blobs")).unwrap();

        let a = store.write("text one").unwrap();
        let b = store.write("text two").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn reading_an_unknown_hash_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let store = BlobStore::open(dir.path().join("blobs")).unwrap();
        assert!(store.read("0000000000").is_err());
    }
}
