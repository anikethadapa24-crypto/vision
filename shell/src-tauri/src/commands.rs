//! Tauri commands invoked from the Query UI frontend.

use tauri::{AppHandle, Emitter, State};
use tokio_stream::StreamExt;

use crate::daemon_client::DaemonClient;
use crate::dto::AnswerChunkDto;

/// Streams a `Query` RPC's results to the frontend as events rather than a
/// single return value, so the UI can render Thinking -> Streaming ->
/// Answered as chunks actually arrive (`docs/UI.SPEC.md` §4), not all at
/// once after the whole response lands.
#[tauri::command]
pub async fn submit_query(
    app: AppHandle,
    daemon: State<'_, DaemonClient>,
    text: String,
) -> Result<(), String> {
    let mut stream = match daemon.query(text).await {
        Ok(stream) => stream,
        Err(err) => {
            let _ = app.emit("query-error", err.clone());
            return Err(err);
        }
    };

    loop {
        match stream.next().await {
            Some(Ok(chunk)) => {
                let _ = app.emit("query-chunk", AnswerChunkDto::from(chunk));
            }
            Some(Err(status)) => {
                let message = format!("query stream failed: {status}");
                let _ = app.emit("query-error", message.clone());
                return Err(message);
            }
            None => break,
        }
    }

    let _ = app.emit("query-done", ());
    Ok(())
}
