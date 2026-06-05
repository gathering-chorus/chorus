//! pulse-gather — the short-term-memory twin of the per-prompt gather (card #3205).
//!
//! Self-contained: std only; reads pulse.* (`~/.chorus/pulse-latest.json`) + the role
//! observation stream (`/tmp/claude-team-scan/<role>-observations.jsonl`) and shells
//! `chorus-log` for the spine. No dependency on any other chorus code. Mirrors the
//! werk-verb blueprint (werk-pull #3045, werk-merge #3175): zero-dep, typed R<T>,
//! a pure testable core (all inputs explicit) wrapped by a thin real entry.
//!
//! THE ONE PATH: gemba and pair both collapse onto this verb. It does not diff
//! snapshot-to-snapshot (the gemba-tick staleness root) — it reads the live
//! observation stream and emits every turn newer than a timestamp cursor. Keyed on
//! `ts`, not line index, so it survives the observer's 200-line rotation and can
//! never report "no change since 16:56" while a 17:16 turn exists in the stream.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

pub type R<T> = Result<T, String>;

#[derive(Debug, Clone, PartialEq)]
pub struct Observation {
    pub ts: String,
    pub role: String,
    pub tool: String,
    pub action: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RoleView {
    pub state: String,
    pub card: String,
}

pub struct GatherResult {
    pub fresh: Vec<Observation>,
    pub cursor: String,
}

// --- pure core (unit-tested) ---

/// Minimal whitespace-tolerant extractor for a JSON string field: finds `"key"`,
/// then the next quoted value. Zero-dep. Tolerant of a space after the colon (real
/// pretty-printed JSONL), the same shape werk-pull's json_str_field guards against.
fn json_str_field(json: &str, key: &str) -> Option<String> {
    let i = json.find(&format!("\"{}\"", key))?;
    let after_key = &json[i + key.len() + 2..];
    let colon = after_key.find(':')?;
    let after_colon = &after_key[colon + 1..];
    let q1 = after_colon.find('"')?;
    let val = &after_colon[q1 + 1..];
    let q2 = val.find('"')?;
    Some(val[..q2].to_string())
}

/// Parse one JSONL observation line into an Observation. `ts` is required (a line
/// without it is not an observation); the rest default to empty so a partial line
/// still narrates rather than being dropped.
pub fn parse_observation(line: &str) -> Option<Observation> {
    let line = line.trim();
    if line.is_empty() || !line.starts_with('{') {
        return None;
    }
    let ts = json_str_field(line, "ts")?;
    Some(Observation {
        ts,
        role: json_str_field(line, "role").unwrap_or_default(),
        tool: json_str_field(line, "tool").unwrap_or_default(),
        action: json_str_field(line, "action").unwrap_or_default(),
        digest: json_str_field(line, "digest").unwrap_or_default(),
    })
}

/// THE no-stale core. Return every observation strictly newer than `cursor`, in file
/// order, plus the advanced cursor (max ts seen, or the unchanged cursor if nothing
/// is fresh). The observer writes ISO8601 with a fixed local offset, so lexicographic
/// order == chronological order; comparison is on the ts string. Keying on ts (not a
/// line index) is what survives the observer's 200-line rotation: a re-poll with the
/// advanced cursor never replays, and a turn newer than the cursor is never lost.
pub fn gather_since(observations_text: &str, cursor: &str) -> GatherResult {
    let mut fresh = Vec::new();
    let mut max_ts = cursor.to_string();
    for line in observations_text.lines() {
        if let Some(o) = parse_observation(line) {
            if o.ts.as_str() > cursor {
                if o.ts > max_ts {
                    max_ts = o.ts.clone();
                }
                fresh.push(o);
            }
        }
    }
    GatherResult { fresh, cursor: max_ts }
}

/// Extract a role's view (state + card) from the pulse object's `roles.<role>` slice.
/// Zero-dep: scopes to the role's object substring, then reads its fields.
pub fn role_view_from_pulse(pulse_json: &str, role: &str) -> RoleView {
    let scoped = pulse_json
        .find(&format!("\"{}\"", role))
        .map(|i| &pulse_json[i..])
        .unwrap_or(pulse_json);
    RoleView {
        state: json_str_field(scoped, "state").unwrap_or_default(),
        card: json_str_field(scoped, "card").unwrap_or_default(),
    }
}

/// Narrate the change between two role views — one line per changed field, empty when
/// nothing changed. The state slice of a gemba/pair delta.
pub fn role_delta(prev: &RoleView, cur: &RoleView) -> Vec<String> {
    let mut out = Vec::new();
    if prev.state != cur.state {
        out.push(format!("state {} → {}", prev.state, cur.state));
    }
    if prev.card != cur.card {
        let from = if prev.card.is_empty() { "—" } else { prev.card.as_str() };
        let to = if cur.card.is_empty() { "—" } else { cur.card.as_str() };
        out.push(format!("card {} → {}", from, to));
    }
    out
}

/// Terse, terminal-friendly narration of one observation. tool + digest are the signal;
/// the HH:MM:SS is pulled from the ISO ts when present.
pub fn render_observation(o: &Observation) -> String {
    let clock = o.ts.split('T').nth(1).map(|t| &t[..t.len().min(8)]).unwrap_or(&o.ts);
    format!("{} {} — {}", clock, o.tool, o.digest)
}

/// The spine event contract (mirrors werk-pull's spine_args): args handed to
/// `chorus-log` so a gather is queryable in Loki — event first, role second, then
/// key=value extras.
pub fn spine_args(event: &str, role: &str, extras: &[(&str, &str)]) -> Vec<String> {
    let mut v = vec![event.to_string(), role.to_string()];
    for (k, val) in extras {
        v.push(format!("{}={}", k, val));
    }
    v
}

// --- side-effecting real entry (inputs from env + filesystem) ---

/// Emit one event to the ONE spine via the canonical `chorus-log` subprocess —
/// best-effort, never blocks the verb. Mirrors werk-pull's emit_spine.
fn emit_spine(home: &Path, event: &str, role: &str, extras: &[(&str, &str)]) {
    let log = home.join("platform/scripts/chorus-log");
    let log_s = match log.to_str() {
        Some(s) => s.to_string(),
        None => return,
    };
    let args = spine_args(event, role, extras);
    let mut argv: Vec<&str> = vec![log_s.as_str()];
    argv.extend(args.iter().map(|s| s.as_str()));
    let _ = Command::new("bash").args(&argv).output();
}

/// Resolve the observation stream for a role. The observer writes here today.
fn observations_path(target: &str) -> PathBuf {
    PathBuf::from(format!("/tmp/claude-team-scan/{}-observations.jsonl", target))
}

/// The per-(observer,target) cursor — the carrier that makes re-polls non-replaying.
/// Lives off the durable home so it survives a /tmp wipe (#3202 lesson).
fn cursor_path(home: &Path, role: &str, target: &str) -> PathBuf {
    home.join(format!("ops/pulse-gather/{}-watching-{}.cursor", role, target))
}

/// The whole verb, real inputs from env + fs:
///   pulse-gather <target-role>   (observer role from $DEPLOY_ROLE/$CHORUS_ROLE)
/// Reads the target's observation stream + pulse.*, emits every turn newer than the
/// stored cursor, advances the cursor, narrates, and records a `pulse.gathered` spine
/// event. Idempotent across polls: an unchanged stream yields no deltas and no churn.
pub fn run() -> R<String> {
    let target = env::args()
        .nth(1)
        .ok_or_else(|| "usage: pulse-gather <target-role>".to_string())?;
    let role = env::var("DEPLOY_ROLE")
        .or_else(|_| env::var("CHORUS_ROLE"))
        .unwrap_or_else(|_| "unknown".to_string());
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);

    let stream = fs::read_to_string(observations_path(&target)).unwrap_or_default();
    let cpath = cursor_path(&home, &role, &target);
    let cursor = fs::read_to_string(&cpath).unwrap_or_default().trim().to_string();

    let result = gather_since(&stream, &cursor);

    // advance the cursor (best-effort; durable so re-polls don't replay across a /tmp wipe)
    if result.cursor != cursor {
        if let Some(d) = cpath.parent() {
            let _ = fs::create_dir_all(d);
        }
        let _ = fs::write(&cpath, &result.cursor);
    }

    let count = result.fresh.len();
    emit_spine(
        &home,
        "pulse.gathered",
        &role,
        &[("target", target.as_str()), ("count", &count.to_string())],
    );

    if count == 0 {
        return Ok(String::new()); // genuinely no change — silent, like gemba-tick's null path
    }
    let body: Vec<String> = result.fresh.iter().map(render_observation).collect();
    Ok(format!("{} ({} new):\n{}", target, count, body.join("\n")))
}
