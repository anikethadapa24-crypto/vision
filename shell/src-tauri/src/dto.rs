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

#[derive(Clone, Serialize)]
pub struct GraphNodeDto {
    pub id: String,
    pub path: String,
    /// "filesystem" | "browser" | "unspecified" — stringly-typed rather
    /// than the raw proto enum int so the frontend never has to duplicate
    /// `IngestSource`'s numbering.
    pub source: String,
    pub created_at_unix_ms: i64,
}

#[derive(Clone, Serialize)]
pub struct GraphEdgeDto {
    pub from_id: String,
    pub to_id: String,
    pub weight: f32,
}

#[derive(Clone, Serialize)]
pub struct GetGraphResponseDto {
    pub nodes: Vec<GraphNodeDto>,
    pub edges: Vec<GraphEdgeDto>,
}

fn source_label(source: i32) -> String {
    match vision_proto::IngestSource::try_from(source) {
        Ok(vision_proto::IngestSource::Filesystem) => "filesystem",
        Ok(vision_proto::IngestSource::Browser) => "browser",
        _ => "unspecified",
    }
    .to_string()
}

impl From<vision_proto::GetGraphResponse> for GetGraphResponseDto {
    fn from(resp: vision_proto::GetGraphResponse) -> Self {
        Self {
            nodes: resp
                .nodes
                .into_iter()
                .map(|n| GraphNodeDto {
                    id: n.id,
                    path: n.path,
                    source: source_label(n.source),
                    created_at_unix_ms: n.created_at_unix_ms,
                })
                .collect(),
            edges: resp
                .edges
                .into_iter()
                .map(|e| GraphEdgeDto {
                    from_id: e.from_id,
                    to_id: e.to_id,
                    weight: e.weight,
                })
                .collect(),
        }
    }
}
