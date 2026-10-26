//! #4756 — WTFU keep-alive: wake an idle actor that has work waiting on it.
//!
//! Jeff, 10-26 12:07: an idle session with cards or tasks waiting gets a
//! nudge, and turns stay atomic. Jeff was the keep-alive: Borg Queen sat idle
//! at her proves step from 06:13 until he typed into her pane.
//!
//! Runs in pair-heartbeat's 60s tick (com.chorus.pair-heartbeat), so there is
//! no second daemon to keep alive. Each tick:
//! - work: every dagu human task waiting on a WIP card whose prompt names its
//!   answerer ("(borg-queen answers)": pair greens, borg-queen-proves), and
//!   every nudge emitted to an actor over a minute ago and never surfaced
//! - idle: the actor's last turn ended (agent.turn.ended in Loki) and no turn
//!   started since (~/.chorus/run/turn-<actor>.json), 60s or more ago. A turn
//!   still running is never interrupted.
//! - wake: one nudge naming what waits (ops-nudge, the same delivery as every
//!   nudge), again only when the work changes or 30 minutes pass.
//!
//! Jeff is never woken: his go is his. WTFU_DRY=1 names who would be woken
//! and sends nothing.

use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// An actor idle this long with work waiting is woken.
pub const IDLE_SECS: i64 = 60;
/// The same work wakes the same actor again only after this long: pulse's
/// machine-lane cooldown (MACHINE_COOLDOWN_MS), which would hold it anyway.
pub const REWAKE_SECS: i64 = 1800;
/// A nudge younger than this may still be on its way.
pub const NUDGE_GRACE_SECS: i64 = 60;
/// How far back Loki is read for turn ends and nudges.
pub const WINDOW_SECS: i64 = 7200;
/// Every wake nudge starts with this, so a wake left unread is not more work.
pub const WAKE_PREFIX: &str = "wtfu ";

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Work {
    pub actor: String,
    /// stable id of the waiting thing: what the wake signature is made of
    pub key: String,
    pub what: String,
}

/// Who answers a human task, from its prompt's "(<actor> answers)". None for
/// no name ("( answers)") and for Jeff.
pub fn answerer(prompt: &str) -> Option<String> {
    let end = prompt.rfind(" answers)")?;
    let start = prompt[..end].rfind('(')? + 1;
    let who = prompt[start..end].trim();
    let ok = !who.is_empty() && who != "jeff" && who.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
    ok.then(|| who.to_string())
}

fn param(params: &str, key: &str) -> Option<String> {
    params.split_whitespace().find_map(|kv| kv.strip_prefix(&format!("{key}=")).map(|v| v.trim_matches('"').to_string()))
}

/// The waiting human tasks of `runs` (dagu's /dag-runs listing) on cards in
/// WIP. `detail(name, id)` reads one run's nodes. A run on a card not in WIP
/// is stale (4749's pair still read "waiting" after it landed) and wakes no one.
pub fn human_tasks(runs: &Value, wip: &[String], detail: impl Fn(&str, &str) -> Option<Value>) -> Vec<Work> {
    let mut out = Vec::new();
    for r in runs["dagRuns"].as_array().into_iter().flatten() {
        if r["statusLabel"] != "waiting" {
            continue;
        }
        let (name, id) = (r["name"].as_str().unwrap_or(""), r["dagRunId"].as_str().unwrap_or(""));
        let Some(card) = param(r["params"].as_str().unwrap_or(""), "CARD") else { continue };
        if !wip.contains(&card) {
            continue;
        }
        let Some(d) = detail(name, id) else { continue };
        let d = if d["dagRunDetails"].is_object() { &d["dagRunDetails"] } else { &d };
        for n in d["nodes"].as_array().into_iter().flatten() {
            if n["statusLabel"] != "waiting" {
                continue;
            }
            let step = n["step"]["name"].as_str().unwrap_or("");
            let prompt = n["step"]["humanTask"]["prompt"].as_str().or_else(|| n["step"]["with"]["prompt"].as_str()).unwrap_or("");
            if let Some(actor) = answerer(prompt) {
                out.push(Work { actor, key: format!("{name}/{id}/{step}"), what: format!("#{card} {name} {step} (run {id})") });
            }
        }
    }
    out
}

/// One Loki line: (epoch seconds, labels, the line).
fn loki_lines(v: &Value) -> Vec<(i64, &Value, &str)> {
    let mut out = Vec::new();
    for s in v["data"]["result"].as_array().into_iter().flatten() {
        for e in s["values"].as_array().into_iter().flatten() {
            let ns: i64 = e[0].as_str().and_then(|t| t.parse().ok()).unwrap_or(0);
            out.push((ns / 1_000_000_000, &s["stream"], e[1].as_str().unwrap_or("")));
        }
    }
    out
}

