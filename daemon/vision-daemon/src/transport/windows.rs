//! Windows named-pipe transport for the Local API Gateway.
//!
//! tonic has no built-in named-pipe support (only TCP and Unix domain
//! sockets), so this mirrors the pattern tonic itself uses for UDS: a
//! [`Connected`] wrapper for the server side and a custom
//! [`tower::Service<Uri>`] connector for the client side, both handing
//! [`hyper_util::rt::TokioIo`]-wrapped streams to tonic.

use std::future::Future;
use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;

use http::Uri;
use hyper_util::rt::TokioIo;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::windows::named_pipe::{ClientOptions, NamedPipeServer, ServerOptions};
use tokio_stream::Stream;
use tonic::transport::server::Connected;
use tower::Service;

/// Fixed, per-machine pipe name. Named pipes are already filesystem-ACL'd
/// to the creating Windows user (`docs/ARCHITECTURE.md` §4.3), so unlike
/// the Safari WebSocket bridge this needs no per-install random token.
pub const PIPE_NAME: &str = r"\\.\pipe\vision-daemon";

/// From the Win32 System Error Codes reference: raised by `CreateFile`/
/// `NamedPipeClient::open` when every server instance is currently
/// handling another client and none is free to accept a new connection.
const ERROR_PIPE_BUSY: i32 = 231;

/// A connected named-pipe server handle, wrapped so it can be handed to
/// `tonic::transport::Server::serve_with_incoming`.
pub struct NamedPipeConnection(NamedPipeServer);

impl AsyncRead for NamedPipeConnection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_read(cx, buf)
    }
}

impl AsyncWrite for NamedPipeConnection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.0).poll_write(cx, buf)
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.0).poll_shutdown(cx)
    }
}

/// Connection info for named-pipe streams (mirrors tonic's `UdsConnectInfo`
/// for Unix domain sockets — there's just nothing named-pipe-specific
/// worth exposing here yet).
#[derive(Clone, Debug)]
pub struct NamedPipeConnectInfo;

impl Connected for NamedPipeConnection {
    type ConnectInfo = NamedPipeConnectInfo;

    fn connect_info(&self) -> Self::ConnectInfo {
        NamedPipeConnectInfo
    }
}

/// Server-side incoming-connection stream for `serve_with_incoming`.
///
/// Windows named pipes require a fresh server instance to be created
/// *before* the next client can connect, so each loop iteration creates
/// the next instance right after handing the previous one off.
pub fn incoming(pipe_name: String) -> impl Stream<Item = io::Result<NamedPipeConnection>> {
    async_stream::stream! {
        loop {
            let server = match ServerOptions::new().create(&pipe_name) {
                Ok(server) => server,
                Err(err) => {
                    yield Err(err);
                    return;
                }
            };
            if let Err(err) = server.connect().await {
                yield Err(err);
                return;
            }
            yield Ok(NamedPipeConnection(server));
        }
    }
}

/// Client-side connector: dials `pipe_name` and hands tonic a
/// `TokioIo`-wrapped stream. Retries on `ERROR_PIPE_BUSY` (all server
/// instances currently busy with other clients) with a short backoff.
#[derive(Clone)]
pub struct NamedPipeConnector {
    pipe_name: String,
}

impl NamedPipeConnector {
    pub fn new(pipe_name: impl Into<String>) -> Self {
        Self {
            pipe_name: pipe_name.into(),
        }
    }
}

type ConnectResult = io::Result<TokioIo<tokio::net::windows::named_pipe::NamedPipeClient>>;

impl Service<Uri> for NamedPipeConnector {
    type Response = TokioIo<tokio::net::windows::named_pipe::NamedPipeClient>;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = ConnectResult> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, _uri: Uri) -> Self::Future {
        let pipe_name = self.pipe_name.clone();
        Box::pin(async move {
            loop {
                match ClientOptions::new().open(&pipe_name) {
                    Ok(client) => return Ok(TokioIo::new(client)),
                    Err(err) if err.raw_os_error() == Some(ERROR_PIPE_BUSY) => {
                        tokio::time::sleep(Duration::from_millis(25)).await;
                    }
                    Err(err) => return Err(err),
                }
            }
        })
    }
}
