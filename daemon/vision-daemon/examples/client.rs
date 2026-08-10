//! Manual verification tool: dials the real daemon over its real named
//! pipe (`transport::windows::PIPE_NAME`) and prints what comes back.
//! Run `cargo run -p vision-daemon` in one terminal, then
//! `cargo run -p vision-daemon --example client` in another.
//!
//! Windows-only (named pipe transport); `cargo build --all-targets` still
//! needs this crate to have a `main` on every OS, so non-Windows gets a
//! stub rather than `#![cfg(windows)]` stripping the whole file — that
//! left no `main` at all and broke the Linux/macOS CI legs (E0601).

#[cfg(windows)]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use tonic::transport::Endpoint;
    use tonic::Request;

    use vision_daemon::transport::windows::{NamedPipeConnector, PIPE_NAME};
    use vision_proto::vision_api_client::VisionApiClient;
    use vision_proto::IngestEventRequest;

    println!("connecting to {PIPE_NAME}...");
    let channel = Endpoint::from_static("http://[::]:0")
        .connect_with_connector(NamedPipeConnector::new(PIPE_NAME))
        .await?;
    let mut client = VisionApiClient::new(channel);

    // Ingest reads real file content now (M2-M7), so this needs a real file
    // to point at rather than a made-up path.
    let scratch_dir = std::env::temp_dir().join("vision-client-example");
    std::fs::create_dir_all(&scratch_dir)?;
    let file_path = scratch_dir.join("manual-test.md");
    std::fs::write(&file_path, "hello from examples/client.rs")?;

    let resp = client
        .ingest_event(Request::new(IngestEventRequest {
            path_or_url: file_path.to_string_lossy().to_string(),
            ..Default::default()
        }))
        .await?
        .into_inner();

    println!("IngestEvent -> {resp:?}");
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("this example exercises the named-pipe transport, which is Windows-only");
}
