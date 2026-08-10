//! Query Orchestrator v1 — retrieval only (`docs/ROADMAP.md` M7): embed the
//! query, brute-force cosine search the vector store, return ranked
//! snippets with source citations. No LLM synthesis — that's M8.

use crate::embed::embed_text;
use crate::engine::Engine;
use crate::error::CoreResult;

pub struct RankedResult {
    pub document_id: String,
    pub path: String,
    pub snippet: String,
    pub score: f32,
    pub timestamp_unix_ms: i64,
}

pub fn run(engine: &Engine, query_text: &str, top_k: usize) -> CoreResult<Vec<RankedResult>> {
    let query_vector = embed_text(query_text);
    let scored = engine.vectors.search(&query_vector, top_k)?;

    let mut results = Vec::with_capacity(scored.len());
    for chunk in scored {
        // A chunk can only exist for a document that was upserted first
        // (ingest.rs always writes graph before vectors), so a missing
        // document here would mean the two stores disagreed — skip rather
        // than panic, since a citation-less result is still better than a
        // crashed query.
        let Some(doc) = engine.graph.get(&chunk.document_id)? else {
            continue;
        };
        results.push(RankedResult {
            document_id: chunk.document_id,
            path: doc.path,
            snippet: chunk.text,
            score: chunk.score,
            timestamp_unix_ms: doc.created_at_unix_ms,
        });
    }
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest;
    use vision_proto::IngestSource;

    #[test]
    fn query_surfaces_the_matching_previously_indexed_file() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::open(&dir.path().join("data")).unwrap();

        let cats_path = dir.path().join("cats.md");
        std::fs::write(
            &cats_path,
            "Cats are small domesticated carnivorous mammals.",
        )
        .unwrap();
        ingest::run(&engine, &cats_path, IngestSource::Filesystem as i32).unwrap();

        let budget_path = dir.path().join("budget.md");
        std::fs::write(&budget_path, "Q3 revenue projections and sales targets.").unwrap();
        ingest::run(&engine, &budget_path, IngestSource::Filesystem as i32).unwrap();

        let results = run(&engine, "tell me about cats", 5).unwrap();

        assert!(!results.is_empty());
        assert!(results[0].path.ends_with("cats.md"));
        assert!(results[0].snippet.to_lowercase().contains("cats"));
    }

    #[test]
    fn query_against_an_empty_index_returns_no_results_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::open(&dir.path().join("data")).unwrap();

        let results = run(&engine, "anything", 5).unwrap();
        assert!(results.is_empty());
    }
}
