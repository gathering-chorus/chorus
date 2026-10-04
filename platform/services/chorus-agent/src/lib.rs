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
