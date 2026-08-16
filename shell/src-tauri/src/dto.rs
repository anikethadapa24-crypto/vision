//! Plain, `serde`-serializable mirrors of the prost-generated proto types
//! (`vision-proto` doesn't derive `Serialize` — adding that would be a
//! shared-crate change for a UI-only need), so events emitted to the
//! frontend have a stable JSON shape independent of the wire format.

use serde::Serialize;

#[derive(Clone, Serialize)]
pub struct SourceRefDto {
    pub document_id: String,
    pub path: String,
    pub timestamp_unix_ms: i64,
}

#[derive(Clone, Serialize)]
pub struct AnswerChunkDto {
    pub token: String,
    pub is_final: bool,
    pub sources: Vec<SourceRefDto>,
}

impl From<vision_proto::AnswerChunk> for AnswerChunkDto {
    fn from(chunk: vision_proto::AnswerChunk) -> Self {
        Self {
            token: chunk.token,
            is_final: chunk.is_final,
            sources: chunk
                .sources
                .into_iter()
                .map(|s| SourceRefDto {
                    document_id: s.document_id,
                    path: s.path,
                    timestamp_unix_ms: s.timestamp_unix_ms,
                })
                .collect(),
        }
    }
}
