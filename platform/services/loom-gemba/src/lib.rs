//! loom-gemba — the observation verb (card #3319). A loom SENSE: looking at a
//! role working, the way /lc looks at a Chrome tab.
//!
//! Built ON pulse-gather (the short-term-memory primitive), not merged into it
//! (Jeff 2026-06-10: pulse-gather <> gemba). This crate LINKS pulse-gather's
//! mechanics — stream read, cursor exactness, cold-start windowing — and adds
//! the observation semantics the skill layer proved too skippable to hold:
//!
//!   1. Invoke = declare. Running the verb IS `role-state <role> observing
//!      gemba=<target>` — there is no separate step a model can skip (Wren
//!      skipped it live, 2026-06-10 08:39, while Jeff's pulse said idle).
//!   2. Banner always. The FIRST line of every poll names watcher → target,
//!      cursor, count — including empty polls. In focus mode Jeff sees only
//!      what the agent pastes; an invisible quiet poll is an invisible watch.
//!   3. Own spine signal: `gemba.observed` (observation is its own event,
//!      distinct from the memory-primitive's `pulse.gathered`).
//!   4. Own cursor namespace (`ops/loom-gemba/`): a /gemba poll never moves a
//!      raw pulse-gather consumer's cursor, and vice versa.
//!
//! Decay (AC4) lives on the READ side — role-state's sweep demotes a stale
//! `observing` (chorus-hooks role_state.rs, same card) — because a stopped
//! watcher, by definition, isn't running this verb anymore.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use pulse_gather::{
    effective_cursor, emit_spine, gather_since, observations_path, render_observation,
    Observation, R,
};

/// Cursor namespace — separate from pulse-gather's so the sense and the memory
/// primitive never fight over "since when".
pub const CURSOR_NS: &str = "loom-gemba";

/// Same cold-start window as pulse-gather (#3274): show NOW, not the backlog.
const COLD_START_WINDOW: usize = 10;

// --- pure core (unit-tested) ---

/// The visibility contract: one line, who→whom, since when, how many.
/// Missing stream is "rebuilding", never quiet (#3205 reboot-blindness rule).
pub fn banner(role: &str, target: &str, cursor: &str, count: usize, stream_exists: bool) -> String {
    if !stream_exists {
        return format!(
            "[gemba] {}→{} | observation stream unavailable — rebuilding (not idle)",
            role, target
        );
    }
    if cursor.is_empty() {
        return format!("[gemba] {}→{} | cold start | last {} turns", role, target, count);
    }
    if count == 0 {
        return format!("[gemba] {}→{} | since {} | 0 new turns (quiet)", role, target, cursor);
    }
    format!("[gemba] {}→{} | since {} | {} new turns", role, target, cursor, count)
}

/// Whole-poll rendering: banner FIRST, then pulse-gather's turn lines. Never
/// returns empty — the empty poll is exactly when the watch must stay visible.
pub fn render_poll(
    role: &str,
    target: &str,
    cursor: &str,
    stream_exists: bool,
    fresh: &[Observation],
) -> String {
    let head = banner(role, target, cursor, fresh.len(), stream_exists);
    if !stream_exists || fresh.is_empty() {
        return head;
    }
    let body: Vec<String> = fresh.iter().map(render_observation).collect();
    format!("{}\n{}", head, body.join("\n"))
}

/// Invoke = declare: the exact argv handed to `role-state`. Card/type are
/// refused by role-state (#2629), so the args are only role/state/gemba.
pub fn role_state_args(role: &str, target: &str) -> Vec<String> {
    vec![role.to_string(), "observing".to_string(), format!("gemba={}", target)]
}

/// The `gemba.observed` spine extras — mirrors pulse-gather's status taxonomy
/// (fresh / quiet / rebuilding) so dashboards read both events the same way.
pub fn spine_extras(target: &str, count: usize, stream_exists: bool) -> Vec<(String, String)> {
    let status = if !stream_exists { "rebuilding" } else if count == 0 { "quiet" } else { "fresh" };
    vec![
        ("target".to_string(), target.to_string()),
        ("count".to_string(), count.to_string()),
        ("status".to_string(), status.to_string()),
    ]
}

// --- side-effecting real entry (inputs from env + filesystem) ---

/// The per-(observer,target) gemba cursor — durable, own namespace.
fn cursor_path(home: &Path, role: &str, target: &str) -> PathBuf {
    home.join(format!("ops/{}/{}-watching-{}.cursor", CURSOR_NS, role, target))
}

/// Declare `observing gemba=<target>` via the canonical role-state script —
/// best-effort like emit_spine: a missing script never blocks the poll.
fn declare_observing(home: &Path, role: &str, target: &str) {
    let script = home.join("platform/scripts/role-state");
    let Some(script_s) = script.to_str().map(String::from) else { return };
    let args = role_state_args(role, target);
    let mut argv: Vec<&str> = vec![script_s.as_str()];
    argv.extend(args.iter().map(|s| s.as_str()));
    let _ = Command::new("bash").args(&argv).output();
}

/// The whole verb:
///   loom-gemba <target-role>   (observer role from $DEPLOY_ROLE/$CHORUS_ROLE)
/// Declares observing, gathers via the pulse-gather core (own cursor namespace),
/// advances the cursor, emits `gemba.observed`, and returns banner + turns.
pub fn run() -> R<String> {
    let target = env::args()
        .nth(1)
        .ok_or_else(|| "usage: loom-gemba <target-role>".to_string())?;
    let role = env::var("DEPLOY_ROLE")
        .or_else(|_| env::var("CHORUS_ROLE"))
        .unwrap_or_else(|_| "unknown".to_string());
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);

    // 1. Invoke = declare (AC2). Before the read, so even an empty poll counts
    //    as attention — and each re-poll refreshes the observing ts the decay
    //    sweep (AC4) measures against.
    declare_observing(&home, &role, &target);

    // 2. Gather — pulse-gather mechanics, loom-gemba cursor namespace.
    let spath = observations_path(&target);
    let stream_exists = spath.exists();
    let stream = fs::read_to_string(&spath).unwrap_or_default();
    let cpath = cursor_path(&home, &role, &target);
    let stored = fs::read_to_string(&cpath).unwrap_or_default().trim().to_string();
    let effective = effective_cursor(&stream, &stored, COLD_START_WINDOW);
    let result = gather_since(&stream, &effective);

    if result.cursor != stored {
        if let Some(d) = cpath.parent() {
            let _ = fs::create_dir_all(d);
        }
        let _ = fs::write(&cpath, &result.cursor);
    }

    // 3. Observation is its own spine signal (AC5).
    let extras = spine_extras(&target, result.fresh.len(), stream_exists);
    let extras_ref: Vec<(&str, &str)> =
        extras.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    emit_spine(&home, "gemba.observed", &role, &extras_ref);

    // 4. Banner + turns — `stored` (not advanced) is the honest "since".
    Ok(render_poll(&role, &target, &stored, stream_exists, &result.fresh))
}
