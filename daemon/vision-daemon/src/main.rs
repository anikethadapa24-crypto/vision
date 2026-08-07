#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    eprintln!("vision-daemon: starting, listening on {}", pipe_description());

    vision_daemon::serve(shutdown_signal()).await?;

    eprintln!("vision-daemon: shut down cleanly");
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}

#[cfg(windows)]
fn pipe_description() -> &'static str {
    vision_daemon::transport::windows::PIPE_NAME
}

#[cfg(unix)]
fn pipe_description() -> &'static str {
    "<unix domain socket — not yet implemented>"
}
