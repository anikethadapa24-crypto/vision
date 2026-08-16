//! A gRPC client to the vision-daemon over the Windows named-pipe transport
//! (`daemon/vision-daemon/src/transport/windows.rs`), held for the app's
//! lifetime and shared across Tauri commands. Reconnects lazily: a failed
//! call drops the cached channel so the next call redials rather than
//! staying wedged — covers the still-open M1 manual-test item ("kill the
//! daemon while the tray app is running, confirm reconnect without a
//! client crash").

use tokio::sync::Mutex;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;

use vision_daemon::transport::windows::{NamedPipeConnector, PIPE_NAME};
use vision_proto::vision_api_client::VisionApiClient;
use vision_proto::{AnswerChunk, QueryRequest};

pub struct DaemonClient {
    pipe_name: String,
    cached: Mutex<Option<VisionApiClient<Channel>>>,
}

impl DaemonClient {
    pub fn new() -> Self {
        Self::with_pipe_name(PIPE_NAME.to_string())
    }

    /// Split out from `new()` so tests can point at a throwaway pipe
    /// instead of the production `PIPE_NAME` (avoids colliding with a
    /// real running daemon, same reasoning as
    /// `vision-daemon/tests/named_pipe.rs`'s `unique_pipe_name`).
    fn with_pipe_name(pipe_name: String) -> Self {
        Self {
            pipe_name,
            cached: Mutex::new(None),
        }
    }

