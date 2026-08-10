//! **Interim stand-in for the local embedding model** decided in
//! `docs/ARCHITECTURE.md` §9.1 (llama.cpp). This is a hashing vectorizer —
//! feature-hashed, L2-normalized bag-of-words — giving real cosine-similarity
//! search semantics with zero model download, but lexical (word-overlap)
//! matching rather than true semantic matching. Swapping in a real embedding
//! model is tracked in `docs/TASKS.md`'s Parking Lot; nothing downstream
//! (the vector store, the query path) needs to change shape when that
//! happens — only this function's body.

pub const EMBEDDING_DIM: usize = 256;

/// Deterministic (not platform/process dependent) so the same text always
/// embeds to the same vector — required for cosine search to be meaningful
/// across separate daemon runs.
pub fn embed_text(text: &str) -> Vec<f32> {
    let mut vector = vec![0f32; EMBEDDING_DIM];
    for token in tokenize(text) {
        let bucket = (fnv1a(&token) as usize) % EMBEDDING_DIM;
        vector[bucket] += 1.0;
    }

    let norm = vector.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm > 0.0 {
        for v in vector.iter_mut() {
            *v /= norm;
        }
    }
    vector
}

fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_lowercase())
        .collect()
}

/// FNV-1a — simple, deterministic, no external dependency. Not
/// cryptographic; that's fine, this only needs to spread tokens across
/// buckets, not resist adversarial input.
fn fnv1a(s: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        hash ^= b as u64;
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stores::vectors::cosine_similarity;

    #[test]
    fn embedding_has_the_declared_dimension() {
        assert_eq!(embed_text("hello world").len(), EMBEDDING_DIM);
    }

    #[test]
    fn embedding_is_deterministic() {
        assert_eq!(
            embed_text("vision indexes everything"),
            embed_text("vision indexes everything")
        );
    }

    #[test]
    fn embedding_is_l2_normalized() {
        let v = embed_text("the quick brown fox jumps over the lazy dog");
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-5);
    }

    #[test]
    fn empty_text_embeds_to_the_zero_vector() {
        let v = embed_text("");
        assert!(v.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn shared_vocabulary_scores_higher_than_unrelated_text() {
        let query = embed_text("mitosis and cell division");
        let related = embed_text("notes on mitosis, the phases of cell division");
        let unrelated = embed_text("quarterly revenue projections for the sales team");

        assert!(cosine_similarity(&query, &related) > cosine_similarity(&query, &unrelated));
    }

    #[test]
    fn case_and_punctuation_do_not_change_the_embedding() {
        assert_eq!(embed_text("Hello, World!"), embed_text("hello world"));
    }
}
