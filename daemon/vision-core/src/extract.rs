//! Text extraction (`docs/ARCHITECTURE.md` §6.1, `docs/ROADMAP.md` M5).
//! Dispatches by file extension. Unsupported types degrade gracefully —
//! `None`, not an error — matching M5's exit criteria ("unsupported types
//! degrade... indexed as metadata-only, not crash").

use std::path::Path;

use crate::error::{CoreError, CoreResult};

const PLAIN_TEXT_EXTENSIONS: &[&str] = &[
    "txt", "md", "markdown", "rs", "py", "js", "jsx", "ts", "tsx", "go", "java", "c", "h", "cpp",
    "hpp", "cs", "rb", "php", "sh", "json", "toml", "yaml", "yml", "html", "css", "proto",
];

/// Returns `Ok(None)` for a recognized-but-unsupported or extensionless
/// file (metadata-only indexing), `Err` only for a genuine read/parse
/// failure on a type we do claim to support.
pub fn extract_text(path: &Path) -> CoreResult<Option<String>> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_lowercase());

    match ext.as_deref() {
        Some(ext) if PLAIN_TEXT_EXTENSIONS.contains(&ext) => {
            Ok(Some(std::fs::read_to_string(path)?))
        }
        Some("pdf") => pdf_extract::extract_text(path)
            .map(Some)
            .map_err(|e| CoreError::Extract(e.to_string())),
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_text_files_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("note.md");
        std::fs::write(&path, "# hello\nvision").unwrap();

        assert_eq!(extract_text(&path).unwrap().unwrap(), "# hello\nvision");
    }

    #[test]
    fn recognizes_code_file_extensions_as_plain_text() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("main.rs");
        std::fs::write(&path, "fn main() {}").unwrap();

        assert_eq!(extract_text(&path).unwrap().unwrap(), "fn main() {}");
    }

    #[test]
    fn unsupported_extension_degrades_to_none_not_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("image.png");
        std::fs::write(&path, [0u8, 1, 2, 3]).unwrap();

        assert!(extract_text(&path).unwrap().is_none());
    }

    #[test]
    fn extensionless_file_degrades_to_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("README");
        std::fs::write(&path, "no extension").unwrap();

        assert!(extract_text(&path).unwrap().is_none());
    }

    #[test]
    fn missing_file_with_a_supported_extension_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("missing.txt");
        assert!(extract_text(&path).is_err());
    }
}
