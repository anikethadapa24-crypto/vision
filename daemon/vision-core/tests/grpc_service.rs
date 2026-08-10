//! Proves the real `VisionApi` service round-trips over a *real* gRPC wire
//! connection — a generated client, real protobuf serialization, real
//! HTTP/2 framing, a real temp-dir-backed `Engine` — not just direct trait
//! calls in-process.
//!
//! This binds to a loopback TCP port purely as test scaffolding. It is not
//! the production transport: that's UDS (macOS/Linux, still open) and named
//! pipe (Windows, done — see `vision-daemon/tests/named_pipe.rs`).

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_stream::StreamExt;
use tonic::transport::Server;
use tonic::Request;

use vision_core::{Engine, VisionApiService};
use vision_proto::vision_api_client::VisionApiClient;
use vision_proto::{
    DeleteAuditRequest, GetPermissionsRequest, IngestEventRequest, IngestSource, ListAuditRequest,
    PermissionScope, PermissionScopeType, QueryRequest, RevokePermissionRequest,
    SetPermissionRequest,
};

/// Starts the real service, backed by a fresh temp-dir `Engine`, on an
/// OS-assigned loopback port and returns a connected client plus the temp
/// dir (kept alive for the caller to write test files into).
async fn spawn_server_and_connect() -> (
    VisionApiClient<tonic::transport::Channel>,
    tempfile::TempDir,
) {
    let dir = tempfile::tempdir().unwrap();
    let engine = Engine::open(&dir.path().join("data")).unwrap();
    let service = VisionApiService::new(Arc::new(engine));

    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = TcpListener::bind(addr).await.unwrap();
    let local_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        Server::builder()
            .add_service(vision_proto::vision_api_server::VisionApiServer::new(
                service,
            ))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    let uri = format!("http://{local_addr}");
    let client = timeout(Duration::from_secs(5), VisionApiClient::connect(uri))
        .await
        .expect("server did not become ready in time")
        .expect("client failed to connect");
    (client, dir)
}

#[tokio::test]
async fn ingest_event_round_trips_over_real_grpc() {
    let (mut client, dir) = spawn_server_and_connect().await;
    let file_path = dir.path().join("note.md");
    std::fs::write(&file_path, "a real note, indexed for real").unwrap();

    let resp = client
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
async fn query_streams_a_final_chunk_over_real_grpc() {
    let (mut client, _dir) = spawn_server_and_connect().await;

    let mut stream = client
        .query(Request::new(QueryRequest {
            text: "what did I read last week?".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let mut saw_final = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        if chunk.is_final {
            saw_final = true;
        }
    }
    assert!(saw_final, "stream never sent a final chunk");
}

#[tokio::test]
async fn ingest_then_query_surfaces_a_real_cited_result_over_real_grpc() {
    let (mut client, dir) = spawn_server_and_connect().await;
    let file_path = dir.path().join("cats.md");
    std::fs::write(
        &file_path,
        "cats are small domesticated carnivorous mammals",
    )
    .unwrap();

    client
        .ingest_event(Request::new(IngestEventRequest {
            source: IngestSource::Filesystem as i32,
            path_or_url: file_path.to_string_lossy().to_string(),
            content_ref: String::new(),
        }))
        .await
        .unwrap();

    let mut stream = client
        .query(Request::new(QueryRequest {
            text: "tell me about cats".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();

    let mut cited_paths = Vec::new();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.unwrap();
        cited_paths.extend(chunk.sources.into_iter().map(|s| s.path));
    }
    assert!(cited_paths.iter().any(|p| p.ends_with("cats.md")));
}

#[tokio::test]
async fn permissions_and_audit_rpcs_round_trip_for_real_over_real_grpc() {
    let (mut client, _dir) = spawn_server_and_connect().await;

    let permissions = client
        .get_permissions(Request::new(GetPermissionsRequest {}))
        .await
        .unwrap()
        .into_inner();
    assert!(permissions.permissions.is_empty());

    let set = client
        .set_permission(Request::new(SetPermissionRequest {
            permission: Some(PermissionScope {
                path: "C:\\scratch".to_string(),
                scope_type: PermissionScopeType::Folder as i32,
                granted: true,
            }),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(set.success);

    let after_set = client
        .get_permissions(Request::new(GetPermissionsRequest {}))
        .await
        .unwrap()
        .into_inner();
    assert_eq!(after_set.permissions.len(), 1);
    assert_eq!(after_set.permissions[0].path, "C:\\scratch");

    let revoke = client
        .revoke_permission(Request::new(RevokePermissionRequest {
            path: "C:\\scratch".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(revoke.success);

    let after_revoke = client
        .get_permissions(Request::new(GetPermissionsRequest {}))
        .await
        .unwrap()
        .into_inner();
    assert!(after_revoke.permissions.is_empty());

    let audit = client
        .list_audit(Request::new(ListAuditRequest {}))
        .await
        .unwrap()
        .into_inner();
    assert!(audit.entries.is_empty());

    let delete = client
        .delete_audit(Request::new(DeleteAuditRequest {
            id: "does-not-exist".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(delete.success);
}