    async fn connect(&self) -> Result<VisionApiClient<Channel>, String> {
        let mut guard = self.cached.lock().await;
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }
        let channel = Endpoint::from_static("http://[::]:0")
            .connect_with_connector(NamedPipeConnector::new(self.pipe_name.clone()))
            .await
            .map_err(|_| "Vision isn't running".to_string())?;
        let client = VisionApiClient::new(channel);
        *guard = Some(client.clone());
        Ok(client)
    }

    /// Drops the cached channel so the next `connect()` redials. Called
    /// whenever an RPC fails, since a stale channel (daemon restarted,
    /// pipe closed) won't recover on its own.
    async fn invalidate(&self) {
        *self.cached.lock().await = None;
    }

    pub async fn query(&self, text: String) -> Result<tonic::Streaming<AnswerChunk>, String> {
        let mut client = self.connect().await?;
        match client.query(Request::new(QueryRequest { text })).await {
            Ok(response) => Ok(response.into_inner()),
            Err(status) => {
                self.invalidate().await;
                Err(format!("query failed: {status}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tokio::sync::oneshot;
    use tokio_stream::StreamExt;
    use vision_core::{Engine, VisionApiService};

    use super::*;

    /// Spins up a real daemon (temp-dir-backed `Engine`, real named pipe)
    /// on a throwaway pipe name and returns a `DaemonClient` pointed at it,
    /// plus the shutdown sender and temp dir (kept alive so the caller can
    /// write files into it).
    async fn spawn_real_daemon(
        test_name: &str,
    ) -> (DaemonClient, oneshot::Sender<()>, tempfile::TempDir) {
        let pipe_name = format!(r"\\.\pipe\vision-shell-test-{test_name}");
        let dir = tempfile::tempdir().unwrap();
        let engine = Arc::new(Engine::open(&dir.path().join("data")).unwrap());

        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let incoming = vision_daemon::transport::windows::incoming(pipe_name.clone());
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(vision_proto::vision_api_server::VisionApiServer::new(
                    VisionApiService::new(engine),
                ))
                .serve_with_incoming_shutdown(incoming, async {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        (DaemonClient::with_pipe_name(pipe_name), shutdown_tx, dir)
    }

    #[tokio::test]
    #[ignore = "downloads and runs the real local LLM on first call — see \
                daemon/docs/TASKS.md's Parking Lot; run explicitly with \
                `cargo test -- --ignored`"]
    async fn query_against_a_real_daemon_streams_a_final_chunk() {
        let (client, shutdown, _dir) = spawn_real_daemon("streams-final-chunk").await;

        let mut stream = client.query("anything".to_string()).await.unwrap();
        let mut saw_final = false;
        while let Some(chunk) = stream.next().await {
            if chunk.unwrap().is_final {
                saw_final = true;
            }
        }
        assert!(saw_final);

        let _ = shutdown.send(());
    }

    #[tokio::test]
    #[ignore = "downloads and runs the real local LLM on first call — see \
                daemon/docs/TASKS.md's Parking Lot; run explicitly with \
                `cargo test -- --ignored`"]
    async fn query_surfaces_a_real_indexed_file_with_its_citation() {
        let (client, shutdown, dir) = spawn_real_daemon("surfaces-citation").await;
        let file_path = dir.path().join("cats.md");
        std::fs::write(
            &file_path,
            "cats are small domesticated carnivorous mammals",
        )
        .unwrap();

        // Drive ingestion the same way the REPL/watcher would: a second
        // client call against the same running daemon.
        let mut ingest_client = client.connect().await.unwrap();
        ingest_client
            .ingest_event(Request::new(vision_proto::IngestEventRequest {
                source: vision_proto::IngestSource::Filesystem as i32,
                path_or_url: file_path.to_string_lossy().to_string(),
                content_ref: String::new(),
            }))
            .await
            .unwrap();

        let mut stream = client
            .query("tell me about cats".to_string())
            .await
            .unwrap();
        let mut cited_paths = Vec::new();
        while let Some(chunk) = stream.next().await {
            cited_paths.extend(chunk.unwrap().sources.into_iter().map(|s| s.path));
        }
        assert!(cited_paths.iter().any(|p| p.ends_with("cats.md")));

        let _ = shutdown.send(());
    }

    /// Fast, no-LLM coverage for the connect-failure path: kept in the
    /// default suite (unlike the tests above) since it never reaches a
    /// real daemon's `Query` RPC at all.
    #[tokio::test]
    async fn query_against_a_nonexistent_pipe_fails_clearly_without_panicking() {
        let client = DaemonClient::with_pipe_name(
            r"\\.\pipe\vision-shell-test-no-daemon-here-at-all".to_string(),
        );

        let result = client.query("anything".to_string()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    #[ignore = "the recovery call downloads and runs the real local LLM — see \
                daemon/docs/TASKS.md's Parking Lot; run explicitly with \
                `cargo test -- --ignored`"]
    async fn query_against_a_dead_daemon_fails_without_panicking_and_a_later_call_can_recover() {
        let pipe_name = r"\\.\pipe\vision-shell-test-no-daemon-here".to_string();
        let client = DaemonClient::with_pipe_name(pipe_name.clone());

        let result = client.query("anything".to_string()).await;
        assert!(result.is_err());

        // A real daemon now comes up on the same pipe name — the client
        // shouldn't stay wedged on the earlier failed connection attempt.
        // (Nothing was cached on a failed *connect*, only on a failed
        // *call after connecting*, so this also exercises that distinction.)
        let dir = tempfile::tempdir().unwrap();
        let engine = Arc::new(Engine::open(&dir.path().join("data")).unwrap());
        let (shutdown_tx, shutdown_rx) = oneshot::channel::<()>();
        let incoming = vision_daemon::transport::windows::incoming(pipe_name);
        tokio::spawn(async move {
            tonic::transport::Server::builder()
                .add_service(vision_proto::vision_api_server::VisionApiServer::new(
                    VisionApiService::new(engine),
                ))
                .serve_with_incoming_shutdown(incoming, async {
                    let _ = shutdown_rx.await;
                })
                .await
                .unwrap();
        });
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        let recovered = client.query("anything".to_string()).await;
        assert!(
            recovered.is_ok(),
            "client should reconnect once the daemon exists"
        );

        let _ = shutdown_tx.send(());
    }
}
