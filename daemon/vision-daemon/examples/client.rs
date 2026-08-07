//! Manual verification tool: dials the real daemon over its real named
//! pipe (`transport::windows::PIPE_NAME`) and prints what comes back.
//! Run `cargo run -p vision-daemon` in one terminal, then
//! `cargo run -p vision-daemon --example client` in another.
#![cfg(windows)]

use tonic::transport::Endpoint;
use tonic::Request;

use vision_daemon::transport::windows::{NamedPipeConnector, PIPE_NAME};
use vision_proto::vision_api_client::VisionApiClient;
use vision_proto::IngestEventRequest;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("connecting to {PIPE_NAME}...");
    let channel = Endpoint::from_static("http://[::]:0")
        .connect_with_connector(NamedPipeConnector::new(PIPE_NAME))
        .await?;
    let mut client = VisionApiClient::new(channel);

    let resp = client
        .ingest_event(Request::new(IngestEventRequest {
            path_or_url: "C:\\scratch\\manual-test.md".to_string(),
            ..Default::default()
        }))
        .await?
        .into_inner();

    println!("IngestEvent -> {resp:?}");
    Ok(())
}
