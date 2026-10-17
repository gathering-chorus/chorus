//! pair-heartbeat — daemon-side navigator-silence monitor (card #3253).
//!
//! Self-contained: std only; reads the active-pair registry + each navigator's
//! observation stream, shells `chorus-log` for the spine. Runs on a launchd
//! StartInterval (com.chorus.pair-heartbeat) — NOT an agent-session cron, NOT a skill.
//! That removes the #2317 fragility class at the root: no cron in a session to die,
//! no skill name to dangle ("unknown skill").
//!
//! Zero-ISO-parsing trick: the daemon stamps its OWN clock (last_active_epoch) when it
//! sees a newer observation ts (lexicographic compare, like pulse-gather). Silence is
//! `now - last_active_epoch` — plain epoch seconds, no ISO8601→epoch math.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub type R<T> = Result<T, String>;

/// Escalation ladder. Ordered Active < Warn < ReNudge < Stall (declaration order),
/// so "did it get worse?" is a `>` comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Level {
    Active,
    Warn,
    ReNudge,
    Stall,
}

impl Level {
    pub fn label(self) -> &'static str {
        match self {
            Level::Active => "active",
            Level::Warn => "warn",
            Level::ReNudge => "renudge",
            Level::Stall => "stall",
        }
    }
    pub fn parse(s: &str) -> Level {
        match s {
            "warn" => Level::Warn,
            "renudge" => Level::ReNudge,
            "stall" => Level::Stall,
            _ => Level::Active,
        }
    }
}

// --- pure core (unit-tested) ---

/// Whitespace-tolerant JSON string-field extractor (zero-dep). Mirrors pulse-gather.
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

/// Peek the navigator's last activity: the lexicographically-greatest observation ts
/// in the stream. Read-only — does NOT advance any cursor (distinct from pulse-gather's
/// delta gather). None when the stream is empty/garbage (caller treats absent as "no
/// stall", never escalating on missing data).
pub fn newest_ts(text: &str) -> Option<String> {
    let mut max: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if !line.starts_with('{') {
            continue;
        }
        if let Some(ts) = json_str_field(line, "ts") {
            if max.as_deref().map(|m| ts.as_str() > m).unwrap_or(true) {
                max = Some(ts);
            }
        }
    }
    max
}

/// Seconds of silence → escalation level. 60/120/180, parity with #2317.
pub fn silence_level(secs: i64) -> Level {
    if secs >= 180 {
        Level::Stall
    } else if secs >= 120 {
        Level::ReNudge
    } else if secs >= 60 {
        Level::Warn
    } else {
        Level::Active
    }
}

/// Fire an escalation ONLY when the level increased — each tier fires once, no re-spam,
/// and a navigator coming back (level drops) is a reset, not an event. Returns the new
/// level to act on, or None.
pub fn escalation(prev: Level, cur: Level) -> Option<Level> {
    if cur > prev {
        Some(cur)
    } else {
        None
    }
}

// --- registry (one file per active pair) ---

#[derive(Debug, Clone)]
pub struct PairEntry {
    pub card: String,
    pub navigator: String,
    /// #4577 — the driver, written by pair-start; kept through every tick
    pub driver: String,
    pub last_seen_iso: String,
    pub last_active_epoch: i64,
    pub last_level: Level,
}

impl PairEntry {
    fn from_json(j: &str) -> Option<PairEntry> {
        Some(PairEntry {
            card: json_str_field(j, "card")?,
            navigator: json_str_field(j, "navigator")?,
            driver: json_str_field(j, "driver").unwrap_or_default(),
            last_seen_iso: json_str_field(j, "last_seen_iso").unwrap_or_default(),
            last_active_epoch: json_str_field(j, "last_active_epoch")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            last_level: Level::parse(&json_str_field(j, "last_level").unwrap_or_default()),
        })
    }
    fn to_json(&self) -> String {
        format!(
            "{{\"card\":\"{}\",\"navigator\":\"{}\",\"driver\":\"{}\",\"last_seen_iso\":\"{}\",\"last_active_epoch\":\"{}\",\"last_level\":\"{}\"}}",
            self.card, self.navigator, self.driver, self.last_seen_iso, self.last_active_epoch, self.last_level.label()
        )
    }
}

// --- side-effecting real entry ---

fn now_epoch() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
}

fn registry_dir(home: &Path) -> PathBuf {
    home.join("ops/active-pairs")
}

/// #4577 — PAIR_OBS_DIR moves the observation streams (tests); the team-scan dir otherwise.
fn observations_path(role: &str) -> PathBuf {
    let dir = env::var("PAIR_OBS_DIR").unwrap_or_else(|_| "/tmp/claude-team-scan".to_string());
    PathBuf::from(format!("{}/{}-observations.jsonl", dir, role))
}

/// #4577 — who hears a stalled navigator: Abby, who runs the ceremonies. Before,
/// the 180s tier only emitted an event (as "wren") and nobody acted on it.
pub const STALL_TO: &str = "abby-normal";

/// #4577 — the nudge a level sends. A stall nudges Abby; at 120s the
/// navigator hears it first (Abby's review, 10-17: the renudge tier only
/// logged). Below that, nobody is nudged.
pub fn level_nudge(card: &str, navigator: &str, driver: &str, level: Level, silence: i64) -> Option<(String, String)> {
    let driver = if driver.is_empty() { "the driver" } else { driver };
    match level {
        Level::Stall => Some((
            STALL_TO.to_string(),
            format!("pair #{card}: navigator {navigator} silent {silence}s; the pair has stopped. Restart the navigator, or end it: `--input by=abby-normal --input decision=stop`."),
        )),
        Level::ReNudge => Some((
            navigator.to_string(),
            format!("pair #{card}: you have been quiet {silence}s and {driver} is waiting on you. Give the next direction, or the pair goes to Abby at 180s."),
        )),
        _ => None,
    }
}