fn line_json(line: &str) -> Value {
    serde_json::from_str(line).unwrap_or(Value::Null)
}

/// When each actor's newest turn ended, from agent.turn.ended lines.
pub fn turn_ends(loki: &Value) -> BTreeMap<String, i64> {
    let mut out = BTreeMap::new();
    for (at, _, line) in loki_lines(loki) {
        let j = line_json(line);
        if j["event"] != "agent.turn.ended" {
            continue;
        }
        if let Some(role) = j["role"].as_str() {
            let e = out.entry(role.to_string()).or_insert(at);
            *e = (*e).max(at);
        }
    }
    out
}

/// `a=b,c=d` packed into one JSON value by chorus-log: the value of `key`.
fn packed(line: &str, key: &str) -> Option<String> {
    let start = line.find(&format!("{key}="))? + key.len() + 1;
    let tail = &line[start..];
    Some(tail[..tail.find([',', '"']).unwrap_or(tail.len())].to_string())
}

/// Nudges emitted to an actor before `now - NUDGE_GRACE_SECS` and never
/// surfaced (nudge_poll's fold: unread = emitted − surfaced, by trace). A wake
/// nudge is not counted: an actor who slept through one is not given more work.
pub fn unread_nudges(loki: &Value, now: i64) -> Vec<Work> {
    let lines = loki_lines(loki);
    let mut surfaced = BTreeSet::new();
    for (_, _, line) in &lines {
        let j = line_json(line);
        if j["event"] == "nudge.surfaced" {
            let t = j["trace_id"].as_str().map(String::from).or_else(|| j["trace"].as_str().map(|t| t.split(',').next().unwrap_or("").to_string()));
            if let Some(t) = t {
                surfaced.insert(t);
            }
        }
    }
    let mut out = Vec::new();
    for (at, _, line) in &lines {
        if !line.contains("\"event\":\"nudge.emitted\"") || *at > now - NUDGE_GRACE_SECS {
            continue;
        }
        let (Some(to), Some(trace)) = (packed(line, "to"), packed(line, "trace")) else { continue };
        if surfaced.contains(&trace) || to == "jeff" || line.contains(&format!("content={WAKE_PREFIX}")) {
            continue;
        }
        let from = line_json(line)["role"].as_str().unwrap_or("someone").to_string();
        out.push(Work { actor: to, key: format!("nudge/{trace}"), what: format!("an unread nudge from {from}") });
    }
    out
}

/// Seconds the actor has been idle, or None while a turn runs. `started` is
/// the newest turn start (the turn file), `ended` the newest turn end (Loki).
/// No end in the window and no turn running means idle the whole window.
pub fn idle_secs(started: Option<i64>, ended: Option<i64>, now: i64) -> Option<i64> {
    match (started, ended) {
        (Some(s), Some(e)) if s > e => None,
        (_, Some(e)) => Some(now - e),
        (Some(s), None) if now - s < WINDOW_SECS => None,
        _ => Some(WINDOW_SECS),
    }
}

/// The work signature a wake is remembered by.
pub fn signature(works: &[&Work]) -> String {
    let keys: BTreeSet<&str> = works.iter().map(|w| w.key.as_str()).collect();
    keys.into_iter().collect::<Vec<_>>().join(" ")
}

/// Wake when idle long enough with work waiting, and either the work is new
/// since the last wake or the last wake is REWAKE_SECS old.
pub fn should_wake(idle: Option<i64>, sig: &str, last_sig: &str, last_at: i64, now: i64) -> bool {
    let idle_enough = idle.is_some_and(|s| s >= IDLE_SECS);
    idle_enough && !sig.is_empty() && (sig != last_sig || now - last_at >= REWAKE_SECS)
}

/// The actor and the work lead: pulse dedupes machine nudges on their first
/// 48 characters, so two actors' wakes, or new work, must differ there.
pub fn wake_message(actor: &str, idle: i64, works: &[&Work]) -> String {
    let mut items: Vec<&str> = works.iter().filter(|w| !w.key.starts_with("nudge/")).map(|w| w.what.as_str()).collect();
    let nudges = works.len() - items.len();
    let n = format!("{nudges} unread nudge(s)");
    if nudges > 0 {
        items.push(&n);
    }
    format!("{WAKE_PREFIX}{actor}: {} waits on you; you have been idle {}m.", items.join("; "), idle / 60)
}

// --- the side-effecting tick ---

