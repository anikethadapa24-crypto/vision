//! Proves the Windows named-pipe transport actually works end to end: a
//! real `VisionApiClient` dials a real named pipe and gets real responses
//! back from a real server, going through the exact `vision_daemon::serve`
//! entry point `main.rs` calls — not a TCP stand-in.
#![cfg(windows)]

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot;
use tonic::transport::Endpoint;
use tonic::Request;

use vision_core::Engine;
use vision_daemon::transport::windows::NamedPipeConnector;
use vision_proto::vision_api_client::VisionApiClient;
use vision_proto::IngestEventRequest;

/// Distinct pipe name per test so parallel `cargo test` runs (and any
/// developer's already-running `vision-daemon.exe`) can't collide.
fn unique_pipe_name(test_name: &str) -> String {
    format!(r"\\.\pipe\vision-daemon-test-{test_name}")
}

#[tokio::test]
async fn client_round_trips_ingest_event_over_a_real_named_pipe() {
    let pipe_name = unique_pipe_name("ingest-event");
    let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();

    let dir = tempfile::tempdir().unwrap();
    let engine = Arc::new(Engine::open(&dir.path().join("data")).unwrap());
    let file_path = dir.path().join("note.md");
    std::fs::write(&file_path, "indexed over a real named pipe").unwrap();

    let incoming = vision_daemon::transport::windows::incoming(pipe_name.clone());
    let server = tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(vision_proto::vision_api_server::VisionApiServer::new(
                vision_core::VisionApiService::new(engine),
            ))
            .serve_with_incoming_shutdown(incoming, async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    // Give the server a moment to create its first pipe instance.
    tokio::time::sleep(Duration::from_millis(100)).await;

    let channel = Endpoint::from_static("http://[::]:0")
        .connect_with_connector(NamedPipeConnector::new(pipe_name))
        .await
        .expect("failed to connect over named pipe");
    let mut client = VisionApiClient::new(channel);

    let resp = client
        .ingest_event(Request::new(IngestEventRequest {
            path_or_url: file_path.to_string_lossy().to_string(),
            ..Default::default()
        }))
        .await
        .unwrap()
        .into_inner();

    assert!(resp.accepted);
    assert!(!resp.event_id.is_empty());

    let _ = shutdown_tx.send(());
    let _ = tokio::time::timeout(Duration::from_secs(2), server).await;
}
