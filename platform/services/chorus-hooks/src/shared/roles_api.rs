//! #4028/#4077 — role rows for the pulse, from chorus-api's one derivation.

/// #4028/#4077 — the role rows come from the ONE derivation, chorus-api's
/// /api/chorus/context/roles (state from the streams, card from the board).
/// The declared files this used to compose are gone; nothing local is read.
/// Unreachable → every row says so ("unmeasured", source "streams-unreachable")
/// rather than "unknown" — the pulse must not look like a stale declaration.
pub fn roles_endpoint() -> String {
    std::env::var("CHORUS_ROLES_URL")
        .unwrap_or_else(|_| "http://localhost:3340/api/chorus/context/roles".to_string())
}

pub fn roles_from_api(url: &str) -> serde_json::Value {
    let fetched: Result<serde_json::Value, String> =
        match ureq::get(url).timeout(std::time::Duration::from_millis(2500)).call() {
            Ok(resp) => resp.into_json().map_err(|e| format!("non-JSON: {e}")),
            Err(e) => Err(format!("unreachable: {e}")),
        };
    let mut roles = serde_json::Map::new();
    match fetched {
        Ok(body) => {
            let rows = body.pointer("/data/roles").and_then(|v| v.as_array()).cloned().unwrap_or_default();
            for row in rows {
                if let Some(name) = row.get("role").and_then(|v| v.as_str()) {
                    roles.insert(name.to_string(), row.clone());
                }
            }
            if roles.is_empty() {
                for role in &["wren", "silas", "kade"] {
                    roles.insert(role.to_string(), serde_json::json!({
                        "role": role, "state": "unmeasured", "source": "streams-empty", "detail": "roles endpoint returned no rows"
                    }));
                }
            }
        }
        Err(why) => {
            for role in &["wren", "silas", "kade"] {
                roles.insert(role.to_string(), serde_json::json!({
                    "role": role, "state": "unmeasured", "source": "streams-unreachable", "detail": why
                }));
            }
        }
    }
    serde_json::Value::Object(roles)
}

