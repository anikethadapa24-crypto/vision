//! The ingestion pipeline (`docs/ARCHITECTURE.md` §6.1 Flow A): extract ->
//! chunk -> embed -> blob -> graph -> vector -> audit. One function, called
//! from both the `IngestEvent` RPC handler and the filesystem watcher, so
//! there's exactly one path into storage no matter which client triggered
//! it — matching §1's "single writer" principle at the pipeline level too.

use std::path::Path;

use crate::chunk::{chunk_text, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP};
use crate::embed::embed_text;
use crate::engine::Engine;
use crate::error::CoreResult;
use crate::extract::extract_text;
use crate::stores::audit::NewAuditEntry;
use crate::stores::graph::NewDocument;
use crate::stores::vectors::NewChunk;

pub struct IngestOutcome {
    pub document_id: String,
    pub audit_id: String,
    pub blob_hash: Option<String>,
    pub chunks_indexed: usize,
}

/// Reads `path` from local disk and extracts its text — the
/// `IngestSource::Filesystem` path (watcher, REPL, manual `IngestEvent`).
pub fn run(engine: &Engine, path: &Path, source: i32) -> CoreResult<IngestOutcome> {
    let path_str = path.to_string_lossy().to_string();
    let extracted = extract_text(path)?;
    run_with_content(engine, &path_str, source, extracted)
}

/// The `IngestSource::Browser` path: `url` identifies the document (same
/// role `path` plays for the filesystem case — a stable id for upsert, and
/// what a source citation links back to) but `text` is already-extracted
/// page content handed in directly, not read from disk. Real capture is the
/// browser extension's job (`extension/background.js`); this is the
/// daemon-side half of that pipeline.
pub fn run_browser(
    engine: &Engine,
    url: &str,
    source: i32,
    text: String,
) -> CoreResult<IngestOutcome> {
    let extracted = if text.trim().is_empty() {
        None
    } else {
        Some(text)
    };
    run_with_content(engine, url, source, extracted)
}

fn run_with_content(
    engine: &Engine,
    path_str: &str,
    source: i32,
    extracted: Option<String>,
) -> CoreResult<IngestOutcome> {
    let path_str = path_str.to_string();
    let blob_hash = match &extracted {
        Some(text) => Some(engine.blobs.write(text)?),
        None => None,
    };

    let document_id = engine.graph.upsert_document(NewDocument {
        path: path_str.clone(),
        blob_hash: blob_hash.clone(),
        source,
    })?;

    // Re-indexing safety: drop this document's old chunks before writing
    // the new set, so a shrunk/edited file doesn't leave stale trailing
    // chunks searchable.
    engine.vectors.delete_for_document(&document_id)?;

    let chunks_indexed = if let Some(text) = &extracted {
        let pieces = chunk_text(text, DEFAULT_CHUNK_SIZE, DEFAULT_OVERLAP);
        for (i, piece) in pieces.iter().enumerate() {
            engine.vectors.insert_chunk(&NewChunk {
                id: format!("{document_id}#{i}"),
                document_id: document_id.clone(),
                blob_hash: blob_hash.clone(),
                text: piece.clone(),
                vector: embed_text(piece),
            })?;
        }
        pieces.len()
    } else {
        0
    };

    let audit_entry = engine.audit.append(NewAuditEntry {
        source,
        path: path_str,
    })?;

    Ok(IngestOutcome {
        document_id,
        audit_id: audit_entry.id,
        blob_hash,
        chunks_indexed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use vision_proto::IngestSource;

    #[test]
    fn ingesting_a_text_file_produces_a_blob_document_chunks_and_an_audit_entry() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::open(&dir.path().join("data")).unwrap();
        let file_path = dir.path().join("note.md");
        std::fs::write(&file_path, "Vision indexes everything you've ever read.").unwrap();

        let outcome = run(&engine, &file_path, IngestSource::Filesystem as i32).unwrap();

        assert!(outcome.blob_hash.is_some());
        assert_eq!(outcome.chunks_indexed, 1);
        assert!(!outcome.document_id.is_empty());
        assert_eq!(engine.audit.list().unwrap().len(), 1);

        let blob = engine
            .blobs
            .read(outcome.blob_hash.as_ref().unwrap())
            .unwrap();
        assert_eq!(blob, "Vision indexes everything you've ever read.");
    }

    #[test]
    fn ingesting_an_unsupported_file_type_is_metadata_only_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::open(&dir.path().join("data")).unwrap();
        let file_path = dir.path().join("image.png");
        std::fs::write(&file_path, [0u8, 1, 2, 3]).unwrap();

        let outcome = run(&engine, &file_path, IngestSource::Filesystem as i32).unwrap();

        assert!(outcome.blob_hash.is_none());
        assert_eq!(outcome.chunks_indexed, 0);
        // still gets a graph node + audit entry, per ROADMAP M5's
        // "unsupported types degrade gracefully... metadata-only" criteria
        assert!(engine.graph.get(&outcome.document_id).unwrap().is_some());
        assert_eq!(engine.audit.list().unwrap().len(), 1);
    }

    #[test]
    fn re_ingesting_the_same_path_replaces_its_chunks_instead_of_appending() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::open(&dir.path().join("data")).unwrap();
        let file_path = dir.path().join("note.md");

        std::fs::write(&file_path, "one two three four five six seven eight").unwrap();
        let first = run(&engine, &file_path, IngestSource::Filesystem as i32).unwrap();

        std::fs::write(&file_path, "only one sentence now").unwrap();
        let second = run(&engine, &file_path, IngestSource::Filesystem as i32).unwrap();

        assert_eq!(first.document_id, second.document_id);
        let results = engine
            .vectors
            .search(&embed_text("only one sentence now"), 10)
            .unwrap();
        let this_doc_chunks = results
            .iter()
            .filter(|r| r.document_id == second.document_id)
            .count();
        assert_eq!(this_doc_chunks, 1);
    }
}
