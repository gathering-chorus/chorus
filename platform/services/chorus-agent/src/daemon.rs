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
/// #4424 — hand the socket to a shared group (directory 0750, socket 0660).
fn share_with_group(dir: &std::path::Path, socket: &std::path::Path, group: &str) -> Result<()> {
    let name = std::ffi::CString::new(group).map_err(|_| "invalid socket_group")?;
    let entry = unsafe { libc::getgrnam(name.as_ptr()) };
    if entry.is_null() {
        return Err(format!("socket_group {group} does not exist"));
    }
    let gid = unsafe { (*entry).gr_gid };
    for (path, mode) in [(dir, 0o750), (socket, 0o660)] {
        std::os::unix::fs::chown(path, None, Some(gid)).map_err(|e| format!("chown {}: {e}", path.display()))?;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).map_err(|e| e.to_string())?;
    }
    Ok(())
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
    if let Some(group) = &config.socket_group {
        share_with_group(parent, &socket, group)?;
    }
    let api = std::env::var("CHORUS_API_URL").unwrap_or_else(|_| "http://127.0.0.1:3340".into());
    let app = server::Supervisor::new(
        config,
        store::Store::open(config::root())?,
        format!("{}/api/chorus/identity/verify", api.trim_end_matches('/')),
    )?;
    eprintln!("chorus-agentd ready: {}", socket.display());
    let shutdown = app.clone();
    axum::serve(listener, server::make_service(app))
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
