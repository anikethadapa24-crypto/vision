//! **Interim stand-in for the LanceDB vector index** decided in
//! `docs/ARCHITECTURE.md` §9.1. Search here is a brute-force cosine scan
//! over every stored chunk — fine at prototype scale, explicitly not what
//! meets the PRD §7.3 100K+ node target (that's real LanceDB's job, M11's
//! job to benchmark). Tracked in `docs/TASKS.md`'s Parking Lot.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection};

use crate::error::CoreResult;

pub struct VectorStore {
    conn: Mutex<Connection>,
}

pub struct NewChunk {
    pub id: String,
    pub document_id: String,
    pub blob_hash: Option<String>,
    pub text: String,
    pub vector: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredChunk {
    pub document_id: String,
    pub text: String,
    pub score: f32,
}

impl VectorStore {
    pub fn open(path: &Path) -> CoreResult<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                blob_hash TEXT,
                text TEXT NOT NULL,
                vector BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS chunks_document_id ON chunks(document_id);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    #[cfg(test)]
    pub fn open_in_memory() -> CoreResult<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS chunks (
                id TEXT PRIMARY KEY,
                document_id TEXT NOT NULL,
                blob_hash TEXT,
                text TEXT NOT NULL,
                vector BLOB NOT NULL
            );
            CREATE INDEX IF NOT EXISTS chunks_document_id ON chunks(document_id);",
        )?;
        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    pub fn insert_chunk(&self, chunk: &NewChunk) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO chunks (id, document_id, blob_hash, text, vector)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                document_id = excluded.document_id,
                blob_hash = excluded.blob_hash,
                text = excluded.text,
                vector = excluded.vector",
            params![
                chunk.id,
                chunk.document_id,
                chunk.blob_hash,
                chunk.text,
                encode_vector(&chunk.vector),
            ],
        )?;
        Ok(())
    }

    /// Drops all chunks for a document — called before re-indexing it so a
    /// shrunk file doesn't leave stale trailing chunks behind.
    pub fn delete_for_document(&self, document_id: &str) -> CoreResult<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM chunks WHERE document_id = ?1",
            params![document_id],
        )?;
        Ok(())
    }

    /// Averages every chunk's vector per `document_id`, for the Graph
    /// Explorer's similarity edges (`GetGraph` RPC) — a document-level
    /// centroid rather than per-chunk, since the graph shows document
    /// nodes, not chunk nodes. Not L2-renormalized after averaging: cosine
    /// similarity is scale-invariant, so an unnormalized centroid compares
    /// identically to a normalized one.
    pub fn document_centroids(&self) -> CoreResult<Vec<(String, Vec<f32>)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT document_id, vector FROM chunks")?;
        let rows = stmt.query_map([], |row| {
            let document_id: String = row.get(0)?;
            let vector_bytes: Vec<u8> = row.get(1)?;
            Ok((document_id, decode_vector(&vector_bytes)))
        })?;

        let mut sums: std::collections::HashMap<String, (Vec<f32>, usize)> =
            std::collections::HashMap::new();
        for row in rows {
            let (document_id, vector) = row?;
            let entry = sums
                .entry(document_id)
                .or_insert_with(|| (vec![0f32; vector.len()], 0));
            for (i, v) in vector.iter().enumerate() {
                entry.0[i] += v;
            }
            entry.1 += 1;
        }

        Ok(sums
            .into_iter()
            .map(|(document_id, (sum, count))| {
                let mean = sum.into_iter().map(|v| v / count as f32).collect();
                (document_id, mean)
            })
            .collect())
    }

    /// Brute-force cosine similarity search, highest score first.
    pub fn search(&self, query_vector: &[f32], top_k: usize) -> CoreResult<Vec<ScoredChunk>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare("SELECT document_id, text, vector FROM chunks")?;
        let rows = stmt.query_map([], |row| {
            let document_id: String = row.get(0)?;
            let text: String = row.get(1)?;
            let vector_bytes: Vec<u8> = row.get(2)?;
            Ok((document_id, text, decode_vector(&vector_bytes)))
        })?;

        let mut scored: Vec<ScoredChunk> = rows
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|(document_id, text, vector)| ScoredChunk {
                document_id,
                text,
                score: cosine_similarity(query_vector, &vector),
            })
            .collect();

        scored.sort_by(|a, b| b.score.total_cmp(&a.score));
        scored.truncate(top_k);
        Ok(scored)
    }
}

fn encode_vector(vector: &[f32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(vector.len() * 4);
    for v in vector {
        bytes.extend_from_slice(&v.to_le_bytes());
    }
    bytes
}

fn decode_vector(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect()
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }
    dot / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(id: &str, document_id: &str, text: &str, vector: Vec<f32>) -> NewChunk {
        NewChunk {
            id: id.to_string(),
            document_id: document_id.to_string(),
            blob_hash: None,
            text: text.to_string(),
            vector,
        }
    }

    #[test]
    fn vector_encoding_round_trips_exactly() {
        let original = vec![0.5_f32, -1.25, 3.0, 0.0];
        let decoded = decode_vector(&encode_vector(&original));
        assert_eq!(original, decoded);
    }

    #[test]
    fn cosine_similarity_of_identical_vectors_is_one() {
        let v = vec![1.0, 2.0, 3.0];
        assert!((cosine_similarity(&v, &v) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_of_orthogonal_vectors_is_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    #[test]
    fn cosine_similarity_handles_a_zero_vector_without_dividing_by_zero() {
        let zero = vec![0.0, 0.0];
        let other = vec![1.0, 1.0];
        assert_eq!(cosine_similarity(&zero, &other), 0.0);
    }

    #[test]
    fn search_ranks_the_closer_vector_first() {
        let store = VectorStore::open_in_memory().unwrap();
        store
            .insert_chunk(&chunk("c1", "doc-a", "about cats", vec![1.0, 0.0]))
            .unwrap();
        store
            .insert_chunk(&chunk("c2", "doc-b", "about dogs", vec![0.0, 1.0]))
            .unwrap();

        let results = store.search(&[1.0, 0.0], 2).unwrap();
        assert_eq!(results[0].document_id, "doc-a");
        assert_eq!(results[1].document_id, "doc-b");
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn search_respects_top_k() {
        let store = VectorStore::open_in_memory().unwrap();
        for i in 0..5 {
            store
                .insert_chunk(&chunk(
                    &format!("c{i}"),
                    &format!("doc-{i}"),
                    "text",
                    vec![1.0, 0.0],
                ))
                .unwrap();
        }
        assert_eq!(store.search(&[1.0, 0.0], 3).unwrap().len(), 3);
    }

    #[test]
    fn delete_for_document_removes_only_that_documents_chunks() {
        let store = VectorStore::open_in_memory().unwrap();
        store
            .insert_chunk(&chunk("c1", "doc-a", "keep", vec![1.0, 0.0]))
            .unwrap();
        store
            .insert_chunk(&chunk("c2", "doc-b", "gone", vec![0.0, 1.0]))
            .unwrap();

        store.delete_for_document("doc-b").unwrap();

        let results = store.search(&[0.0, 1.0], 10).unwrap();
        assert!(results.iter().all(|r| r.document_id != "doc-b"));
    }
}
