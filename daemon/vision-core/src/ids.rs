use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// A unique-enough id for rows that don't have a natural key (audit
/// entries). Millisecond timestamp + in-process counter avoids pulling in a
/// UUID dependency for a prototype-scale store — collisions would need two
/// ids generated in the same millisecond with a wrapped counter, which
/// isn't reachable at this scale.
pub fn generate_id(prefix: &str) -> String {
    let millis = now_unix_ms();
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}-{millis}-{seq}")
}

pub fn now_unix_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock is before the Unix epoch")
        .as_millis() as i64
}

/// Deterministic document id for a given path, so re-ingesting the same
/// file upserts the same graph node instead of creating a duplicate.
pub fn document_id_for_path(path: &str) -> String {
    hex_encode(&Sha256::digest(path.as_bytes()))
}

pub fn hex_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generate_id_never_repeats_across_many_calls() {
        let ids: std::collections::HashSet<String> =
            (0..1000).map(|_| generate_id("audit")).collect();
        assert_eq!(ids.len(), 1000);
    }

    #[test]
    fn document_id_for_path_is_stable_and_path_sensitive() {
        let a = document_id_for_path("C:\\notes\\a.md");
        let b = document_id_for_path("C:\\notes\\a.md");
        let c = document_id_for_path("C:\\notes\\b.md");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
}
