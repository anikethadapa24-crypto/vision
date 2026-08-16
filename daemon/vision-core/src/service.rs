use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;

use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use vision_proto::vision_api_server::VisionApi;
use vision_proto::{
    AnswerChunk, DeleteAuditRequest, DeleteAuditResponse, GetPermissionsRequest,
    GetPermissionsResponse, IngestEventRequest, IngestEventResponse, ListAuditRequest,
    ListAuditResponse, QueryRequest, RevokePermissionRequest, RevokePermissionResponse,
    SetPermissionRequest, SetPermissionResponse, SourceRef,
};

use crate::engine::Engine;
use crate::{ingest, query, synthesize};

/// How many ranked results `Query` returns per request. Fixed for now —
/// exposing it as a request field is a proto change with no caller today.
const TOP_K: usize = 5;

/// Generation length cap. Trimmed from 200 to keep a live-demo query under
/// ~30s on a 1.1B CPU model (~200 tokens measured ~50-60s) while still
/// leaving room for a couple of grounded sentences.
const MAX_NEW_TOKENS: usize = 110;

/// The real `VisionApi` gRPC service (`docs/ARCHITECTURE.md` §4.2). Every
/// RPC is backed by `Engine` — see its module doc for what's real storage
/// vs. an interim stand-in (`stores::graph`/`stores::vectors`/`embed`).
#[derive(Clone)]
pub struct VisionApiService {
    engine: Arc<Engine>,
}

impl VisionApiService {
    pub fn new(engine: Arc<Engine>) -> Self {
        Self { engine }
    }
}

/// Maps an internal storage/pipeline failure to a gRPC status. Every such
/// failure is a server-side problem from the client's point of view (bad
/// input either round-trips through `IngestSource`'s default-on-unknown
/// behavior, or protobuf itself rejects unparseable request bytes before we
/// ever see them) so `internal` is the right code across the board here.
fn to_status<E: std::fmt::Display>(e: E) -> Status {
    Status::internal(e.to_string())
}

async fn run_blocking<F, T>(f: F) -> Result<T, Status>
where
    F: FnOnce() -> crate::error::CoreResult<T> + Send + 'static,
    T: Send + 'static,
{
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| Status::internal(format!("blocking task panicked: {e}")))?
        .map_err(to_status)
}

#[tonic::async_trait]
impl VisionApi for VisionApiService {
    async fn ingest_event(
        &self,
        request: Request<IngestEventRequest>,
    ) -> Result<Response<IngestEventResponse>, Status> {
        let req = request.into_inner();
        let engine = self.engine.clone();
        let path = PathBuf::from(&req.path_or_url);
        let source = req.source;

        let outcome = run_blocking(move || ingest::run(&engine, &path, source)).await?;

        Ok(Response::new(IngestEventResponse {
            event_id: outcome.audit_id,
            accepted: true,
        }))
    }

    type QueryStream = Pin<Box<dyn Stream<Item = Result<AnswerChunk, Status>> + Send + 'static>>;

    async fn query(
        &self,
        request: Request<QueryRequest>,
    ) -> Result<Response<Self::QueryStream>, Status> {
        let text = request.into_inner().text;
        let engine = self.engine.clone();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        // Runs on the blocking pool: retrieval + (usually) generation, both
        // synchronous CPU/disk work. Streams tokens live via `tx` as
        // they're produced; the closure's return value becomes the final
        // chunk's citation list once this task resolves.
        let synth_task = tokio::task::spawn_blocking(move || {
            if engine.llm().is_err() {
                // No local model available (not downloaded yet, no
                // network, load failed) — degrade to M7's retrieval-only
                // snippets rather than failing the whole query. Ranked
                // sources are still useful on their own; see
                // docs/TASKS.md's Parking Lot.
                let results = query::run(&engine, &text, TOP_K)?;
                for r in &results {
                    let _ = tx.send(r.snippet.clone());
                }
                return Ok(results);
            }
            synthesize::run(&engine, &text, TOP_K, MAX_NEW_TOKENS, |token| {
                let _ = tx.send(token.to_string());
            })
        });

        let output = async_stream::stream! {
            while let Some(token) = rx.recv().await {
                yield Ok(AnswerChunk { token, is_final: false, sources: vec![] });
            }
            match synth_task.await {
                Ok(Ok(sources)) => {
                    let sources = sources
                        .into_iter()
                        .map(|r| SourceRef {
                            document_id: r.document_id,
                            path: r.path,
                            timestamp_unix_ms: r.timestamp_unix_ms,
                        })
                        .collect();
                    yield Ok(AnswerChunk { token: String::new(), is_final: true, sources });
                }
                Ok(Err(e)) => yield Err(to_status(e)),
                Err(e) => yield Err(Status::internal(format!("query task panicked: {e}"))),
            }
        };

        let stream: Self::QueryStream = Box::pin(output);
        Ok(Response::new(stream))
    }