fn get(base: &str, path: &str, query: &[(&str, String)]) -> Option<Value> {
    let mut cmd = Command::new("curl");
    cmd.args(["-sf", "--max-time", "10"]);
    if base.starts_with("http") && !query.is_empty() {
        cmd.arg("-G");
        for (k, v) in query {
            cmd.args(["--data-urlencode", &format!("{k}={v}")]);
        }
    }
    // a file:// fixture is one <path>.json per path (dag-runs.json beside dag-runs/)
    let url = if base.starts_with("http") { format!("{base}{path}") } else { format!("{base}{path}.json") };
    let out = cmd.arg(url).output().ok()?;
    out.status.success().then(|| serde_json::from_slice(&out.stdout).ok()).flatten()
}

fn turn_started(run_dir: &Path, actor: &str) -> Option<i64> {
    let v: Value = serde_json::from_str(&fs::read_to_string(run_dir.join(format!("turn-{actor}.json"))).ok()?).ok()?;
    v["at"].as_i64()
}

fn state_path(home: &Path, actor: &str) -> PathBuf {
    home.join(format!("ops/wtfu/{actor}.json"))
}

/// One tick. `wip` None (board unreadable) wakes no one for a human task.
pub fn run(home: &Path, wip: Option<&[String]>, now: i64) -> crate::R<String> {
    let dagu = env::var("WTFU_DAGU").unwrap_or_else(|_| "http://127.0.0.1:8526".into());
    let loki = env::var("WTFU_LOKI").unwrap_or_else(|_| "http://127.0.0.1:3102".into());
    let run_dir = env::var("WTFU_RUN_DIR").map(PathBuf::from).unwrap_or_else(|_| PathBuf::from(env::var("HOME").unwrap_or_default()).join(".chorus/run"));

    let mut works = Vec::new();
    if let (Some(wip), Some(runs)) = (wip, get(&dagu, "/api/v1/dag-runs", &[("status", "7".into()), ("perPage", "100".into())])) {
        works.extend(human_tasks(&runs, wip, |name, id| get(&dagu, &format!("/api/v1/dag-runs/{name}/{id}"), &[])));
    }
    let q = |logql: &str| {
        let start = ((now - WINDOW_SECS) as i128 * 1_000_000_000).to_string();
        get(&loki, "/loki/api/v1/query_range", &[("query", logql.into()), ("start", start), ("limit", "5000".into())])
    };
    let Some(ends) = q(r#"{job="platform-chorus"} |= "agent.turn.ended""#) else {
        // no turn ends means no idle actor can be told apart from a busy one
        return Ok("wtfu: loki unreadable, woke no one".into());
    };
    let ends = turn_ends(&ends);
    if let Some(n) = q(r#"{job="platform-chorus"} |~ "nudge\\.(emitted|surfaced)""#) {
        works.extend(unread_nudges(&n, now));
    }

    let mut by_actor: BTreeMap<&str, Vec<&Work>> = BTreeMap::new();
    for w in &works {
        by_actor.entry(w.actor.as_str()).or_default().push(w);
    }
    let mut woke = Vec::new();
    for (actor, ws) in by_actor {
        let idle = idle_secs(turn_started(&run_dir, actor), ends.get(actor).copied(), now);
        let sig = signature(&ws);
        let path = state_path(home, actor);
        let last: Value = fs::read_to_string(&path).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or(Value::Null);
        if !should_wake(idle, &sig, last["sig"].as_str().unwrap_or(""), last["at"].as_i64().unwrap_or(0), now) {
            continue;
        }
        let idle = idle.unwrap_or(0);
        let msg = wake_message(actor, idle, &ws);
        if env::var("WTFU_DRY").is_ok() {
            woke.push(format!("{actor} (dry: {msg} keys: {sig})"));
            continue;
        }
        let sent = Command::new("bash").arg(home.join("platform/scripts/ops-nudge")).args([actor, msg.as_str()]).env_remove("CHORUS_ROLE").env_remove("ROLE").output();
        if !sent.is_ok_and(|o| o.status.success()) {
            continue; // not remembered: the next tick tries again
        }
        let _ = fs::create_dir_all(path.parent().unwrap());
        let _ = fs::write(&path, serde_json::json!({"sig": sig, "at": now}).to_string());
        crate::emit_spine(home, "wtfu.woke", &[("actor", actor), ("idle", &format!("{idle}s")), ("items", &ws.len().to_string())]);
        woke.push(actor.to_string());
    }
    Ok(format!("wtfu: {} item(s) waiting, woke [{}]", works.len(), woke.join(" ")))
}
