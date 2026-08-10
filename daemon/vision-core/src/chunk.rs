//! Chunking (`docs/ROADMAP.md` M5): fixed-size sliding window over
//! characters (not bytes, so multi-byte UTF-8 never gets split mid-codepoint)
//! with overlap, sized for downstream embedding.

pub const DEFAULT_CHUNK_SIZE: usize = 800;
pub const DEFAULT_OVERLAP: usize = 100;

/// Splits `text` into chunks of at most `chunk_size` chars, each starting
/// `chunk_size - overlap` chars after the previous one. Empty input yields
/// no chunks. Panics if `overlap >= chunk_size` (would never advance).
pub fn chunk_text(text: &str, chunk_size: usize, overlap: usize) -> Vec<String> {
    assert!(
        overlap < chunk_size,
        "overlap ({overlap}) must be smaller than chunk_size ({chunk_size}) or chunking would never advance"
    );

    let chars: Vec<char> = text.chars().collect();
    if chars.is_empty() {
        return Vec::new();
    }

    let stride = chunk_size - overlap;
    let mut chunks = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let end = (start + chunk_size).min(chars.len());
        chunks.push(chars[start..end].iter().collect());
        if end == chars.len() {
            break;
        }
        start += stride;
    }
    chunks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_yields_no_chunks() {
        assert!(chunk_text("", 10, 2).is_empty());
    }

    #[test]
    fn text_shorter_than_chunk_size_yields_one_chunk() {
        let chunks = chunk_text("hello", 10, 2);
        assert_eq!(chunks, vec!["hello"]);
    }

    #[test]
    fn text_longer_than_chunk_size_overlaps_correctly() {
        // 10 chars, chunk_size=4, overlap=1 -> stride=3
        let text = "abcdefghij";
        let chunks = chunk_text(text, 4, 1);
        assert_eq!(chunks, vec!["abcd", "defg", "ghij"]);
    }

    #[test]
    fn every_char_is_covered_by_at_least_one_chunk() {
        let text = "the quick brown fox jumps over the lazy dog";
        let chunks = chunk_text(text, 10, 3);
        let covered: String = chunks.join("");
        for c in text.chars() {
            assert!(covered.contains(c));
        }
    }

    #[test]
    fn multi_byte_utf8_is_never_split_mid_codepoint() {
        let text = "caf\u{e9} \u{1f600} vision"; // é + emoji
        let chunks = chunk_text(text, 5, 1);
        let covered: String = chunks.join("");
        for c in text.chars() {
            assert!(covered.contains(c));
        }
    }

    #[test]
    #[should_panic(expected = "overlap")]
    fn overlap_greater_or_equal_to_chunk_size_panics() {
        chunk_text("abcdef", 4, 4);
    }
}
