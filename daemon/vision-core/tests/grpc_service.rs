//! Proves the M1 stub `VisionApi` service round-trips over a *real* gRPC
//! wire connection — a generated client, real protobuf serialization, real
//! HTTP/2 framing — not just direct trait calls in-process.
//!
//! This binds to a loopback TCP port purely as test scaffolding. It is not
//! the production transport: that's UDS (macOS/Linux) and named pipe
//! (Windows), each its own task in `docs/TASKS.md` M1, wired into
//! `vision-daemon`'s binary once implemented.

use std::net::SocketAddr;
use std::time::Duration;

use tokio::net::TcpListener;
use tokio::time::timeout;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_stream::StreamExt;
use tonic::transport::Server;
use tonic::Request;

use vision_core::VisionApiService;
use vision_proto::vision_api_client::VisionApiClient;
use vision_proto::{
    DeleteAuditRequest, GetPermissionsRequest, IngestEventRequest, ListAuditRequest,
    QueryRequest, RevokePermissionRequest, SetPermissionRequest,
};

/// Starts the stub service on an OS-assigned loopback port and returns a
/// connected client. The server task is detached; the OS reclaims the port
/// when the test process exits.
async fn spawn_server_and_connect() -> VisionApiClient<tonic::transport::Channel> {
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let listener = TcpListener::bind(addr).await.unwrap();
    let local_addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        Server::builder()
            .add_service(vision_proto::vision_api_server::VisionApiServer::new(
                VisionApiService,
            ))
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });

    let uri = format!("http://{local_addr}");
    timeout(Duration::from_secs(5), VisionApiClient::connect(uri))
        .await
        .expect("server did not become ready in time")
        .expect("client failed to connect")
}

#[tokio::test]
async fn ingest_event_round_trips_over_real_grpc() {
    let mut client = spawn_server_and_connect().await;

    let resp = client
        .ingest_event(Request::new(IngestEventRequest {
            path_or_url: "C:\\scratch\\note.md".to_string(),
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(resp.accepted);
    assert_eq!(resp.event_id, "stub-event-id");
}

#[tokio::test]
async fn query_streams_a_final_chunk_over_real_grpc() {
    let mut client = spawn_server_and_connect().await;

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
async fn permissions_and_audit_rpcs_round_trip_over_real_grpc() {
    let mut client = spawn_server_and_connect().await;

    let permissions = client
        .get_permissions(Request::new(GetPermissionsRequest {}))
        .await
        .unwrap()
        .into_inner();
    assert!(permissions.permissions.is_empty());

    let set = client
        .set_permission(Request::new(SetPermissionRequest::default()))
        .await
        .unwrap()
        .into_inner();
    assert!(set.success);

    let revoke = client
        .revoke_permission(Request::new(RevokePermissionRequest {
            path: "C:\\scratch".to_string(),
        }))
        .await
        .unwrap()
        .into_inner();
    assert!(revoke.success);

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
