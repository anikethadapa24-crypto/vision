//! The Vision daemon: boots the Local API Gateway over the OS-native local
//! transport (`docs/ARCHITECTURE.md` §4.1) and serves `VisionApiService`
//! until asked to shut down.

pub mod single_instance;
pub mod transport;

use std::future::Future;

/// Windows named-pipe transport. macOS/Linux (Unix domain socket) is the
/// next task in `docs/TASKS.md` M1 and not yet implemented.
#[cfg(windows)]
pub async fn serve(
    shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), tonic::transport::Error> {
    use tonic::transport::Server;
    use vision_core::VisionApiService;
    use vision_proto::vision_api_server::VisionApiServer;

    let incoming = transport::windows::incoming(transport::windows::PIPE_NAME.to_string());

    Server::builder()
        .add_service(VisionApiServer::new(VisionApiService))
        .serve_with_incoming_shutdown(incoming, shutdown)
        .await
}

#[cfg(unix)]
pub async fn serve(
    _shutdown: impl Future<Output = ()> + Send + 'static,
) -> Result<(), tonic::transport::Error> {
    unimplemented!(
        "UDS transport is the next M1 task in docs/TASKS.md; named pipe (Windows) is implemented"
    )
}
