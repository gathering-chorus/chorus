pub mod allowed_tools;
pub mod config;
pub mod contract;
pub mod execution;
pub mod server;
pub mod store;

pub type Result<T> = std::result::Result<T, String>;

pub fn now() -> String {
    chrono::Utc::now().to_rfc3339()
}
pub fn id() -> String {
    uuid::Uuid::now_v7().to_string()
}
pub fn digest(value: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(value))
}

/// #4424 — spine events go through main's chorus-log writer (role, principal
/// and the rest as key=value), never appended raw to a file of our own.
/// CHORUS_LOG_BIN names the writer; default $CHORUS_HOME/platform/scripts/chorus-log.
pub fn spine(event: &str, role: &str, fields: &[(&str, String)]) -> Result<()> {
    let bin = std::env::var("CHORUS_LOG_BIN").unwrap_or_else(|_| {
        let home = std::env::var("CHORUS_HOME").unwrap_or_else(|_| {
            format!("{}/CascadeProjects/chorus", std::env::var("HOME").unwrap_or_default())
        });
        format!("{home}/platform/scripts/chorus-log")
    });
    let mut args = vec![event.to_string(), role.to_string()];
    args.extend(fields.iter().filter(|(_, v)| !v.is_empty()).map(|(k, v)| format!("{k}={v}")));
    let status = std::process::Command::new(&bin)
        .args(&args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("chorus-log unavailable ({bin}): {e}"))?;
    if !status.success() {
        return Err(format!("chorus-log refused {event}"));
    }
    Ok(())
}
