use std::pin::Pin;

use tokio_stream::Stream;
use tonic::{Request, Response, Status};

use vision_proto::vision_api_server::VisionApi;
use vision_proto::{
    AnswerChunk, DeleteAuditRequest, DeleteAuditResponse, GetPermissionsRequest,
    GetPermissionsResponse, IngestEventRequest, IngestEventResponse, ListAuditRequest,
    ListAuditResponse, QueryRequest, RevokePermissionRequest, RevokePermissionResponse,
    SetPermissionRequest, SetPermissionResponse,
};

/// M1 stub implementation of the Local API Gateway contract
/// (`docs/ARCHITECTURE.md` §4.2). Every RPC returns a fixed response — no
/// storage is touched yet. Real persistence replaces this milestone by
/// milestone, starting with Permissions/Audit in M2 (`docs/TASKS.md`).
#[derive(Debug, Default)]
pub struct VisionApiService;

#[tonic::async_trait]
impl VisionApi for VisionApiService {
    async fn ingest_event(
        &self,
        _request: Request<IngestEventRequest>,
    ) -> Result<Response<IngestEventResponse>, Status> {
        Ok(Response::new(IngestEventResponse {
            event_id: "stub-event-id".to_string(),
            accepted: true,
        }))
    }

    type QueryStream = Pin<Box<dyn Stream<Item = Result<AnswerChunk, Status>> + Send + 'static>>;

    async fn query(
        &self,
        _request: Request<QueryRequest>,
    ) -> Result<Response<Self::QueryStream>, Status> {
        let chunks = vec![
            Ok(AnswerChunk {
                token: "Vision can't answer yet — the Query Orchestrator lands in a later \
                        milestone. This is a fixed stub response."
                    .to_string(),
                is_final: false,
                sources: vec![],
            }),
            Ok(AnswerChunk {
                token: String::new(),
                is_final: true,
                sources: vec![],
            }),
        ];
        let stream: Self::QueryStream = Box::pin(tokio_stream::iter(chunks));
        Ok(Response::new(stream))
    }

    async fn get_permissions(
        &self,
        _request: Request<GetPermissionsRequest>,
    ) -> Result<Response<GetPermissionsResponse>, Status> {
        // Opt-in by default (UI.SPEC.md §5b/§5c): nothing is granted until
        // M2 wires this to config.sqlite.
        Ok(Response::new(GetPermissionsResponse {
            permissions: vec![],
        }))
    }

    async fn set_permission(
        &self,
        _request: Request<SetPermissionRequest>,
    ) -> Result<Response<SetPermissionResponse>, Status> {
        Ok(Response::new(SetPermissionResponse { success: true }))
    }

    async fn revoke_permission(
        &self,
        _request: Request<RevokePermissionRequest>,
    ) -> Result<Response<RevokePermissionResponse>, Status> {
        Ok(Response::new(RevokePermissionResponse { success: true }))
    }

    async fn list_audit(
        &self,
        _request: Request<ListAuditRequest>,
    ) -> Result<Response<ListAuditResponse>, Status> {
        Ok(Response::new(ListAuditResponse { entries: vec![] }))
    }

    async fn delete_audit(
        &self,
        _request: Request<DeleteAuditRequest>,
    ) -> Result<Response<DeleteAuditResponse>, Status> {
        Ok(Response::new(DeleteAuditResponse { success: true }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio_stream::StreamExt;

    #[tokio::test]
    async fn ingest_event_returns_fixed_accepted_response() {
        let svc = VisionApiService;
        let resp = svc
            .ingest_event(Request::new(IngestEventRequest::default()))
            .await
            .unwrap()
            .into_inner();
        assert!(resp.accepted);
        assert!(!resp.event_id.is_empty());
    }

    #[tokio::test]
    async fn query_stream_ends_with_a_final_chunk() {
        let svc = VisionApiService;
        let stream = svc
            .query(Request::new(QueryRequest::default()))
            .await
            .unwrap()
            .into_inner();
        let chunks: Vec<AnswerChunk> = stream.collect::<Result<Vec<_>, _>>().await.unwrap();

        assert_eq!(chunks.len(), 2);
        assert!(!chunks[0].is_final);
        assert!(!chunks[0].token.is_empty());
        assert!(chunks[1].is_final);
    }

    #[tokio::test]
    async fn get_permissions_is_empty_until_m2_wires_persistence() {
        let svc = VisionApiService;
        let resp = svc
            .get_permissions(Request::new(GetPermissionsRequest::default()))
            .await
            .unwrap()
            .into_inner();
        assert!(resp.permissions.is_empty());
    }

    #[tokio::test]
    async fn set_and_revoke_permission_report_fixed_success() {
        let svc = VisionApiService;

        let set = svc
            .set_permission(Request::new(SetPermissionRequest::default()))
            .await
            .unwrap()
            .into_inner();
        assert!(set.success);

        let revoke = svc
            .revoke_permission(Request::new(RevokePermissionRequest::default()))
            .await
            .unwrap()
            .into_inner();
        assert!(revoke.success);
    }

    #[tokio::test]
    async fn list_audit_is_empty_and_delete_reports_fixed_success() {
        let svc = VisionApiService;

        let list = svc
            .list_audit(Request::new(ListAuditRequest::default()))
            .await
            .unwrap()
            .into_inner();
        assert!(list.entries.is_empty());

        let delete = svc
            .delete_audit(Request::new(DeleteAuditRequest::default()))
            .await
            .unwrap()
            .into_inner();
        assert!(delete.success);
    }
}