    async fn get_permissions(
        &self,
        _request: Request<GetPermissionsRequest>,
    ) -> Result<Response<GetPermissionsResponse>, Status> {
        let engine = self.engine.clone();
        let permissions = run_blocking(move || engine.config.list()).await?;
        Ok(Response::new(GetPermissionsResponse { permissions }))
    }

    async fn set_permission(
        &self,
        request: Request<SetPermissionRequest>,
    ) -> Result<Response<SetPermissionResponse>, Status> {
        let Some(scope) = request.into_inner().permission else {
            return Err(Status::invalid_argument("permission is required"));
        };
        let engine = self.engine.clone();
        run_blocking(move || engine.config.set(&scope)).await?;
        Ok(Response::new(SetPermissionResponse { success: true }))
    }

    async fn revoke_permission(
        &self,
        request: Request<RevokePermissionRequest>,
    ) -> Result<Response<RevokePermissionResponse>, Status> {
        let path = request.into_inner().path;
        let engine = self.engine.clone();
        run_blocking(move || engine.config.revoke(&path)).await?;
        Ok(Response::new(RevokePermissionResponse { success: true }))
    }

    async fn list_audit(
        &self,
        _request: Request<ListAuditRequest>,
    ) -> Result<Response<ListAuditResponse>, Status> {
        let engine = self.engine.clone();
        let entries = run_blocking(move || engine.audit.list()).await?;
        Ok(Response::new(ListAuditResponse { entries }))
    }

