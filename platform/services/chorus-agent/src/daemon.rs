use chorus_agent::{config, server, store, Result};
use std::{
    fs::OpenOptions,
    os::{
        fd::AsRawFd,
        unix::fs::{OpenOptionsExt, PermissionsExt},
    },
};

#[tokio::main]
async fn main() {
    if let Err(error) = run().await {
        eprintln!("chorus-agentd: {error}");
        std::process::exit(1);
    }
}
async fn run() -> Result<()> {
    let config = config::read(&config::config_path())?;
    let socket = config::socket();
    let parent = socket.parent().ok_or("invalid socket path")?;
    store::private_dir(parent)?;
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .mode(0o600)
        .open(socket.with_extension("lock"))
        .map_err(|e| e.to_string())?;
    if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err("another supervisor owns the socket".into());
    }
    if socket.exists() {
        std::fs::remove_file(&socket).map_err(|e| e.to_string())?;
    }
    let listener = tokio::net::UnixListener::bind(&socket).map_err(|e| e.to_string())?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))
        .map_err(|e| e.to_string())?;
    let api = std::env::var("CHORUS_API_URL").unwrap_or_else(|_| "http://127.0.0.1:3340".into());
    let app = server::Supervisor::new(
        config,
        store::Store::open(config::root())?,
        format!("{}/api/chorus/identity/verify", api.trim_end_matches('/')),
    )?;
    eprintln!("chorus-agentd ready: {}", socket.display());
    let shutdown = app.clone();
    axum::serve(listener, server::router(app))
        .with_graceful_shutdown(async move {
            let mut terminate =
                tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                    .expect("SIGTERM handler");
            tokio::select! {_=tokio::signal::ctrl_c()=>{},_=terminate.recv()=>{}}
            shutdown.shutdown().await;
        })
        .await
        .map_err(|e| e.to_string())
}
