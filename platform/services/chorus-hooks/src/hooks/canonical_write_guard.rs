//! Canonical write guard (#2735).
//!
//! Refuses Edit/Write to canonical (`$CHORUS_HOME`) and to other roles' werks
//! when the role's own werk is initialized. Edits during a session belong in
//! the role's worktree at `/chorus-werk/<role>/`, not in the shared canonical
//! tree. This is the structural close to the shared-canonical anti-pattern
//! Jeff named 2026-05-05.
//!
//! Silent when role env isn't set — bootstrap, migration, and generic shell
//! contexts must not be blocked. Once env is in place, every role-session
//! edit lands in the role's werk.
//!
//! Allowed surfaces (not affected by this guard):
//! - Anything outside `$CHORUS_HOME` (including `$<ROLE>_WERK`)
//! - `/tmp/...` and `/var/folders/...` (sketch surfaces; classifier-exempt)
//! - All `Read` operations (read of canonical is fine — that's what
//!   role-state lives in until sync)

use crate::types::{permission_deny_json, HookInput, HookResponse, Role};

/// Default canonical path. Project layout convention: chorus checkout is at
/// /Users/jeffbridwell/CascadeProjects/chorus. Override with CHORUS_HOME env
/// for tests / non-default installs.
const DEFAULT_CHORUS_HOME: &str = "/Users/jeffbridwell/CascadeProjects/chorus";
const DEFAULT_CHORUS_WERK_BASE: &str = "/Users/jeffbridwell/CascadeProjects/chorus-werk";

pub fn check(input: &HookInput) -> HookResponse {
    // #2908: CHORUS_WERK_ENABLE flag retired 2026-05-11. All three roles
    // have the flag set in their settings.json as of #2735's per-role rollout,
    // so the flag's per-role-opt-in purpose is fulfilled. Bug-class receipt:
    // Wren's 2026-05-11 edit to canonical (directing/clearing/public/index.html)
    // wasn't refused because the flag wasn't loaded into the hook's env at
    // evaluation time — three-layer attention tax followed. With the flag
    // retired, the guard fires whenever the role is determinable. Bootstrap /
    // migration / generic-shell paths are still allowed via the role-detection
    // bail-out below (no role → no refusal).

    let tool = input.tool_name_str();
    if tool != "Write" && tool != "Edit" && tool != "MultiEdit" {
        return HookResponse::allow();
    }

    let file_path = input.get_tool_input_str("file_path");
    if file_path.is_empty() {
        return HookResponse::allow();
    }

    // Role: prefer input.role() (shim injects DEPLOY_ROLE into JSON since env
    // doesn't cross the daemon socket boundary). Fall back to CHORUS_ROLE env
    // for the in-shim case where the hook runs in the same process as the
    // role's session.
    let role: String = match input.role() {
        Role::Kade => "kade".to_string(),
        Role::Wren => "wren".to_string(),
        Role::Silas => "silas".to_string(),
        _ => match std::env::var("CHORUS_ROLE") {
            Ok(r) if r == "kade" || r == "wren" || r == "silas" => r,
            _ => return HookResponse::allow(), // bootstrap / migration / generic shell
        },
    };

    let canonical = std::env::var("CHORUS_HOME")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_CHORUS_HOME.to_string());

    let werk_base = std::env::var("CHORUS_WERK_BASE")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_CHORUS_WERK_BASE.to_string());

    let werk_var = format!("{}_WERK", role.to_uppercase());
    let own_werk = std::env::var(&werk_var)
        .unwrap_or_else(|_| format!("{}/{}", werk_base, role));

    // Sketch / temp surfaces — always allowed (matches file_classification.rs exemption)
    if file_path.starts_with("/tmp/") || file_path.starts_with("/var/folders/") {
        return HookResponse::allow();
    }

    // Cross-role check first: write to another role's werk is always wrong,
    // regardless of where canonical points.
    if file_path.starts_with(&format!("{}/", werk_base)) {
        // Inside CHORUS_WERK_BASE — figure out which role's slot it lands in.
        let rest = &file_path[werk_base.len() + 1..];
        let other_role = rest.split('/').next().unwrap_or("");
        if !other_role.is_empty() && other_role != role {
            let msg = format!(
                "BLOCKED: cross-role write — {role} cannot write to {other_role}'s werk at {file_path}. \
                 Each role only writes to their own werk (${werk_var}={own_werk}). \
                 If you need to coordinate, file a brief or open a PR."
            );
            return HookResponse::deny(&permission_deny_json(&msg));
        }
        // Own werk — allow.
        return HookResponse::allow();
    }

    // Canonical check: if writing under $CHORUS_HOME, refuse with a redirect
    // suggestion pointing at the same relative path under $<ROLE>_WERK.
    if file_path.starts_with(&format!("{}/", canonical)) || file_path == canonical {
        let rel = file_path
            .strip_prefix(&format!("{}/", canonical))
            .unwrap_or("");
        let suggested = if !own_werk.is_empty() {
            format!("{}/{}", own_werk, rel)
        } else {
            format!("(role werk not initialized; run 'chorus-werk init {role}')")
        };
        let msg = format!(
            "BLOCKED: canonical is read-only during sessions ({canonical}). \
             Rewrite this path under your werk and retry: {suggested}. \
             Canonical refreshes only at lock-guarded sync moments — edits land in ${werk_var}."
        );
        return HookResponse::deny(&permission_deny_json(&msg));
    }

    HookResponse::allow()
}
