use std::sync::Arc;

use vision_core::Engine;
use vision_daemon::single_instance;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let data_dir = vision_core::paths::base_data_dir();
    let _instance_lock = match single_instance::acquire(&data_dir) {
        Ok(lock) => lock,
        Err(single_instance::AcquireError::AlreadyRunning) => {
            eprintln!(
                "vision-daemon: {}",
                single_instance::AcquireError::AlreadyRunning
            );
            std::process::exit(1);
        }
        Err(err) => return Err(Box::new(err) as Box<dyn std::error::Error>),
    };

    let engine = Arc::new(Engine::open(&data_dir)?);
    let _watcher = vision_daemon::watcher::spawn(engine.clone());

    eprintln!(
        "vision-daemon: starting, listening on {}",
        pipe_description()
    );

    vision_daemon::serve(engine, shutdown_signal()).await?;

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