/// #4577 — the cards in WIP, read from the board (PAIR_BOARD_URL, the context
/// API by default). None when the board cannot be read: nothing is pruned then.
pub fn wip_ids(json: &str) -> Vec<String> {
    let mut ids = Vec::new();
    let mut rest = json;
    while let Some(i) = rest.find("\"id\":") {
        rest = &rest[i + 5..];
        let n: String = rest.trim_start().trim_start_matches('"').chars().take_while(|c| c.is_ascii_digit()).collect();
        if !n.is_empty() {
            ids.push(n);
        }
    }
    ids
}

fn board_wip() -> Option<Vec<String>> {
    let url = env::var("PAIR_BOARD_URL").unwrap_or_else(|_| "http://127.0.0.1:3340/api/chorus/context/board/wip".to_string());
    let out = Command::new("curl").args(["-sf", "--max-time", "10", &url]).output().ok()?;
    let body = String::from_utf8_lossy(&out.stdout).into_owned();
    (out.status.success() && body.contains("\"cards\"")).then(|| wip_ids(&body))
}

/// Emit one event to the ONE spine via `chorus-log` — best-effort, never blocks.
fn emit_spine(home: &Path, event: &str, extras: &[(&str, &str)]) {
    let log = home.join("platform/scripts/chorus-log");
    let log_s = match log.to_str() {
        Some(s) => s.to_string(),
        None => return,
    };
    // #4577 — the daemon's own events are the system's, not a role's
    let mut argv: Vec<String> = vec![event.to_string(), "system".to_string()];
    for (k, v) in extras {
        argv.push(format!("{}={}", k, v));
    }
    let refs: Vec<&str> = std::iter::once(log_s.as_str()).chain(argv.iter().map(|s| s.as_str())).collect();
    let _ = Command::new("bash").args(&refs).output();
}

/// The escalation event for a level — fired by the daemon, queryable in Loki. The
/// 180s tier is the canonical `pair.navigator.stall` (#2317 parity).
fn fire(home: &Path, entry: &PairEntry, level: Level, silence: i64) {
    let event = match level {
        Level::Stall => "pair.navigator.stall",
        _ => "pair.heartbeat.silence",
    };
    emit_spine(
        home,
        event,
        &[
            ("card", entry.card.as_str()),
            ("nav", entry.navigator.as_str()),
            ("level", level.label()),
            ("elapsed", &format!("{}s", silence)),
        ],
    );
    if let Some((to, msg)) = level_nudge(&entry.card, &entry.navigator, &entry.driver, level, silence) {
        let _ = Command::new("bash").arg(home.join("platform/scripts/ops-nudge")).args([&to, &msg]).env_remove("CHORUS_ROLE").output();
    }
}

/// One tick: walk every active pair, update its silence state on the daemon's clock,
/// and fire each escalation tier once. Returns a one-line summary.
pub fn run() -> R<String> {
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    let dir = registry_dir(&home);
    let now = now_epoch();

    let entries = match fs::read_dir(&dir) {
        Ok(rd) => rd,
        Err(_) => return Ok("no active pairs".to_string()), // registry absent => nothing to watch
    };

    let wip = board_wip();
    let mut pruned = 0;
    let mut watched = 0;
    let mut fired = 0;
    for de in entries.flatten() {
        let path = de.path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let raw = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(_) => continue,
        };
        let mut entry = match PairEntry::from_json(&raw) {
            Some(e) => e,
            None => continue,
        };
        // #4577 (Abby, 10-17 09:24) — five pairs on Done cards kept nudging her:
        // a pair whose card left WIP is over, and is dropped here
        if let Some(wip) = &wip {
            if !wip.contains(&entry.card) {
                let _ = fs::remove_file(&path);
                emit_spine(&home, "pair.ended", &[("card", entry.card.as_str()), ("reason", "card-not-in-wip")]);
                pruned += 1;
                continue;
            }
        }
        watched += 1;

        let stream = fs::read_to_string(observations_path(&entry.navigator)).unwrap_or_default();
        let newest = newest_ts(&stream);

        match newest {
            // fresh activity (a ts strictly newer than last seen) => reset on our clock.
            Some(ts) if entry.last_seen_iso.is_empty() || ts.as_str() > entry.last_seen_iso.as_str() => {
                entry.last_seen_iso = ts;
                entry.last_active_epoch = now;
                entry.last_level = Level::Active;
            }
            // stream absent (post-reboot "rebuilding") => never a stall; hold, don't escalate.
            None if stream.is_empty() => {
                // leave state as-is; missing data is not silence.
            }
            // no newer turn => measure silence on our own clock.
            _ => {
                let silence = now - entry.last_active_epoch;
                let cur = silence_level(silence);
                if let Some(level) = escalation(entry.last_level, cur) {
                    fire(&home, &entry, level, silence);
                    fired += 1;
                }
                entry.last_level = cur;
            }
        }
        let _ = fs::write(&path, entry.to_json());
    }

    Ok(format!("pair-heartbeat: watched {} pair(s), fired {} escalation(s), dropped {} not in WIP{}", watched, fired, pruned,
        if wip.is_none() { " (board unreadable: none dropped)" } else { "" }))
}