    async fn delete_audit(
        &self,
        request: Request<DeleteAuditRequest>,
    ) -> Result<Response<DeleteAuditResponse>, Status> {
        let id = request.into_inner().id;
        let engine = self.engine.clone();
        run_blocking(move || engine.audit.soft_delete(&id)).await?;
        Ok(Response::new(DeleteAuditResponse { success: true }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;
    use vision_proto::{IngestSource, PermissionScope, PermissionScopeType};

    fn test_service() -> (VisionApiService, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let engine = Engine::open(&dir.path().join("data")).unwrap();
        (VisionApiService::new(Arc::new(engine)), dir)
    }

    #[tokio::test]
    async fn ingest_event_indexes_a_real_file_and_returns_an_audit_id() {
        let (svc, dir) = test_service();
        let file_path = dir.path().join("note.md");
        std::fs::write(&file_path, "hello from a real file").unwrap();

        let resp = svc
            .ingest_event(Request::new(IngestEventRequest {
                source: IngestSource::Filesystem as i32,
                path_or_url: file_path.to_string_lossy().to_string(),
                content_ref: String::new(),
            }))
            .await
            .unwrap()
            .into_inner();

        assert!(resp.accepted);
        assert!(!resp.event_id.is_empty());
    }

    #[tokio::test]
    #[ignore = "downloads and runs the real local LLM on first call — see docs/TASKS.md's \
                Parking Lot; run explicitly with `cargo test -- --ignored`"]
    async fn query_stream_ends_with_a_final_chunk() {
        let (svc, _dir) = test_service();
        let stream = svc
            .query(Request::new(QueryRequest {
                text: "anything".to_string(),
            }))
            .await
            .unwrap()
            .into_inner();
        let chunks: Vec<AnswerChunk> = stream.collect::<Result<Vec<_>, _>>().await.unwrap();

        assert!(chunks.last().unwrap().is_final);
    }

    #[tokio::test]
    #[ignore = "downloads and runs the real local LLM on first call — see docs/TASKS.md's \
                Parking Lot; run explicitly with `cargo test -- --ignored`"]
    async fn query_after_ingest_surfaces_the_indexed_file_with_a_citation() {
        let (svc, dir) = test_service();
        let file_path = dir.path().join("cats.md");
        std::fs::write(&file_path, "cats are wonderful small mammals").unwrap();

        svc.ingest_event(Request::new(IngestEventRequest {
            source: IngestSource::Filesystem as i32,
            path_or_url: file_path.to_string_lossy().to_string(),
            content_ref: String::new(),
        }))
        .await
        .unwrap();

        let stream = svc
            .query(Request::new(QueryRequest {
                text: "tell me about cats".to_string(),
            }))
            .await
            .unwrap()
            .into_inner();
        let chunks: Vec<AnswerChunk> = stream.collect::<Result<Vec<_>, _>>().await.unwrap();

        let cited_paths: Vec<String> = chunks
            .iter()
            .flat_map(|c| c.sources.iter())
            .map(|s| s.path.clone())
            .collect();
        assert!(cited_paths.iter().any(|p| p.ends_with("cats.md")));
    }

    #[tokio::test]
    async fn get_permissions_is_empty_before_anything_is_granted() {
        let (svc, _dir) = test_service();
        let resp = svc
            .get_permissions(Request::new(GetPermissionsRequest {}))
            .await
            .unwrap()
            .into_inner();
        assert!(resp.permissions.is_empty());
    }

    #[tokio::test]
    async fn set_then_get_then_revoke_permission_round_trips_for_real() {
        let (svc, _dir) = test_service();

        let set = svc
            .set_permission(Request::new(SetPermissionRequest {
                permission: Some(PermissionScope {
                    path: "C:\\notes".to_string(),
                    scope_type: PermissionScopeType::Folder as i32,
                    granted: true,
                }),
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(set.success);

        let listed = svc
            .get_permissions(Request::new(GetPermissionsRequest {}))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(listed.permissions.len(), 1);
        assert_eq!(listed.permissions[0].path, "C:\\notes");

        let revoke = svc
            .revoke_permission(Request::new(RevokePermissionRequest {
                path: "C:\\notes".to_string(),
            }))
            .await
            .unwrap()
            .into_inner();
        assert!(revoke.success);

        let listed_after = svc
            .get_permissions(Request::new(GetPermissionsRequest {}))
            .await
            .unwrap()
            .into_inner();
        assert!(listed_after.permissions.is_empty());
    }

    #[tokio::test]
    async fn list_audit_reflects_real_ingests_and_delete_soft_deletes() {
        let (svc, dir) = test_service();
        let file_path = dir.path().join("note.md");
        std::fs::write(&file_path, "some content").unwrap();

        svc.ingest_event(Request::new(IngestEventRequest {
            source: IngestSource::Filesystem as i32,
            path_or_url: file_path.to_string_lossy().to_string(),
            content_ref: String::new(),
        }))
        .await
        .unwrap();

        let list = svc
            .list_audit(Request::new(ListAuditRequest {}))
            .await
            .unwrap()
            .into_inner();
        assert_eq!(list.entries.len(), 1);
        let entry_id = list.entries[0].id.clone();

        let delete = svc
            .delete_audit(Request::new(DeleteAuditRequest { id: entry_id }))
            .await
            .unwrap()
            .into_inner();
        assert!(delete.success);

        let list_after = svc
            .list_audit(Request::new(ListAuditRequest {}))
            .await
            .unwrap()
            .into_inner();
        assert!(list_after.entries.is_empty());
    }
}
