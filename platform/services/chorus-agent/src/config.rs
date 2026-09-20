use crate::{contract::*, Result};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub version: u32,
    pub profiles: BTreeMap<String, Profile>,
    #[serde(default)]
    pub roles: BTreeMap<String, String>,
    #[serde(default)]
    pub role_workspaces: BTreeMap<String, String>,
    pub worktree_base: Option<String>,
    #[serde(default = "default_concurrency")]
    pub max_concurrent_jobs: usize,
}
fn default_concurrency() -> usize {
    3
}
pub fn root() -> PathBuf {
    std::env::var_os("CHORUS_AGENT_STATE_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".chorus")
        })
}
pub fn socket() -> PathBuf {
    std::env::var_os("CHORUS_AGENT_SOCKET")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("run/chorus-agent.sock"))
}
pub fn config_path() -> PathBuf {
    std::env::var_os("CHORUS_AGENT_CONFIG")
        .map(PathBuf::from)
        .unwrap_or_else(|| root().join("agent-profiles.json"))
}
pub fn read(path: &Path) -> Result<Config> {
    let bytes = std::fs::read(path).map_err(|e| format!("read operator profiles: {e}"))?;
    let c: Config =
        serde_json::from_slice(&bytes).map_err(|e| format!("invalid operator profiles: {e}"))?;
    validate(&c)?;
    Ok(c)
}
pub fn validate(c: &Config) -> Result<()> {
    if c.version != VERSION {
        return Err("unsupported profile schema major".into());
    }
    if c.max_concurrent_jobs == 0 || c.max_concurrent_jobs > 32 {
        return Err("max_concurrent_jobs must be 1..32".into());
    }
    for p in c.profiles.values() {
        validate_profile(p)?;
    }
    for p in c.roles.values() {
        if !c.profiles.contains_key(p) {
            return Err(format!("role references missing profile {p}"));
        }
    }
    Ok(())
}
pub fn validate_profile(p: &Profile) -> Result<()> {
    if p.timeout_secs == 0 || p.timeout_secs > 86400 {
        return Err("timeout_secs must be 1..86400".into());
    }
    if p.enforcement == Enforcement::Verified && p.conformance.is_none() {
        return Err(
            "verified enrollment requires an operator-installed conformance attestation".into(),
        );
    }
    if let Some(c) = &p.conformance {
        if c.report_sha256.len() != 64 || !c.report_sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("invalid conformance report hash".into());
        }
        let report = std::fs::read(&c.report_path).map_err(|_| "conformance report unavailable")?;
        if crate::digest(&report) != c.report_sha256.to_lowercase() {
            return Err("conformance report hash mismatch".into());
        }
        let report: serde_json::Value =
            serde_json::from_slice(&report).map_err(|_| "invalid conformance report")?;
        if report["passed"] != true
            || report["runtime_version"] != c.runtime_version
            || report["adapter_version"] != c.adapter_version
            || report["capabilities"] != serde_json::to_value(&c.capabilities).unwrap()
        {
            return Err("conformance report does not match attestation".into());
        }
    }
    for key in p.adapter_config.keys() {
        if [
            "api_key",
            "token",
            "password",
            "secret",
            "env",
            "environment",
            "env_refs",
        ]
        .contains(&key.as_str())
        {
            return Err("adapter settings must use generated config and credential references, not embedded credentials/identity".into());
        }
    }
    if let Some(provider) = &p.provider {
        if !["anthropic", "openai-chat", "openai-responses"].contains(&provider.protocol.as_str()) {
            return Err(
                "provider protocol must be anthropic, openai-chat, or openai-responses".into(),
            );
        }
        if let Some(key) = &provider.api_key_env {
            if key.is_empty() || !key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
                return Err(
                    "provider credential must reference an environment variable name".into(),
                );
            }
        }
        if let Some(url) = &provider.base_url {
            let url = reqwest::Url::parse(url).map_err(|_| "invalid provider base URL")?;
            if !url.username().is_empty()
                || url.password().is_some()
                || url.query().is_some()
                || url.fragment().is_some()
            {
                return Err("credentials/query strings are forbidden in provider base URLs".into());
            }
            if url.scheme() != "https"
                && !(url.scheme() == "http"
                    && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]")))
            {
                return Err("provider base URL requires HTTPS (except loopback)".into());
            }
        }
    }
    Ok(())
}
pub fn hash(profile: &Profile) -> String {
    crate::digest(&serde_json::to_vec(profile).unwrap())
}
