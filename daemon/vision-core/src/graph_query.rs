//! Assembles the `GetGraph` RPC's response: every ingested document as a
//! node, plus edges from real cosine similarity between documents' chunk-
//! embedding centroids (`stores::vectors::VectorStore::document_centroids`)
//! — not a placeholder graph, though it rides on the same embedding
//! stand-in as retrieval (`embed.rs`'s hashing vectorizer), so edges reflect
//! lexical overlap, not true semantic relatedness. Real Kùzu integration
//! (typed edges, entity extraction) is tracked in `docs/TASKS.md`'s Parking
//! Lot; this is the honest version of what today's stand-ins can produce.

use crate::engine::Engine;
use crate::error::CoreResult;
use crate::stores::vectors::cosine_similarity;

pub struct NodeRecord {
    pub id: String,
    pub path: String,
    pub source: i32,
    pub created_at_unix_ms: i64,
}

pub struct EdgeRecord {
    pub from_id: String,
    pub to_id: String,
    pub weight: f32,
}

/// Below this cosine similarity, two documents are considered unrelated and
/// no edge is drawn — otherwise the hashing vectorizer's bucket collisions
/// on common short words would connect nearly everything. Also caps edges
/// per node to its `EDGES_PER_NODE` strongest matches, so a hub document
/// (e.g. one sharing vocabulary with everything) doesn't turn the graph
/// into a dense, unreadable ball at demo scale.
const SIMILARITY_THRESHOLD: f32 = 0.12;
const EDGES_PER_NODE: usize = 3;

pub fn run(engine: &Engine) -> CoreResult<(Vec<NodeRecord>, Vec<EdgeRecord>)> {
    let documents = engine.graph.list_all()?;
    let nodes: Vec<NodeRecord> = documents
        .iter()
        .map(|d| NodeRecord {
            id: d.id.clone(),
            path: d.path.clone(),
            source: d.source,
            created_at_unix_ms: d.created_at_unix_ms,
        })
        .collect();

    let centroids = engine.vectors.document_centroids()?;
    let mut edges = Vec::new();
    let mut seen_pairs = std::collections::HashSet::new();

    for (i, (id_a, vec_a)) in centroids.iter().enumerate() {
        let mut best: Vec<(f32, &str)> = centroids
            .iter()
            .enumerate()
            .filter(|(j, _)| *j != i)
            .map(|(_, (id_b, vec_b))| (cosine_similarity(vec_a, vec_b), id_b.as_str()))
            .filter(|(score, _)| *score >= SIMILARITY_THRESHOLD)
            .collect();
        best.sort_by(|a, b| b.0.total_cmp(&a.0));
        best.truncate(EDGES_PER_NODE);

        for (score, id_b) in best {
            let pair = if id_a.as_str() < id_b {
                (id_a.clone(), id_b.to_string())
            } else {
                (id_b.to_string(), id_a.clone())
            };
            if seen_pairs.insert(pair.clone()) {
                edges.push(EdgeRecord {
                    from_id: pair.0,
                    to_id: pair.1,
                    weight: score,
                });
            }
        }
    }

    Ok((nodes, edges))
}
