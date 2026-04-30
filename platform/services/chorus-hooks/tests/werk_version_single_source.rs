//! #2311 rescope AC#5 — Single source of truth for "Werk v" render.
//!
//! Jenga rule: every "Werk v<N>" rendered anywhere in the system resolves
//! from PROTOCOL_VERSION, not from manifest.json's build counter. The
//! manifest's "version" key is renamed to "_build" so nothing can bind
//! to the attractor name and regress.

use std::fs;

/// #2614: returns true (and prints a skip line) when RUN_INTEGRATION is unset.
fn skip_unless_integration(reason: &str) -> bool {
    if std::env::var("RUN_INTEGRATION").is_err() {
        eprintln!("SKIP: axis-4 — {reason} (set RUN_INTEGRATION=1 to run)");
        return true;
    }
    false
}

const PROTOCOL_VERSION_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../designing/claudemd/PROTOCOL_VERSION"
);
const MANIFEST_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../designing/claudemd/manifest.json"
);

const SHIM: &str = env!("CARGO_BIN_EXE_chorus-hook-shim");

/// manifest.json must not carry a top-level "version" key — the attractor
/// that made two "Werk v" render paths possible. Build counter, if kept,
/// lives under "_build".
#[test]
fn manifest_has_no_version_key() {
    let content = fs::read_to_string(MANIFEST_PATH).expect("manifest.json readable");
    let v: serde_json::Value = serde_json::from_str(&content)
        .expect("manifest.json parseable");

    assert!(
        v.get("version").is_none(),
        "manifest.json top-level 'version' key is the jenga attractor — \
         rename to '_build' (or remove). Found: {:?}",
        v.get("version")
    );
}

/// context_cache rendering of Werk v<N> comes from PROTOCOL_VERSION, not
/// manifest.json. Run session-start (which runs context_cache) for silas
/// and verify the output contains the PROTOCOL_VERSION value.
#[test]
fn context_cache_renders_protocol_version_not_manifest_build() {
    if skip_unless_integration("removes/writes /tmp/session-context-silas.md, races live silas session") { return; }
    // Force cache refresh by removing existing file
    let _ = fs::remove_file("/tmp/session-context-silas.md");

    let protocol_version = fs::read_to_string(PROTOCOL_VERSION_PATH)
        .expect("PROTOCOL_VERSION readable")
        .trim()
        .to_string();

    let _ = std::process::Command::new(SHIM)
        .args(["session-start", "silas"])
        .output()
        .expect("session-start runs");

    let ctx = fs::read_to_string("/tmp/session-context-silas.md")
        .expect("session-context-silas.md written");

    let expected_render = format!("Werk v{}", protocol_version);
    assert!(
        ctx.contains(&expected_render),
        "session-context should render '{}' sourced from PROTOCOL_VERSION. \
         Head: {:?}",
        expected_render,
        &ctx[..ctx.len().min(300)]
    );

    // Negative: must NOT render the manifest build counter as a Werk version.
    // Read whatever _build is and prove it isn't in the Werk v render.
    let manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(MANIFEST_PATH).unwrap()
    ).unwrap();
    if let Some(build_str) = manifest.get("_build").and_then(|v| v.as_str()) {
        // Parse only if non-trivial (e.g., 217)
        if build_str != protocol_version {
            let wrong_render = format!("Werk v{}", build_str);
            assert!(
                !ctx.contains(&wrong_render),
                "session-context must not render '{}' — build counter must \
                 not leak into Werk version render path",
                wrong_render
            );
        }
    }
}
