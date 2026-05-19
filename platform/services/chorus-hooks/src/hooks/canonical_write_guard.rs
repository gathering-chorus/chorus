//! Canonical write guard (#2735, #2913).
//!
//! Refuses Edit/Write to canonical (`$CHORUS_HOME`) and to other roles' werks.
//! Edits during a session belong in the role's worktree, not in the shared
//! canonical tree. This is the structural close to the shared-canonical
//! anti-pattern Jeff named 2026-05-05.
//!
//! #2913: werks are ephemeral per-card — `/chorus-werk/<role>-<card>/`, not
//! the old persistent `/chorus-werk/<role>/`. The cross-role check parses the
//! owning role as the segment before the first `-` in the werk-slot name.
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
    // #2913: no persistent-per-role fallback. Under the ephemeral model the
    // role's werk is per-card (chorus-werk/<role>-<card>/), so there is no
    // single stable path to default to. If <ROLE>_WERK isn't set, leave
    // own_werk empty — the suggestion messages below handle that case
    // without fabricating a path that doesn't exist.
    let own_werk = std::env::var(&werk_var).unwrap_or_default();

    // Sketch / temp surfaces — always allowed (matches file_classification.rs exemption)
    if file_path.starts_with("/tmp/") || file_path.starts_with("/var/folders/") {
        return HookResponse::allow();
    }

    // Cross-role check first: write to another role's werk is always wrong,
    // regardless of where canonical points.
    if file_path.starts_with(&format!("{}/", werk_base)) {
        // Inside CHORUS_WERK_BASE — figure out which role's slot it lands in.
        // #2913: werk slots are <role>-<card> (ephemeral per-card). The owning
        // role is the segment before the first '-'. A bare <role> slot from
        // the pre-#2913 persistent model still parses — no '-', whole segment
        // is the role — so this is correct during a heterogeneous migration.
        let rest = &file_path[werk_base.len() + 1..];
        let werk_slot = rest.split('/').next().unwrap_or("");
        let other_role = werk_slot.split('-').next().unwrap_or("");
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

    // #3003: role-state allowlist for no-WIP /reboot.
    //
    // /reboot with no WIP card writes roles/<role>/{next-session.md,
    // next-session.md.consumed, current-work.md, tech-debt.md, briefs/*} to
    // canonical. Pre-#3003 the guard refused with a fabricated redirect path
    // — #2913 retired the persistent per-role werk, so there is nothing to
    // redirect to. Allow these writes when <ROLE>_WERK is empty (no WIP card,
    // or ambiguous because the role has 2+ cards in flight — chorus-env-setup
    // leaves the var unset in both cases).
    //
    // When <ROLE>_WERK IS set (single WIP card), this allowlist is skipped —
    // falls through to the existing redirect, which correctly targets the
    // werk's roles/<role>/ mirror. Mid-card /reboot writes land in the werk,
    // preserving the existing per-card-werk behavior (AC3 regression check).
    let own_role_prefix = format!("{}/roles/{}/", canonical, role);
    if own_werk.is_empty() && file_path.starts_with(&own_role_prefix) {
        let rel = &file_path[own_role_prefix.len()..];
        if is_role_state_allowlisted(rel) {
            return HookResponse::allow();
        }
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
            // #2913: ephemeral model — no persistent per-role werk to point
            // at. The werk for the active card is chorus-werk/<role>-<card>/,
            // created by `chorus-werk add` (via /pull). If ${werk_var} isn't
            // set, the role hasn't pulled a card this session.
            format!("(no active werk — pull a card first; its werk is {werk_base}/{role}-<card>/{rel})")
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

/// #3003: role-state files /reboot writes to canonical when no WIP card is in
/// flight. Filename-strict — adding new files here is a deliberate decision,
/// not a side effect. `rel` is the path inside `roles/<role>/`.
fn is_role_state_allowlisted(rel: &str) -> bool {
    matches!(
        rel,
        "next-session.md"
            | "next-session.md.consumed"
            | "current-work.md"
            | "tech-debt.md"
            | "stories.md"
            | "decisions.md"
    ) || rel.starts_with("briefs/")
}
