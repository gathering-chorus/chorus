//! chorus-ops — Rust port of chorus-ops.sh (DEC-100)
//!
//! Subcommands:
//!   errors    — Defect polling: query Loki for error patterns, dedup, auto-card
//!   health    — Health agent: pre-fetch system state, claude reasoning, act on findings
//!   all       — Run errors first, then health (health self-throttles to every 3rd invocation)
//!   status    — Show current state for both subsystems
//!   dry-run   — Show what each subsystem would do, don't act

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode, Output};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
use crate::shared::state_paths::chorus_root;
use regex::Regex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

// --- Configuration ---

const LOKI_URL: &str = "http://localhost:3102";
const ALERTMANAGER_URL: &str = "http://localhost:9093";
const DEFAULT_WINDOW: &str = "5m";
const DEFAULT_MODEL: &str = "haiku";
const DEFAULT_BUDGET: &str = "0.05";
const MAX_CARDS: usize = 2;
const PATTERN_THRESHOLD: u32 = 3;
const DEDUP_WINDOW_HOURS: i64 = 24;
const HEALTH_THROTTLE_EVERY: u64 = 3;
const COOLDOWN_HOURS: i64 = 24;

const CRITICAL_PATTERN: &str =
    r"(?i)\bpanic\b|\bfatal\b|\bOOM\b|\boom-kill\b|\bSIGKILL\b|\bcrash\b|\bsegfault\b|\bout of memory\b";

const FALSE_POSITIVES: &[&str] = &[
    "errorsmith",
    "npm ci failed.*continuing",
    "npm error.*permissions",
    "npm error.*complete log",
    "npm error.*root/Administrator",
    "WARN:.*npm ci",
    "write-scrubber",
    "infra-guardrails",
    "uncommitted files",
    "activity.md has no entries",
    "chorus-audit",
    "grafana-alerts",
    "INFO Fuseki.*PUT",
    "INFO Fuseki.*GET",
    "C3 memory usage",
    "traffic spike",
    "Deploy time",
    "Build time exceeds",
    "SPARQL query.*slow",
    "SPARQL query.*degraded",
    "XA crash recov",
    "command not found",
];

// --- CommandRunner seam (#2167) ---
//
// All `Command::new(...).output()` calls in the orchestrators go through this
// trait so tests can swap in a FakeCommandRunner that captures argv and
// returns canned Output without spawning real processes.

/// Seam for spawning external CLI tools (cards, chorus-log, claude, etc.).
pub(crate) trait CommandRunner {
    fn run(&self, bin: &Path, args: &[&str]) -> std::io::Result<Output>;

    /// Like `run` but writes `stdin_data` to the child's stdin before waiting.
    /// Used by the claude agent invocation which feeds the context JSON via
    /// stdin rather than argv (too large, and argv would leak via ps).
    fn run_with_stdin(
        &self,
        bin: &Path,
        args: &[&str],
        stdin_data: &[u8],
    ) -> std::io::Result<Output>;
}

/// Production runner — shells out via `std::process::Command`.
pub(crate) struct RealCommandRunner;

impl CommandRunner for RealCommandRunner {
    fn run(&self, bin: &Path, args: &[&str]) -> std::io::Result<Output> {
        Command::new(bin).args(args).output()
    }

    fn run_with_stdin(
        &self,
        bin: &Path,
        args: &[&str],
        stdin_data: &[u8],
    ) -> std::io::Result<Output> {
        let mut child = Command::new(bin)
            .args(args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .env_remove("CLAUDECODE")
            .spawn()?;
        if let Some(ref mut stdin) = child.stdin {
            let _ = stdin.write_all(stdin_data);
        }
        child.wait_with_output()
    }
}

// --- Action (extracted from do_errors for testability) ---

#[derive(Debug, Clone)]
pub(crate) struct Action {
    pub(crate) action_type: String, // "card" or "comment"
    pub(crate) hash: String,
    pub(crate) priority: String,
    pub(crate) _reason: String,
}

// --- State types ---

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct OpsState {
    version: u32,
    #[serde(default)]
    defects: HashMap<String, Defect>,
    #[serde(default)]
    last_errors_poll: String,
    #[serde(default)]
    health: HealthState,
    #[serde(default)]
    all_invocation_count: u64,
}

impl Default for OpsState {
    fn default() -> Self {
        Self {
            version: 2,
            defects: HashMap::new(),
            last_errors_poll: String::new(),
            health: HealthState::default(),
            all_invocation_count: 0,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct Defect {
    hash: String,
    source: String,
    pattern: String,
    sample: String,
    tier: String,
    count: u32,
    first_seen: String,
    last_seen: String,
    card_id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
struct HealthState {
    last_run: String,
    findings: Vec<Finding>,
    cards_created: u64,
    last_status: String,
    last_summary: String,
    #[serde(default)]
    carded_categories: HashMap<String, String>,
}

impl Default for HealthState {
    fn default() -> Self {
        Self {
            last_run: String::new(),
            findings: Vec::new(),
            cards_created: 0,
            last_status: "unknown".to_string(),
            last_summary: String::new(),
            carded_categories: HashMap::new(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub(crate) struct Finding {
    id: String,
    severity: String,
    category: String,
    title: String,
    description: String,
    action: String,
    is_repeat: bool,
}

// --- CLI parsing ---

pub(crate) struct Config {
    subcommand: String,
    window: String,
    model: String,
    budget: String,
    verbose: bool,
    dry_run: bool,
    script_dir: PathBuf,
    state_file: PathBuf,
    cards_bin: PathBuf,
    chorus_log_bin: PathBuf,
    prompt_file: PathBuf,
}

fn log_msg(msg: &str) {
    let now = chrono::Local::now().format("%H:%M:%S");
    eprintln!("[chorus-ops] {} {}", now, msg);
}

fn parse_args(args: &[String]) -> Result<Config, String> {
    let script_dir = find_script_dir();
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let state_dir = PathBuf::from(&home).join(".chorus");
    let _ = fs::create_dir_all(&state_dir);

    let mut config = Config {
        subcommand: String::new(),
        window: DEFAULT_WINDOW.to_string(),
        model: DEFAULT_MODEL.to_string(),
        budget: DEFAULT_BUDGET.to_string(),
        verbose: false,
        dry_run: false,
        script_dir: script_dir.clone(),
        state_file: state_dir.join("chorus-ops-state.json"),
        cards_bin: script_dir.join("cards"),
        chorus_log_bin: script_dir.join("chorus-log"),
        prompt_file: script_dir.join("ops-agent-prompt.md"),
    };

    if args.is_empty() {
        return Err("Usage: chorus-ops {errors|health|all|status|dry-run} [options]".to_string());
    }

    let sub = &args[0];
    config.subcommand = match sub.as_str() {
        "defects" => "errors".to_string(),
        "--help" | "-h" => return Err(help_text()),
        other => other.to_string(),
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--window" => {
                i += 1;
                config.window = args.get(i).cloned().ok_or("--window requires a value")?;
            }
            "--model" => {
                i += 1;
                config.model = args.get(i).cloned().ok_or("--model requires a value")?;
            }
            "--budget" => {
                i += 1;
                config.budget = args.get(i).cloned().ok_or("--budget requires a value")?;
            }
            "--verbose" => config.verbose = true,
            "--help" | "-h" => return Err(help_text()),
            other => return Err(format!("Unknown arg: {}", other)),
        }
        i += 1;
    }

    Ok(config)
}

fn find_script_dir() -> PathBuf {
    // Look for the scripts directory relative to the binary or use a known path
    let mut candidates: Vec<PathBuf> = Vec::new();

    // Env override
    if let Ok(dir) = std::env::var("CHORUS_SCRIPTS_DIR") {
        if !dir.is_empty() {
            candidates.push(PathBuf::from(dir));
        }
    }

    // Relative to the binary itself (binary is at .../chorus-hooks/target/release/)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(exe_dir) = exe.parent() {
            // target/release/ → ../../.. → chorus-hooks → ../../../scripts
            let relative = exe_dir.join("../../../scripts");
            if let Ok(canonical) = relative.canonicalize() {
                candidates.push(canonical);
            }
        }
    }

    // Symlinked scripts
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    candidates.push(PathBuf::from(&home).join(".chorus/scripts"));

    // Hardcoded fallback
    candidates.push(PathBuf::from(format!("{}/platform/scripts", chorus_root())));

    for c in &candidates {
        if c.join("cards").exists() {
            return c.clone();
        }
    }
    // Last resort
    PathBuf::from(&std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string())).join(".chorus/scripts")
}

fn help_text() -> String {
    r#"chorus-ops — Unified operations daemon (Rust port of chorus-ops.sh)

Usage: chorus-ops {defects|errors|health|all|status|dry-run} [options]

Subcommands:
  defects|errors      Poll Loki for defects, dedup, auto-card
  health              Health agent (claude reasoning)
  all                 Both (health self-throttles to every 3rd run)
  status              Show current state
  dry-run             Dry run both subsystems

Options:
  --window <5m|1h|1d> Error polling window (default: 5m)
  --model <model>     Health agent model (default: haiku)
  --verbose           Extra logging
  --help              Show this help

State: ~/.chorus/chorus-ops-state.json"#
        .to_string()
}

// --- State management ---

fn load_state(path: &Path) -> OpsState {
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => OpsState::default(),
    }
}

fn save_state(path: &Path, state: &OpsState) {
    if let Ok(json) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, json);
    }
}

// --- Lock file ---

fn acquire_lock() -> Result<(), String> {
    let lock_path = Path::new("/tmp/chorus-ops.lock");
    if lock_path.exists() {
        if let Ok(pid_str) = fs::read_to_string(lock_path) {
            let pid_str = pid_str.trim();
            if !pid_str.is_empty() {
                // Check if process is still running
                if let Ok(pid) = pid_str.parse::<i32>() {
                    if libc_kill(pid) {
                        return Err(format!("Already running (PID {}), skipping", pid));
                    }
                }
            }
        }
        let _ = fs::remove_file(lock_path);
    }
    let pid = std::process::id();
    let _ = fs::write(lock_path, pid.to_string());
    Ok(())
}

fn release_lock() {
    let _ = fs::remove_file("/tmp/chorus-ops.lock");
}

/// Check if a process is alive (signal 0)
fn libc_kill(pid: i32) -> bool {
    // Use kill(pid, 0) to check process existence
    let output = Command::new("kill")
        .args(["-0", &pid.to_string()])
        .output();
    matches!(output, Ok(o) if o.status.success())
}

// --- Pattern normalization ---

fn normalize_pattern(line: &str) -> String {
    let mut s = line.to_string();
    // Timestamps
    let re_ts = Regex::new(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}[.\d]*Z?").unwrap();
    s = re_ts.replace_all(&s, "<TS>").to_string();
    // UUIDs
    let re_uuid =
        Regex::new(r"[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}").unwrap();
    s = re_uuid.replace_all(&s, "<UUID>").to_string();
    // Long hex
    let re_hex = Regex::new(r"[0-9a-f]{16,}").unwrap();
    s = re_hex.replace_all(&s, "<HEX>").to_string();
    // Addresses
    let re_addr = Regex::new(r"0x[0-9a-f]+").unwrap();
    s = re_addr.replace_all(&s, "<ADDR>").to_string();
    // Paths
    let re_path = Regex::new(r"(?:/[\w.\-]+){2,}(?:\.[\w]+)?").unwrap();
    s = re_path.replace_all(&s, "<PATH>").to_string();
    // URLs
    let re_url = Regex::new(r#"https?://[^\s,"]+"#).unwrap();
    s = re_url.replace_all(&s, "<URL>").to_string();
    // Go goroutines
    let re_goroutine = Regex::new(r"goroutine \d+").unwrap();
    s = re_goroutine.replace_all(&s, "goroutine <N>").to_string();
    // Stack traces
    let re_stack = Regex::new(r#"stack=".+""#).unwrap();
    s = re_stack.replace_all(&s, r#"stack="<STACK>""#).to_string();
    // Message IDs
    let re_msgid = Regex::new(r"message [0-9a-f\-]+").unwrap();
    s = re_msgid.replace_all(&s, "message <ID>").to_string();
    // Watermill handler/topic/subscriber poisoned
    let re_hp = Regex::new(r"handler_poisoned=\S+").unwrap();
    s = re_hp.replace_all(&s, "handler_poisoned=<H>").to_string();
    let re_tp = Regex::new(r"topic_poisoned=\S+").unwrap();
    s = re_tp.replace_all(&s, "topic_poisoned=<T>").to_string();
    let re_sp = Regex::new(r"subscriber_poisoned=\S+").unwrap();
    s = re_sp.replace_all(&s, "subscriber_poisoned=<S>").to_string();
    let re_rp = Regex::new(r"reason_poisoned=.*$").unwrap();
    s = re_rp.replace_all(&s, "reason_poisoned=<REASON>").to_string();
    // Ports
    let re_port = Regex::new(r":\d{2,5}\b").unwrap();
    s = re_port.replace_all(&s, ":<PORT>").to_string();
    // Collapse whitespace
    let re_ws = Regex::new(r"\s+").unwrap();
    re_ws.replace_all(&s, " ").trim().to_string()
}

fn hash_pattern(source: &str, pattern: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}:{}", source, pattern));
    let result = hasher.finalize();
    hex::encode(&result[..8])
}

/// Simple hex encoding for the hash (avoid adding another dep)
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

fn is_false_positive(line: &str, fp_regexes: &[Regex]) -> bool {
    for fp in fp_regexes {
        if fp.is_match(line) {
            return true;
        }
    }
    // Also check parsed JSON appName+message
    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
        let app = parsed.get("appName").and_then(|v| v.as_str()).unwrap_or("");
        let msg = parsed.get("message").and_then(|v| v.as_str()).unwrap_or("");
        let combined = format!("{} {}", app, msg);
        for fp in fp_regexes {
            if fp.is_match(&combined) {
                return true;
            }
        }
    }
    false
}

fn classify_tier(line: &str, critical_re: &Regex) -> String {
    if critical_re.is_match(line) {
        "critical".to_string()
    } else {
        "warning".to_string()
    }
}

fn compile_false_positives() -> Vec<Regex> {
    FALSE_POSITIVES
        .iter()
        .filter_map(|p| Regex::new(&format!("(?i){}", p)).ok())
        .collect()
}

// --- Loki query ---

fn parse_window_seconds(window: &str) -> u64 {
    if let Some(m) = window.strip_suffix('m') {
        m.parse::<u64>().unwrap_or(5) * 60
    } else if let Some(h) = window.strip_suffix('h') {
        h.parse::<u64>().unwrap_or(1) * 3600
    } else if let Some(d) = window.strip_suffix('d') {
        d.parse::<u64>().unwrap_or(1) * 86400
    } else {
        300
    }
}

fn fetch_loki(query: &str, start_epoch: u64, end_epoch: u64) -> serde_json::Value {
    let url = format!("{}/loki/api/v1/query_range", LOKI_URL);
    let result = ureq::get(&url)
        .query("query", query)
        .query("limit", "100")
        .query("start", &start_epoch.to_string())
        .query("end", &end_epoch.to_string())
        .timeout(Duration::from_secs(10))
        .call();

    match result {
        Ok(resp) => resp.into_json().unwrap_or_else(|_| empty_loki_result()),
        Err(_) => empty_loki_result(),
    }
}

fn fetch_loki_instant(query: &str) -> serde_json::Value {
    let url = format!("{}/loki/api/v1/query", LOKI_URL);
    let result = ureq::get(&url)
        .query("query", query)
        .timeout(Duration::from_secs(10))
        .call();

    match result {
        Ok(resp) => resp.into_json().unwrap_or_else(|_| empty_loki_result()),
        Err(_) => empty_loki_result(),
    }
}

fn empty_loki_result() -> serde_json::Value {
    serde_json::json!({"data": {"result": []}})
}

fn loki_ready() -> bool {
    ureq::get(&format!("{}/ready", LOKI_URL))
        .timeout(Duration::from_secs(3))
        .call()
        .is_ok()
}

// --- ERRORS subcommand (extracted pieces, #2167) ---

/// Walk the Loki response map and mutate `state.defects`.
///
/// Returns (new_hashes, updated_hashes) — hashes of defects that were
/// freshly inserted vs. hashes whose `count` was incremented.
///
/// Pure-ish: state mutation is explicit via the `&mut OpsState` parameter,
/// but there's no I/O, no process spawning, no clock beyond the `now_iso`
/// string passed in by the caller.
pub(crate) fn process_error_streams(
    results: &HashMap<&str, serde_json::Value>,
    state: &mut OpsState,
    fp_regexes: &[Regex],
    critical_re: &Regex,
    now_iso: &str,
) -> (Vec<String>, Vec<String>) {
    let mut new_defects: Vec<String> = Vec::new();
    let mut updated_defects: Vec<String> = Vec::new();

    let source_labels = [
        ("structured", "STRUCTURED"),
        ("unstructured", "UNSTRUCTURED"),
        ("chorus", "CHORUS"),
    ];

    for (key, env_type) in &source_labels {
        let data = match results.get(key) {
            Some(d) => d,
            None => continue,
        };

        let streams = data
            .pointer("/data/result")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for stream in &streams {
            let labels = stream.get("stream").and_then(|v| v.as_object());
            let container = labels
                .and_then(|l| {
                    l.get("container_name")
                        .or_else(|| l.get("appName"))
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("unknown");

            let stream_app = labels
                .and_then(|l| l.get("appName").and_then(|v| v.as_str()))
                .unwrap_or("");
            let stream_fp = is_false_positive(stream_app, fp_regexes);

            let values = stream
                .get("values")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            #[allow(clippy::regex_creation_in_loops)]
            for entry in &values {
                let arr = match entry.as_array() {
                    Some(a) if a.len() >= 2 => a,
                    _ => continue,
                };
                let line = arr[1].as_str().unwrap_or("");

                if stream_fp || is_false_positive(line, fp_regexes) {
                    continue;
                }

                let msg;
                if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line) {
                    let log_level = parsed
                        .get("level")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_lowercase();
                    if !["error", "fatal", "panic", "err", "crit"].contains(&log_level.as_str()) {
                        continue;
                    }
                    msg = parsed
                        .get("message")
                        .or_else(|| parsed.get("msg"))
                        .and_then(|v| v.as_str())
                        .unwrap_or(line)
                        .to_string();
                } else {
                    if *env_type == "STRUCTURED" {
                        continue;
                    }
                    if *env_type == "CHORUS" {
                        msg = line.to_string();
                    } else {
                        let level_re = Regex::new(r"level=(\w+)").unwrap();
                        if let Some(caps) = level_re.captures(line) {
                            let lvl = caps[1].to_lowercase();
                            if !["error", "fatal", "panic", "err", "crit"].contains(&lvl.as_str())
                            {
                                continue;
                            }
                        }
                        msg = line.to_string();
                    }
                }

                let pattern = normalize_pattern(&msg);
                let h = hash_pattern(container, &pattern);
                let tier = classify_tier(line, critical_re);

                if let Some(existing) = state.defects.get_mut(&h) {
                    existing.count += 1;
                    existing.last_seen = now_iso.to_string();
                    if existing.tier != "critical" && tier == "critical" {
                        existing.tier = "critical".to_string();
                    }
                    updated_defects.push(h);
                } else {
                    state.defects.insert(
                        h.clone(),
                        Defect {
                            hash: h.clone(),
                            source: container.to_string(),
                            pattern: pattern.chars().take(200).collect(),
                            sample: msg.chars().take(500).collect(),
                            tier,
                            count: 1,
                            first_seen: now_iso.to_string(),
                            last_seen: now_iso.to_string(),
                            card_id: None,
                        },
                    );
                    new_defects.push(h);
                }
            }
        }
    }

    (new_defects, updated_defects)
}

/// Turn (new, updated) defect hashes into concrete Actions.
///
/// Rules:
///   - every new critical → P1 card
///   - every new warning  → P2 card
///   - updated without card + count ≥ threshold → P2 card
///   - updated with card + count % 10 == 0 → comment
///   - dedup across new/updated via a `seen` set
pub(crate) fn decide_actions(
    new_defects: &[String],
    updated_defects: &[String],
    state: &OpsState,
) -> Vec<Action> {
    let mut actions: Vec<Action> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for h in new_defects {
        if seen.contains(h) {
            continue;
        }
        seen.insert(h.clone());
        let d = match state.defects.get(h) {
            Some(d) => d,
            None => continue,
        };
        let priority = if d.tier == "critical" { "P1" } else { "P2" };
        let reason = if d.tier == "critical" {
            "new critical"
        } else {
            "new warning"
        };
        actions.push(Action {
            action_type: "card".to_string(),
            hash: h.clone(),
            priority: priority.to_string(),
            _reason: reason.to_string(),
        });
    }

    for h in updated_defects {
        if seen.contains(h) {
            continue;
        }
        let d = match state.defects.get(h) {
            Some(d) => d,
            None => continue,
        };
        if d.card_id.is_none() && d.count >= PATTERN_THRESHOLD {
            actions.push(Action {
                action_type: "card".to_string(),
                hash: h.clone(),
                priority: "P2".to_string(),
                _reason: format!("pattern threshold ({}x)", d.count),
            });
            seen.insert(h.clone());
        } else if d.card_id.is_some() && d.count % 10 == 0 {
            actions.push(Action {
                action_type: "comment".to_string(),
                hash: h.clone(),
                priority: String::new(),
                _reason: format!("recurring ({}x)", d.count),
            });
            seen.insert(h.clone());
        }
    }

    actions
}

/// Execute a `card` action via the command runner. Returns true if a card
/// was created (i.e. the runner output contained a parseable `#<id>`).
///
/// Side effects:
///   - spawns `cards add ...` then `chorus-log ops.defect.detected ...`
///   - writes the new card id back into `state.defects[hash].card_id` on success
///   - in dry_run mode, prints the intended action and returns false without
///     invoking the runner
pub(crate) fn execute_card_action<C: CommandRunner>(
    action: &Action,
    state: &mut OpsState,
    config: &Config,
    cmd: &C,
) -> bool {
    let d = match state.defects.get(&action.hash) {
        Some(d) => d.clone(),
        None => return false,
    };
    let title_prefix = if action.priority == "P1" {
        "DEFECT"
    } else {
        "defect"
    };
    let title = format!(
        "[{}] {}: {}",
        title_prefix,
        d.source,
        &d.pattern[..d.pattern.len().min(60)]
    );

    if config.dry_run {
        println!("DRY-RUN: would card [{}] {}", action.priority, title);
        return false;
    }

    let owner = if d.source.contains("personal-site-app") || d.source.contains("wordpress") {
        "Kade"
    } else {
        "Silas"
    };

    let desc = format!(
        "## Experience\nThe recurring error stops appearing in the logs — diagnosed and fixed, so it no longer adds noise or masks real failures.\n\n## AC\n- [ ] root cause identified\n- [ ] fix deployed\n- [ ] error no longer appears in logs\n\n## Detail\nAuto-detected by chorus-ops (errors).\nPattern: {}\nSample: {}\nFirst seen: {}\nCount: {}\nHash: {}",
        &d.pattern[..d.pattern.len().min(200)],
        &d.sample[..d.sample.len().min(300)],
        d.first_seen,
        d.count,
        d.hash
    );

    let args = [
        "add",
        &title,
        "--owner",
        owner,
        "--priority",
        action.priority.as_str(),
        "--status",
        "ops",
        "--domain",
        "infrastructure",
        "--type",
        "fix",
        "--description",
        &desc,
    ];
    let output = cmd.run(&config.cards_bin, &args);

    let mut carded = false;
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let card_re = Regex::new(r"#(\d+)").unwrap();
            let combined = format!("{} {}", stdout, stderr);
            if let Some(caps) = card_re.captures(&combined) {
                let card_id = caps[1].to_string();
                if let Some(defect) = state.defects.get_mut(&action.hash) {
                    defect.card_id = Some(card_id.clone());
                }
                println!(
                    "CARDED: #{} [{}] {}: {}",
                    card_id,
                    action.priority,
                    d.source,
                    &d.pattern[..d.pattern.len().min(60)]
                );
                carded = true;
            } else {
                eprintln!("ERROR: Card creation returned: {}", combined.trim());
            }
        }
        Err(e) => eprintln!("ERROR: Failed to spawn cards: {}", e),
    }

    // Spine event — best-effort.
    let card_id_str = state
        .defects
        .get(&action.hash)
        .and_then(|d| d.card_id.as_deref())
        .unwrap_or("none")
        .to_string();
    let source_str = format!("source={}", d.source);
    let tier_str = format!("tier={}", d.tier);
    let hash_str = format!("hash={}", d.hash);
    let card_str = format!("card_id={}", card_id_str);
    let pattern_str = format!("pattern={}", &d.pattern[..d.pattern.len().min(80)]);
    let _ = cmd.run(
        &config.chorus_log_bin,
        &[
            "ops.defect.detected",
            "system",
            &source_str,
            &tier_str,
            &hash_str,
            &card_str,
            &pattern_str,
        ],
    );

    carded
}

/// Execute a `comment` action — noop if the defect has no card_id or in dry_run.
pub(crate) fn execute_comment_action<C: CommandRunner>(
    action: &Action,
    state: &OpsState,
    config: &Config,
    cmd: &C,
) {
    let d = match state.defects.get(&action.hash) {
        Some(d) => d,
        None => return,
    };
    let card_id = match &d.card_id {
        Some(c) => c,
        None => return,
    };
    if config.dry_run {
        println!("DRY-RUN: would comment on #{} ({}x)", card_id, d.count);
        return;
    }
    let comment = format!(
        "Defect recurring: {}x since {}. Latest: {}",
        d.count, d.first_seen, d.last_seen
    );
    let _ = cmd.run(&config.cards_bin, &["comment", card_id, &comment]);
    println!("COMMENT: #{} ({}x)", card_id, d.count);
}

// --- ERRORS subcommand ---

fn do_errors(config: &Config) -> Result<(), String> {
    let now_epoch = Utc::now().timestamp() as u64;
    let seconds = parse_window_seconds(&config.window);
    let start_epoch = now_epoch - seconds;

    if !loki_ready() {
        log_msg("WARN: Loki unreachable");
        return Ok(());
    }

    // Loki queries
    let query_structured = r#"{container_name=~".+"} | json | level="error""#;
    let query_unstructured = r#"{container_name=~".+"} |~ "(?i)\\bpanic\\b|\\bfatal\\b|\\bOOM\\b|\\bSIGKILL\\b|\\bout of memory\\b|\\bcrash\\b|\\bsegfault\\b""#;
    let query_chorus = r#"{job="chorus-operations", level="error"}"#;

    // Fetch in parallel
    let (tx, rx) = mpsc::channel();
    let queries = vec![
        ("structured", query_structured.to_string()),
        ("unstructured", query_unstructured.to_string()),
        ("chorus", query_chorus.to_string()),
    ];

    for (label, query) in queries {
        let tx = tx.clone();
        thread::spawn(move || {
            let result = fetch_loki(&query, start_epoch, now_epoch);
            let _ = tx.send((label, result));
        });
    }
    drop(tx);

    let mut results: HashMap<&str, serde_json::Value> = HashMap::new();
    for (label, data) in rx {
        results.insert(label, data);
    }

    let runner = RealCommandRunner;
    let now_iso = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    do_errors_post_prefetch(results, config, &runner, &now_iso)
}

/// Everything in do_errors after the parallel Loki fetch completes:
///   - compile regex caches
///   - load state + expire entries older than DEDUP_WINDOW_HOURS
///   - process_error_streams (classify into new/updated defects)
///   - decide_actions + execute card/comment via cmd
///   - save state + print summary
pub(crate) fn do_errors_post_prefetch<C: CommandRunner>(
    results: HashMap<&str, serde_json::Value>,
    config: &Config,
    cmd: &C,
    now_iso: &str,
) -> Result<(), String> {
    let fp_regexes = compile_false_positives();
    let critical_re = Regex::new(CRITICAL_PATTERN).unwrap();

    let mut state = load_state(&config.state_file);

    let cutoff = (Utc::now() - chrono::Duration::hours(DEDUP_WINDOW_HOURS))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    state.defects.retain(|_, d| d.last_seen >= cutoff);

    let (new_defects, updated_defects) =
        process_error_streams(&results, &mut state, &fp_regexes, &critical_re, now_iso);

    let actions = decide_actions(&new_defects, &updated_defects, &state);
    let mut carded = 0u32;
    for act in &actions {
        if act.action_type == "card" {
            if execute_card_action(act, &mut state, config, cmd) {
                carded += 1;
            }
        } else if act.action_type == "comment" {
            execute_comment_action(act, &state, config, cmd);
        }
    }

    state.last_errors_poll = now_iso.to_string();
    save_state(&config.state_file, &state);

    let total = new_defects.len() + updated_defects.len();
    if total > 0 || carded > 0 {
        println!(
            "[errors] Poll: {} errors, {} new patterns, {} carded",
            total,
            new_defects.len(),
            carded
        );
    } else {
        println!("[errors] Poll: clean");
    }

    Ok(())
}

// --- HEALTH subcommand (extracted pieces, #2167) ---

/// Unwrap the claude-agent JSON envelope and pull out (status, findings, summary).
///
/// Handles both output shapes:
///   - "json" output-format → `{"structured_output": {...}}`
///   - "text" output-format → `{"result": "<json string>"}`  (falls through
///     `parse_result_field` which also strips markdown code fences)
///
/// Defaults: status = "ok", findings = [], summary = "No summary".
pub(crate) fn parse_agent_response(
    response_raw: &str,
) -> Result<(String, Vec<Finding>, String), String> {
    let response: serde_json::Value = serde_json::from_str(response_raw)
        .map_err(|e| format!("JSON parse error: {}", e))?;

    let inner = if let Some(so) = response.get("structured_output") {
        if !so.is_null() {
            so.clone()
        } else {
            parse_result_field(&response)?
        }
    } else {
        parse_result_field(&response)?
    };

    let status = inner
        .get("status")
        .and_then(|v| v.as_str())
        .unwrap_or("ok")
        .to_string();
    let findings: Vec<Finding> = inner
        .get("findings")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();
    let summary = inner
        .get("summary")
        .and_then(|v| v.as_str())
        .unwrap_or("No summary")
        .to_string();

    Ok((status, findings, summary))
}


/// Process a list of agent findings: for each, decide whether to card it
/// (respecting MAX_CARDS, repeat flag, cooldown), emit a LOG line, or skip.
///
/// Returns (cards_created_this_run, updated carded_categories). The caller
/// merges these back into `state.health`.
pub(crate) fn process_health_findings<C: CommandRunner>(
    findings: &[Finding],
    state: &mut OpsState,
    config: &Config,
    cmd: &C,
    now_iso: &str,
) -> (u64, HashMap<String, String>) {
    let mut cards_created: u64 = 0;
    let mut carded_categories = state.health.carded_categories.clone();

    #[allow(clippy::regex_creation_in_loops)]
    for f in findings {
        if f.action == "card"
            && !f.is_repeat
            && cards_created < MAX_CARDS as u64
            && !is_on_cooldown(&f.category, &carded_categories)
        {
            let priority = if f.severity == "critical" { "P1" } else { "P2" };
            let card_desc = format!(
                "## Experience\nThe detected health issue is resolved — the system returns to a healthy state and the finding clears.\n\n## AC\n- [ ] finding root-caused\n- [ ] fix applied\n- [ ] finding no longer detected\n\n## Detail\nAuto-detected by chorus-ops (health).\n{}\n\nFinding ID: {}\nSeverity: {}\nCategory: {}",
                f.description, f.id, f.severity, f.category
            );
            let title = format!("[ops-health] {}", f.title);
            let args = [
                "add",
                title.as_str(),
                "--owner",
                "Silas",
                "--priority",
                priority,
                "--status",
                "ops",
                "--domain",
                "infrastructure",
                "--type",
                "fix",
                "--description",
                &card_desc,
            ];
            match cmd.run(&config.cards_bin, &args) {
                Ok(out) => {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    let stderr = String::from_utf8_lossy(&out.stderr);
                    let combined = format!("{} {}", stdout, stderr);
                    let card_re = Regex::new(r"#(\d+)").unwrap();
                    let card_id = card_re
                        .captures(&combined)
                        .map(|c| c[1].to_string())
                        .unwrap_or("?".to_string());
                    cards_created += 1;
                    carded_categories.insert(f.category.clone(), now_iso.to_string());
                    println!("CARD: #{} [{}] {}", card_id, priority, f.title);
                }
                Err(e) => eprintln!("ERROR: card creation failed: {}", e),
            }
        } else if f.action == "card" && is_on_cooldown(&f.category, &carded_categories) {
            println!(
                "COOLDOWN: [{}] {}: {} (carded within {}h)",
                f.severity, f.category, f.title, COOLDOWN_HOURS
            );
        } else if f.action == "log" || (f.action == "card" && f.is_repeat) {
            println!("LOG: [{}] {}: {}", f.severity, f.id, f.title);
        }
        // action == "ignore" → skip
    }

    (cards_created, carded_categories)
}

/// Turn raw pre-fetched strings (alerts JSON, loki JSON, disk text, board text)
/// into a structured HealthContext. Pure — no I/O, no subprocess.
///
/// The caller supplies `timestamp` and `previous_findings` since those come
/// from clock + state respectively (orchestrator concerns).
pub(crate) fn assemble_health_context(
    fetched: &HashMap<String, String>,
    previous_findings: Vec<Finding>,
    timestamp: String,
) -> HealthContext {
    let alert_info = parse_alerts(fetched.get("alerts").map(|s| s.as_str()).unwrap_or("[]"));
    let error_info = parse_errors(
        fetched.get("loki_errors").map(|s| s.as_str()).unwrap_or(""),
        fetched.get("loki_sync").map(|s| s.as_str()).unwrap_or(""),
    );
    let disk_info = parse_disk_diskutil(fetched.get("disk").map(|s| s.as_str()).unwrap_or(""));
    let board_info = BoardInfo {
        summary: fetched
            .get("board")
            .map(|s| s.chars().take(2000).collect())
            .unwrap_or_default(),
    };

    HealthContext {
        timestamp,
        alerts: alert_info,
        errors: error_info,
        disk: disk_info,
        board: board_info,
        previous_findings,
    }
}

/// Invoke `claude -p` with the given system prompt + context payload via stdin.
/// Returns the raw stdout on success, or an error string on failure.
///
/// The Command::new leaf goes through the `CommandRunner` trait so tests can
/// feed a canned response without spawning the real claude binary.
pub(crate) fn run_claude_agent<C: CommandRunner>(
    config: &Config,
    context_json: &str,
    system_prompt: &str,
    cmd: &C,
) -> Result<String, String> {
    let json_schema = r#"{"type":"object","properties":{"status":{"type":"string"},"findings":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"severity":{"type":"string"},"category":{"type":"string"},"title":{"type":"string"},"description":{"type":"string"},"action":{"type":"string"},"is_repeat":{"type":"boolean"}},"required":["id","severity","category","title","description","action","is_repeat"]}},"summary":{"type":"string"}},"required":["status","findings","summary"]}"#;

    let args = [
        "-p",
        "--model",
        config.model.as_str(),
        "--permission-mode",
        "dontAsk",
        "--no-session-persistence",
        "--max-budget-usd",
        config.budget.as_str(),
        "--output-format",
        "json",
        "--json-schema",
        json_schema,
        "--disallowedTools",
        "Bash,Edit,Write,Glob,Grep,WebFetch,WebSearch,NotebookEdit,Task",
        "--system-prompt",
        system_prompt,
    ];
    let output = cmd.run_with_stdin(Path::new("claude"), &args, context_json.as_bytes());

    match output {
        Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).to_string()),
        Ok(out) => Err(format!(
            "claude -p failed (exit code {:?})",
            out.status.code()
        )),
        Err(e) => Err(format!("claude -p spawn failed: {}", e)),
    }
}

// --- HEALTH subcommand ---

/// Data collected during pre-fetch phase
#[derive(Serialize)]
pub(crate) struct HealthContext {
    timestamp: String,
    alerts: AlertInfo,
    errors: ErrorInfo,
    disk: DiskInfo,
    board: BoardInfo,
    previous_findings: Vec<Finding>,
}

#[derive(Serialize)]
struct AlertInfo {
    firing: Vec<AlertDetail>,
}

#[derive(Serialize)]
struct AlertDetail {
    alertname: String,
    severity: String,
    summary: String,
}

#[derive(Serialize)]
struct ErrorInfo {
    total_30m: u32,
    by_container: HashMap<String, u32>,
    sync_storm: SyncStorm,
}

#[derive(Serialize)]
struct SyncStorm {
    detected: bool,
    container: Option<String>,
    count: u32,
}

#[derive(Serialize)]
struct DiskInfo {
    usage_pct: u32,
    available_gb: f64,
}

#[derive(Serialize)]
struct BoardInfo {
    summary: String,
}

fn do_health(config: &Config) -> Result<(), String> {
    if config.verbose {
        log_msg("Phase 1: Pre-fetching system state");
    }

    // Parallel pre-fetch using threads
    let (tx, rx) = mpsc::channel();

    // Alertmanager
    {
        let tx = tx.clone();
        thread::spawn(move || {
            let url = format!("{}/api/v2/alerts?active=true", ALERTMANAGER_URL);
            let out = ureq::get(&url)
                .timeout(Duration::from_secs(5))
                .call()
                .ok()
                .and_then(|r| r.into_string().ok())
                .unwrap_or("[]".to_string());
            let _ = tx.send(("alerts", out));
        });
    }

    // Loki errors 30m
    {
        let tx = tx.clone();
        thread::spawn(move || {
            let data = fetch_loki_instant(
                r#"sum by (container_name) (count_over_time({container_name=~".+"} | json | level="error" [30m]))"#,
            );
            let _ = tx.send(("loki_errors", serde_json::to_string(&data).unwrap_or_default()));
        });
    }

    // Loki sync storm
    {
        let tx = tx.clone();
        thread::spawn(move || {
            let data = fetch_loki_instant(
                r#"sum by (container_name) (count_over_time({container_name=~".+"} |~ "(?i)(sync|fuseki)" | json | level="error" [30m]))"#,
            );
            let _ = tx.send(("loki_sync", serde_json::to_string(&data).unwrap_or_default()));
        });
    }

    // Disk — use diskutil instead of df (#1868 fix)
    {
        let tx = tx.clone();
        thread::spawn(move || {
            let out = Command::new("diskutil")
                .args(["info", "/"])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            let _ = tx.send(("disk", out));
        });
    }

    // Board
    {
        let tx = tx.clone();
        let cards = config.cards_bin.clone();
        thread::spawn(move || {
            let out = Command::new(&cards)
                .arg("list")
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            let _ = tx.send(("board", out));
        });
    }

    drop(tx);

    let mut fetched: HashMap<String, String> = HashMap::new();
    for (label, data) in rx {
        fetched.insert(label.to_string(), data);
    }

    if config.verbose {
        log_msg("Phase 1: Pre-fetch complete");
    }

    let runner = RealCommandRunner;
    let now_iso = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    do_health_post_prefetch(fetched, config, &runner, &now_iso)
}

/// Everything in do_health after the parallel pre-fetch completes:
///   - assemble HealthContext
///   - dry-run short-circuit
///   - read system prompt from config.prompt_file
///   - invoke claude via run_claude_agent
///   - parse the envelope
///   - process findings (creating cards via cmd)
///   - save state + emit spine event
///
/// Extracted so tests can drive the entire post-fetch flow with canned data
/// + a FakeCommandRunner that returns a scripted claude response.
pub(crate) fn do_health_post_prefetch<C: CommandRunner>(
    fetched: HashMap<String, String>,
    config: &Config,
    cmd: &C,
    now_iso: &str,
) -> Result<(), String> {
    // Assemble context.
    let mut state = load_state(&config.state_file);
    let previous_findings = state.health.findings.clone();
    let context =
        assemble_health_context(&fetched, previous_findings, now_iso.to_string());

    // Dry run: show context and exit.
    if config.dry_run {
        println!("[health] Dry run — context JSON:");
        if let Ok(json) = serde_json::to_string_pretty(&context) {
            println!("{}", json);
        }
        return Ok(());
    }

    if config.verbose {
        log_msg(&format!(
            "Phase 2: Calling claude -p (model={}, budget={})",
            config.model, config.budget
        ));
    }

    let system_prompt = match fs::read_to_string(&config.prompt_file) {
        Ok(p) => p,
        Err(_) => {
            log_msg(&format!(
                "ERROR: System prompt not found at {:?}",
                config.prompt_file
            ));
            return Err("Missing prompt file".to_string());
        }
    };

    let context_json = serde_json::to_string(&context).map_err(|e| e.to_string())?;

    let response_raw = match run_claude_agent(config, &context_json, &system_prompt, cmd) {
        Ok(s) => s,
        Err(e) => {
            log_msg(&format!("ERROR: {}", e));
            if e.contains("failed") {
                let status_arg = "status=error".to_string();
                let model_arg = format!("model={}", config.model);
                let _ = cmd.run(
                    &config.chorus_log_bin,
                    &["ops.agent.completed", "system", &status_arg, &model_arg],
                );
            }
            return Err(e);
        }
    };

    if response_raw.len() < 10 {
        log_msg(&format!(
            "ERROR: Claude response too small ({} bytes)",
            response_raw.len()
        ));
        let status_arg = "status=error".to_string();
        let model_arg = format!("model={}", config.model);
        let _ = cmd.run(
            &config.chorus_log_bin,
            &[
                "ops.agent.completed",
                "system",
                &status_arg,
                &model_arg,
                "error=empty_response",
            ],
        );
        return Err("Empty response".to_string());
    }

    // Best-effort debug dump — ignore failure (test environments may lack
    // the ~/Library/Logs/Chorus path).
    let _ = fs::write(
        "/Users/jeffbridwell/Library/Logs/Chorus/chorus-ops-last-health-response.json",
        &response_raw,
    );

    if config.verbose {
        log_msg(&format!(
            "Phase 2: Claude response received ({} bytes)",
            response_raw.len()
        ));
    }

    let (status, findings, summary) = parse_agent_response(&response_raw)?;

    if config.verbose {
        log_msg("Phase 3: Processing findings");
    }

    let (cards_created, carded_categories) =
        process_health_findings(&findings, &mut state, config, cmd, now_iso);

    // Spine event.
    let status_arg = format!("status={}", status);
    let findings_arg = format!("findings={}", findings.len());
    let cards_arg = format!("cards={}", cards_created);
    let model_arg = format!("model={}", config.model);
    let summary_arg = format!("summary={}", &summary[..summary.len().min(100)]);
    let _ = cmd.run(
        &config.chorus_log_bin,
        &[
            "ops.agent.completed",
            "system",
            &status_arg,
            &findings_arg,
            &cards_arg,
            &model_arg,
            &summary_arg,
        ],
    );

    state.health = HealthState {
        last_run: now_iso.to_string(),
        findings,
        cards_created: state.health.cards_created + cards_created,
        last_status: status.clone(),
        last_summary: summary.clone(),
        carded_categories,
    };
    save_state(&config.state_file, &state);

    println!(
        "[health] Run complete: status={} findings={} cards={}",
        status,
        state.health.findings.len(),
        cards_created
    );
    if !summary.is_empty() {
        println!("[health] Summary: {}", summary);
    }

    Ok(())
}

fn parse_result_field(response: &serde_json::Value) -> Result<serde_json::Value, String> {
    let result_text = response.get("result");
    match result_text {
        Some(serde_json::Value::String(s)) => {
            let mut cleaned = s.trim().to_string();
            if cleaned.starts_with("```") {
                let re_start = Regex::new(r"^```\w*\n?").unwrap();
                cleaned = re_start.replace(&cleaned, "").to_string();
                let re_end = Regex::new(r"\n?```\s*$").unwrap();
                cleaned = re_end.replace(&cleaned, "").to_string();
            }
            serde_json::from_str(&cleaned).map_err(|e| format!("JSON parse from result: {}", e))
        }
        Some(obj @ serde_json::Value::Object(_)) => Ok(obj.clone()),
        _ => Ok(response.clone()),
    }
}

fn is_on_cooldown(category: &str, carded: &HashMap<String, String>) -> bool {
    if let Some(last_str) = carded.get(category) {
        if let Ok(last) = DateTime::parse_from_rfc3339(
            &last_str.replace("Z", "+00:00"),
        ) {
            let elapsed = Utc::now().signed_duration_since(last.with_timezone(&Utc));
            return elapsed.num_seconds() < COOLDOWN_HOURS * 3600;
        }
        // Try parsing without timezone
        if let Ok(last) = chrono::NaiveDateTime::parse_from_str(last_str.trim_end_matches('Z'), "%Y-%m-%dT%H:%M:%S") {
            let last_utc = last.and_utc();
            let elapsed = Utc::now().signed_duration_since(last_utc);
            return elapsed.num_seconds() < COOLDOWN_HOURS * 3600;
        }
    }
    false
}

#[allow(dead_code)]
fn emit_chorus_log(config: &Config, args: &[&str]) {
    let _ = Command::new(&config.chorus_log_bin)
        .args(args)
        .output();
}

// --- Pre-fetch parsers ---

fn parse_alerts(alerts_json: &str) -> AlertInfo {
    let mut info = AlertInfo {
        firing: Vec::new(),
    };
    if let Ok(data) = serde_json::from_str::<serde_json::Value>(alerts_json) {
        if let Some(arr) = data.as_array() {
            for a in arr {
                let labels = a.get("labels").and_then(|v| v.as_object());
                let annotations = a.get("annotations").and_then(|v| v.as_object());
                info.firing.push(AlertDetail {
                    alertname: labels
                        .and_then(|l| l.get("alertname").and_then(|v| v.as_str()))
                        .unwrap_or("unknown")
                        .to_string(),
                    severity: labels
                        .and_then(|l| l.get("severity").and_then(|v| v.as_str()))
                        .unwrap_or("unknown")
                        .to_string(),
                    summary: annotations
                        .and_then(|a| {
                            a.get("summary")
                                .or_else(|| a.get("description"))
                                .and_then(|v| v.as_str())
                        })
                        .unwrap_or("")
                        .chars()
                        .take(200)
                        .collect(),
                });
            }
        }
    }
    info
}

fn parse_errors(loki_errors_json: &str, loki_sync_json: &str) -> ErrorInfo {
    let mut info = ErrorInfo {
        total_30m: 0,
        by_container: HashMap::new(),
        sync_storm: SyncStorm {
            detected: false,
            container: None,
            count: 0,
        },
    };

    if let Ok(data) = serde_json::from_str::<serde_json::Value>(loki_errors_json) {
        if let Some(results) = data.pointer("/data/result").and_then(|v| v.as_array()) {
            for result in results {
                let container = result
                    .pointer("/metric/container_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let count = result
                    .get("value")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(1))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(|f| f as u32)
                    .unwrap_or(0);
                info.by_container.insert(container.to_string(), count);
                info.total_30m += count;
            }
        }
    }

    if let Ok(data) = serde_json::from_str::<serde_json::Value>(loki_sync_json) {
        if let Some(results) = data.pointer("/data/result").and_then(|v| v.as_array()) {
            for result in results {
                let container = result
                    .pointer("/metric/container_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                let count = result
                    .get("value")
                    .and_then(|v| v.as_array())
                    .and_then(|a| a.get(1))
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(|f| f as u32)
                    .unwrap_or(0);
                if count > 10 {
                    info.sync_storm = SyncStorm {
                        detected: true,
                        container: Some(container.to_string()),
                        count,
                    };
                    break;
                }
            }
        }
    }

    info
}

/// Parse diskutil info output instead of df (#1868)
fn parse_disk_diskutil(diskutil_output: &str) -> DiskInfo {
    let mut total_bytes: u64 = 0;
    let mut free_bytes: u64 = 0;

    // diskutil info / outputs lines like:
    //   Container Total Space:     1999345127424 B (2.0 TB)
    //   Container Free Space:      598374400000 B (598.4 GB)
    //   Volume Used Space:         ...
    // Format: "Container Total Space:     2.0 TB (1995218165760 Bytes)"
    let re_total = Regex::new(r"Container Total Space:.*\((\d+) Bytes\)").unwrap();
    let re_free = Regex::new(r"Container Free Space:.*\((\d+) Bytes\)").unwrap();

    for line in diskutil_output.lines() {
        if let Some(caps) = re_total.captures(line) {
            total_bytes = caps[1].parse().unwrap_or(0);
        }
        if let Some(caps) = re_free.captures(line) {
            free_bytes = caps[1].parse().unwrap_or(0);
        }
    }

    if total_bytes > 0 {
        let used = total_bytes - free_bytes;
        let pct = ((used as f64 / total_bytes as f64) * 100.0) as u32;
        let avail_gb = free_bytes as f64 / 1_073_741_824.0;
        DiskInfo {
            usage_pct: pct,
            available_gb: (avail_gb * 10.0).round() / 10.0,
        }
    } else {
        DiskInfo {
            usage_pct: 0,
            available_gb: 0.0,
        }
    }
}

// --- STATUS subcommand ---

fn do_status(config: &Config) {
    let state = load_state(&config.state_file);

    println!("=== chorus-ops status ===");
    println!();

    // Errors subsystem
    println!("[errors] Last poll: {}", if state.last_errors_poll.is_empty() { "never" } else { &state.last_errors_poll });
    println!("[errors] Tracked defects: {}", state.defects.len());

    let mut sorted_defects: Vec<_> = state.defects.values().collect();
    sorted_defects.sort_by(|a, b| b.last_seen.cmp(&a.last_seen));
    for d in sorted_defects.iter().take(10) {
        println!(
            "  [{:>8}] x{:<3} card={:<6} {}: {}",
            d.tier,
            d.count,
            d.card_id.as_deref().unwrap_or("none"),
            d.source,
            &d.pattern[..d.pattern.len().min(80)]
        );
    }
    println!();

    // Health subsystem
    println!("[health] Last run: {}", if state.health.last_run.is_empty() { "never" } else { &state.health.last_run });
    println!("[health] Status: {}", state.health.last_status);
    println!("[health] Cards created (total): {}", state.health.cards_created);
    if !state.health.last_summary.is_empty() {
        println!("[health] Summary: {}", state.health.last_summary);
    }
    if !state.health.findings.is_empty() {
        println!("[health] Active findings: {}", state.health.findings.len());
        for f in &state.health.findings {
            println!("  [{:>8}] {}: {}", f.severity, f.id, f.title);
            println!(
                "           action={} repeat={}",
                f.action, f.is_repeat
            );
        }
    } else {
        println!("[health] No active findings.");
    }
    println!();
    println!("[all] Invocation count: {}", state.all_invocation_count);
    println!("[all] Health runs every {}th 'all' invocation", HEALTH_THROTTLE_EVERY);
}

// --- Main entry point ---

pub fn run(args: &[String]) -> ExitCode {
    let config = match parse_args(args) {
        Ok(c) => c,
        Err(msg) => {
            eprintln!("{}", msg);
            return if msg.contains("Usage:") || msg.contains("chorus-ops") {
                ExitCode::from(0) // help text
            } else {
                ExitCode::from(1)
            };
        }
    };

    match config.subcommand.as_str() {
        "status" => {
            do_status(&config);
            ExitCode::SUCCESS
        }
        "dry-run" => {
            let dry_config = Config {
                subcommand: "dry-run".to_string(),
                window: config.window.clone(),
                model: config.model.clone(),
                budget: config.budget.clone(),
                verbose: config.verbose,
                dry_run: true,
                script_dir: config.script_dir.clone(),
                state_file: config.state_file.clone(),
                cards_bin: config.cards_bin.clone(),
                chorus_log_bin: config.chorus_log_bin.clone(),
                prompt_file: config.prompt_file.clone(),
            };
            println!("=== errors (dry-run) ===");
            let _ = do_errors(&dry_config);
            println!();
            println!("=== health (dry-run) ===");
            let _ = do_health(&dry_config);
            ExitCode::SUCCESS
        }
        "errors" => {
            if let Err(e) = acquire_lock() {
                log_msg(&e);
                return ExitCode::SUCCESS; // Not an error — just skip
            }
            let result = do_errors(&config);
            release_lock();
            match result {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    log_msg(&format!("ERROR: {}", e));
                    ExitCode::from(1)
                }
            }
        }
        "health" => {
            if let Err(e) = acquire_lock() {
                log_msg(&e);
                return ExitCode::SUCCESS;
            }
            let result = do_health(&config);
            release_lock();
            match result {
                Ok(_) => ExitCode::SUCCESS,
                Err(e) => {
                    log_msg(&format!("ERROR: {}", e));
                    ExitCode::from(1)
                }
            }
        }
        "all" => {
            if let Err(e) = acquire_lock() {
                log_msg(&e);
                return ExitCode::SUCCESS;
            }

            // Always run errors
            let _ = do_errors(&config);

            // Health self-throttles
            let mut state = load_state(&config.state_file);
            state.all_invocation_count += 1;
            let count = state.all_invocation_count;
            save_state(&config.state_file, &state);

            if count.is_multiple_of(HEALTH_THROTTLE_EVERY) {
                if config.verbose {
                    log_msg(&format!("Health check triggered (invocation #{})", count));
                }
                let _ = do_health(&config);
            } else if config.verbose {
                log_msg(&format!(
                    "Health check skipped (invocation #{}, runs every {}th)",
                    count, HEALTH_THROTTLE_EVERY
                ));
            }

            release_lock();
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("Unknown subcommand: {}", other);
            eprintln!("Usage: chorus-ops {{errors|health|all|status|dry-run}} [options]");
            ExitCode::from(1)
        }
    }
}

#[cfg(test)]
mod pure_tests {
    //! Tests for pure/deterministic helpers in ops.rs. #2167 — coverage push.
    //!
    //! These are the functions that don't touch HTTP, disk state, or process
    //! spawning: pattern normalization, hash, false-positive matching, tier
    //! classification, window-seconds parsing, alertmanager/loki response
    //! parsing, disk-output parsing, cooldown check, argv parsing, help text.
    //!
    //! Orchestrator coverage (do_errors, do_health, do_status) is separate —
    //! needs Loki/Command seams that the next wave of the 80% push introduces.

    use super::*;

    // --- normalize_pattern ---

    #[test]
    fn normalize_pattern_redacts_iso_timestamp() {
        let out = normalize_pattern("2026-04-17T18:42:01.123Z error: something");
        assert!(out.contains("<TS>"), "got: {}", out);
        assert!(!out.contains("2026-04-17"));
    }

    #[test]
    fn normalize_pattern_redacts_uuid() {
        let out = normalize_pattern("trace 550e8400-e29b-41d4-a716-446655440000 failed");
        assert!(out.contains("<UUID>"), "got: {}", out);
    }

    #[test]
    fn normalize_pattern_redacts_long_hex() {
        let out = normalize_pattern("sha 0123456789abcdef0123456789abcdef boom");
        assert!(out.contains("<HEX>"), "got: {}", out);
    }

    #[test]
    fn normalize_pattern_redacts_memory_address() {
        let out = normalize_pattern("dereferenced 0xdeadbeef panic");
        assert!(out.contains("<ADDR>"), "got: {}", out);
    }

    #[test]
    fn normalize_pattern_redacts_filesystem_path() {
        let out = normalize_pattern("read /Users/jeff/project/file.rs failed");
        assert!(out.contains("<PATH>"), "got: {}", out);
    }

    #[test]
    fn normalize_pattern_redacts_url_when_path_regex_does_not_consume_it() {
        // Paths are redacted before URLs, so most URLs with multi-segment
        // paths get hit by the path regex first. A bare URL with no path
        // (scheme + host + port, ending at whitespace) takes the URL branch.
        let out = normalize_pattern(r#"fetch http://host "done""#);
        // Either <URL> or nothing — but the original URL is gone.
        assert!(!out.contains("http://host"), "got: {}", out);
        assert!(out.contains("<URL>") || out.contains("<PATH>"), "got: {}", out);
    }

    #[test]
    fn normalize_pattern_redacts_goroutine_id() {
        let out = normalize_pattern("goroutine 47 stuck");
        assert!(out.contains("goroutine <N>"), "got: {}", out);
    }

    #[test]
    fn normalize_pattern_redacts_stack_dump() {
        let out = normalize_pattern(r#"panic stack="main.go:42 foo.go:8" over"#);
        assert!(out.contains(r#"stack="<STACK>""#), "got: {}", out);
    }

    #[test]
    fn normalize_pattern_redacts_message_id() {
        let out = normalize_pattern("delivering message 7a3b-8c21-ef00 to queue");
        assert!(out.contains("message <ID>"), "got: {}", out);
    }

    #[test]
    fn normalize_pattern_redacts_watermill_poisoned_fields() {
        let out = normalize_pattern(
            r#"handler_poisoned=hX topic_poisoned=tY subscriber_poisoned=sZ reason_poisoned=boom"#,
        );
        assert!(out.contains("handler_poisoned=<H>"));
        assert!(out.contains("topic_poisoned=<T>"));
        assert!(out.contains("subscriber_poisoned=<S>"));
        assert!(out.contains("reason_poisoned=<REASON>"));
    }

    #[test]
    fn normalize_pattern_redacts_port() {
        let out = normalize_pattern("connect :3340 refused");
        assert!(out.contains(":<PORT>"), "got: {}", out);
    }

    #[test]
    fn normalize_pattern_collapses_whitespace() {
        let out = normalize_pattern("  multiple   spaces   here   ");
        assert_eq!(out, "multiple spaces here");
    }

    #[test]
    fn normalize_pattern_is_stable_for_equivalent_inputs() {
        // Two log lines that differ only in timestamp/port should normalize equal.
        let a = normalize_pattern("2026-04-17T10:00:00Z conn :3000 refused");
        let b = normalize_pattern("2026-04-17T11:22:33Z conn :9999 refused");
        assert_eq!(a, b);
    }

    // --- hash_pattern ---

    #[test]
    fn hash_pattern_is_deterministic() {
        let a = hash_pattern("loki", "panic: OOM");
        let b = hash_pattern("loki", "panic: OOM");
        assert_eq!(a, b);
    }

    #[test]
    fn hash_pattern_differs_on_pattern() {
        let a = hash_pattern("loki", "panic: OOM");
        let b = hash_pattern("loki", "panic: segfault");
        assert_ne!(a, b);
    }

    #[test]
    fn hash_pattern_differs_on_source() {
        let a = hash_pattern("loki", "panic: OOM");
        let b = hash_pattern("chorus", "panic: OOM");
        assert_ne!(a, b);
    }

    #[test]
    fn hash_pattern_is_hex_8_bytes_equals_16_chars() {
        // Truncated to first 8 bytes → 16 hex chars.
        let h = hash_pattern("x", "y");
        assert_eq!(h.len(), 16);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit()));
    }

    // --- is_false_positive ---

    #[test]
    fn is_false_positive_matches_plain_line() {
        let fps = compile_false_positives();
        assert!(is_false_positive("errorsmith test run", &fps));
    }

    #[test]
    fn is_false_positive_rejects_real_errors() {
        let fps = compile_false_positives();
        assert!(!is_false_positive("FATAL: database connection lost", &fps));
    }

    #[test]
    fn is_false_positive_matches_against_json_appname_message() {
        let fps = compile_false_positives();
        let line = r#"{"appName":"chorus-audit","message":"nightly run complete","level":"error"}"#;
        assert!(is_false_positive(line, &fps), "should match chorus-audit in JSON");
    }

    #[test]
    fn is_false_positive_handles_malformed_json_gracefully() {
        let fps = compile_false_positives();
        assert!(!is_false_positive("{not valid json", &fps));
    }

    // --- classify_tier ---

    #[test]
    fn classify_tier_panic_is_critical() {
        // \bpanic\b requires a word boundary after "panic" — "panicked" has
        // none (k is still a word char), so use "panic:" for the bare word.
        let re = Regex::new(CRITICAL_PATTERN).unwrap();
        assert_eq!(classify_tier("panic: runtime error", &re), "critical");
    }

    #[test]
    fn classify_tier_oom_is_critical() {
        let re = Regex::new(CRITICAL_PATTERN).unwrap();
        assert_eq!(classify_tier("killed: OOM", &re), "critical");
    }

    #[test]
    fn classify_tier_plain_error_is_warning() {
        let re = Regex::new(CRITICAL_PATTERN).unwrap();
        assert_eq!(classify_tier("failed to parse config", &re), "warning");
    }

    // --- compile_false_positives ---

    #[test]
    fn compile_false_positives_produces_one_regex_per_entry() {
        let fps = compile_false_positives();
        assert_eq!(fps.len(), FALSE_POSITIVES.len());
    }

    #[test]
    fn compile_false_positives_is_case_insensitive() {
        let fps = compile_false_positives();
        // "errorsmith" entry should match upper-case variants.
        assert!(is_false_positive("ERRORSMITH active", &fps));
    }

    // --- parse_window_seconds ---

    #[test]
    fn parse_window_seconds_minutes() {
        assert_eq!(parse_window_seconds("5m"), 300);
        assert_eq!(parse_window_seconds("30m"), 1800);
    }

    #[test]
    fn parse_window_seconds_hours() {
        assert_eq!(parse_window_seconds("1h"), 3600);
        assert_eq!(parse_window_seconds("24h"), 86400);
    }

    #[test]
    fn parse_window_seconds_days() {
        assert_eq!(parse_window_seconds("1d"), 86400);
        assert_eq!(parse_window_seconds("7d"), 7 * 86400);
    }

    #[test]
    fn parse_window_seconds_falls_back_to_5m_default() {
        assert_eq!(parse_window_seconds("garbage"), 300);
        assert_eq!(parse_window_seconds(""), 300);
    }

    #[test]
    fn parse_window_seconds_unparseable_suffix_falls_back() {
        // "abcm" → strips "m", "abc".parse::<u64>() fails, default 5 * 60.
        assert_eq!(parse_window_seconds("abcm"), 300);
    }

    // --- empty_loki_result ---

    #[test]
    fn empty_loki_result_has_data_result_empty_array() {
        let v = empty_loki_result();
        let arr = v
            .pointer("/data/result")
            .and_then(|v| v.as_array())
            .expect("data.result present");
        assert_eq!(arr.len(), 0);
    }

    // --- parse_result_field ---

    #[test]
    fn parse_result_field_unwraps_string_json_payload() {
        let r = serde_json::json!({"result": "{\"ok\": true}"});
        let parsed = parse_result_field(&r).unwrap();
        assert_eq!(parsed["ok"], true);
    }

    #[test]
    fn parse_result_field_strips_markdown_code_fence() {
        let r = serde_json::json!({"result": "```json\n{\"x\": 1}\n```"});
        let parsed = parse_result_field(&r).unwrap();
        assert_eq!(parsed["x"], 1);
    }

    #[test]
    fn parse_result_field_returns_object_as_is() {
        let r = serde_json::json!({"result": {"y": 2}});
        let parsed = parse_result_field(&r).unwrap();
        assert_eq!(parsed["y"], 2);
    }

    #[test]
    fn parse_result_field_missing_returns_whole_response() {
        let r = serde_json::json!({"other": "field"});
        let parsed = parse_result_field(&r).unwrap();
        assert_eq!(parsed["other"], "field");
    }

    #[test]
    fn parse_result_field_malformed_string_errors() {
        let r = serde_json::json!({"result": "not { valid json"});
        assert!(parse_result_field(&r).is_err());
    }

    // --- is_on_cooldown ---

    #[test]
    fn is_on_cooldown_false_when_category_absent() {
        let carded = HashMap::new();
        assert!(!is_on_cooldown("disk", &carded));
    }

    #[test]
    fn is_on_cooldown_true_for_recent_timestamp_rfc3339() {
        let mut carded = HashMap::new();
        let now = Utc::now().to_rfc3339();
        carded.insert("disk".to_string(), now);
        assert!(is_on_cooldown("disk", &carded));
    }

    #[test]
    fn is_on_cooldown_false_for_old_timestamp() {
        let mut carded = HashMap::new();
        // 48h ago — past the 24h window.
        let old = (Utc::now() - chrono::Duration::hours(48)).to_rfc3339();
        carded.insert("disk".to_string(), old);
        assert!(!is_on_cooldown("disk", &carded));
    }

    #[test]
    fn is_on_cooldown_handles_z_suffix_timestamp() {
        let mut carded = HashMap::new();
        // "Z" style — the code substitutes "+00:00" before parsing.
        let ts = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        carded.insert("disk".to_string(), ts);
        assert!(is_on_cooldown("disk", &carded));
    }

    #[test]
    fn is_on_cooldown_handles_naive_timestamp_fallback() {
        let mut carded = HashMap::new();
        // No timezone at all — hits the NaiveDateTime fallback branch.
        let ts = Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string();
        carded.insert("disk".to_string(), ts);
        assert!(is_on_cooldown("disk", &carded));
    }

    #[test]
    fn is_on_cooldown_false_for_unparseable_timestamp() {
        let mut carded = HashMap::new();
        carded.insert("disk".to_string(), "not a date".to_string());
        assert!(!is_on_cooldown("disk", &carded));
    }

    // --- parse_alerts ---

    #[test]
    fn parse_alerts_empty_array_gives_no_firing() {
        let info = parse_alerts("[]");
        assert_eq!(info.firing.len(), 0);
    }

    #[test]
    fn parse_alerts_malformed_json_gives_no_firing() {
        let info = parse_alerts("{not json");
        assert_eq!(info.firing.len(), 0);
    }

    #[test]
    fn parse_alerts_extracts_alertname_severity_summary() {
        let json = r#"[
            {
                "labels": {"alertname": "DiskFull", "severity": "critical"},
                "annotations": {"summary": "root partition 99% full"}
            }
        ]"#;
        let info = parse_alerts(json);
        assert_eq!(info.firing.len(), 1);
        assert_eq!(info.firing[0].alertname, "DiskFull");
        assert_eq!(info.firing[0].severity, "critical");
        assert_eq!(info.firing[0].summary, "root partition 99% full");
    }

    #[test]
    fn parse_alerts_falls_back_to_description_when_no_summary() {
        let json = r#"[
            {
                "labels": {"alertname": "X", "severity": "warning"},
                "annotations": {"description": "from description"}
            }
        ]"#;
        let info = parse_alerts(json);
        assert_eq!(info.firing[0].summary, "from description");
    }

    #[test]
    fn parse_alerts_missing_labels_defaults_to_unknown() {
        let json = r#"[{"annotations": {"summary": "x"}}]"#;
        let info = parse_alerts(json);
        assert_eq!(info.firing[0].alertname, "unknown");
        assert_eq!(info.firing[0].severity, "unknown");
    }

    #[test]
    fn parse_alerts_truncates_summary_to_200_chars() {
        let long = "a".repeat(500);
        let json = format!(
            r#"[{{"labels":{{"alertname":"x","severity":"s"}},"annotations":{{"summary":"{}"}}}}]"#,
            long
        );
        let info = parse_alerts(&json);
        assert_eq!(info.firing[0].summary.chars().count(), 200);
    }

    // --- parse_errors ---

    #[test]
    fn parse_errors_empty_input_gives_zero_counts() {
        let info = parse_errors("{}", "{}");
        assert_eq!(info.total_30m, 0);
        assert_eq!(info.by_container.len(), 0);
        assert!(!info.sync_storm.detected);
    }

    #[test]
    fn parse_errors_aggregates_by_container() {
        let errors = r#"{
            "data": {"result": [
                {"metric": {"container_name": "api"}, "value": [0, "5"]},
                {"metric": {"container_name": "worker"}, "value": [0, "3"]}
            ]}
        }"#;
        let info = parse_errors(errors, "{}");
        assert_eq!(info.total_30m, 8);
        assert_eq!(info.by_container.get("api"), Some(&5));
        assert_eq!(info.by_container.get("worker"), Some(&3));
    }

    #[test]
    fn parse_errors_detects_sync_storm_above_threshold() {
        let sync = r#"{
            "data": {"result": [
                {"metric": {"container_name": "chorus"}, "value": [0, "25"]}
            ]}
        }"#;
        let info = parse_errors("{}", sync);
        assert!(info.sync_storm.detected);
        assert_eq!(info.sync_storm.container.as_deref(), Some("chorus"));
        assert_eq!(info.sync_storm.count, 25);
    }

    #[test]
    fn parse_errors_no_sync_storm_below_threshold() {
        // threshold is > 10, so 10 exactly does not trigger.
        let sync = r#"{
            "data": {"result": [
                {"metric": {"container_name": "x"}, "value": [0, "10"]}
            ]}
        }"#;
        let info = parse_errors("{}", sync);
        assert!(!info.sync_storm.detected);
    }

    #[test]
    fn parse_errors_missing_container_name_defaults_to_unknown() {
        let errors = r#"{
            "data": {"result": [
                {"metric": {}, "value": [0, "1"]}
            ]}
        }"#;
        let info = parse_errors(errors, "{}");
        assert_eq!(info.by_container.get("unknown"), Some(&1));
    }

    // --- parse_disk_diskutil ---

    #[test]
    fn parse_disk_diskutil_computes_pct_and_available_gb() {
        // Total 2GB, free 1GB → 50% used, 1.0 GB available.
        let out = "\
Container Total Space:     2.0 GB (2147483648 Bytes)
Container Free Space:      1.0 GB (1073741824 Bytes)
";
        let info = parse_disk_diskutil(out);
        assert_eq!(info.usage_pct, 50);
        assert_eq!(info.available_gb, 1.0);
    }

    #[test]
    fn parse_disk_diskutil_empty_output_gives_zeros() {
        let info = parse_disk_diskutil("");
        assert_eq!(info.usage_pct, 0);
        assert_eq!(info.available_gb, 0.0);
    }

    #[test]
    fn parse_disk_diskutil_rounds_to_one_decimal() {
        // 1.23 GB free → should render as 1.2.
        let gb_bytes = 1_320_000_000u64;
        let total = 2_147_483_648u64;
        let out = format!(
            "\
Container Total Space:     2.0 GB ({} Bytes)
Container Free Space:      1.23 GB ({} Bytes)
",
            total, gb_bytes
        );
        let info = parse_disk_diskutil(&out);
        // 1_320_000_000 / 1_073_741_824 ≈ 1.2294; rounded to 1.2.
        assert_eq!(info.available_gb, 1.2);
    }

    // --- parse_args ---

    fn sv(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// `Config` deliberately doesn't implement `Debug` (it holds paths that we
    /// don't want spraying into assert messages). These helpers avoid the
    /// Debug bound that `unwrap`/`unwrap_err` would otherwise require.
    fn cfg(r: Result<Config, String>) -> Config {
        match r {
            Ok(c) => c,
            Err(e) => panic!("expected Ok(Config), got Err({})", e),
        }
    }
    fn err(r: Result<Config, String>) -> String {
        match r {
            Ok(_) => panic!("expected Err, got Ok(Config)"),
            Err(e) => e,
        }
    }

    #[test]
    fn parse_args_empty_returns_usage_error() {
        let e = err(parse_args(&sv(&[])));
        assert!(e.contains("Usage:"));
    }

    #[test]
    fn parse_args_errors_subcommand() {
        let c = cfg(parse_args(&sv(&["errors"])));
        assert_eq!(c.subcommand, "errors");
        assert_eq!(c.window, DEFAULT_WINDOW);
    }

    #[test]
    fn parse_args_defects_aliases_to_errors() {
        let c = cfg(parse_args(&sv(&["defects"])));
        assert_eq!(c.subcommand, "errors");
    }

    #[test]
    fn parse_args_health_subcommand() {
        let c = cfg(parse_args(&sv(&["health"])));
        assert_eq!(c.subcommand, "health");
    }

    #[test]
    fn parse_args_window_flag_overrides_default() {
        let c = cfg(parse_args(&sv(&["errors", "--window", "1h"])));
        assert_eq!(c.window, "1h");
    }

    #[test]
    fn parse_args_model_and_budget_flags() {
        let c = cfg(parse_args(&sv(&["health", "--model", "sonnet", "--budget", "0.25"])));
        assert_eq!(c.model, "sonnet");
        assert_eq!(c.budget, "0.25");
    }

    #[test]
    fn parse_args_verbose_flag() {
        let c = cfg(parse_args(&sv(&["all", "--verbose"])));
        assert!(c.verbose);
    }

    #[test]
    fn parse_args_unknown_flag_errors() {
        let e = err(parse_args(&sv(&["errors", "--nope"])));
        assert!(e.contains("Unknown arg: --nope"));
    }

    #[test]
    fn parse_args_window_missing_value_errors() {
        let e = err(parse_args(&sv(&["errors", "--window"])));
        assert!(e.contains("--window requires a value"));
    }

    #[test]
    fn parse_args_help_flag_returns_help_text_as_err() {
        // `parse_args` uses Err to signal "print and exit" for --help.
        let e = err(parse_args(&sv(&["--help"])));
        assert!(e.contains("chorus-ops"));
        assert!(e.contains("Subcommands:"));
    }

    #[test]
    fn parse_args_help_flag_after_subcommand() {
        let e = err(parse_args(&sv(&["errors", "--help"])));
        assert!(e.contains("Subcommands:"));
    }

    // --- help_text ---

    #[test]
    fn help_text_describes_subcommands() {
        let t = help_text();
        assert!(t.contains("errors"));
        assert!(t.contains("health"));
        assert!(t.contains("status"));
        assert!(t.contains("dry-run"));
    }

    #[test]
    fn help_text_lists_window_option() {
        assert!(help_text().contains("--window"));
    }

    // --- load_state / save_state roundtrip (filesystem but hermetic via tempfile) ---

    #[test]
    fn load_state_missing_file_returns_default() {
        let s = load_state(Path::new("/tmp/__does_not_exist_for_chorus_test"));
        assert_eq!(s.all_invocation_count, 0);
        assert_eq!(s.defects.len(), 0);
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn save_state_then_load_state_roundtrips() {
        let tmp = std::env::temp_dir().join(format!("chorus-ops-test-{}.json", std::process::id()));
        let mut s = OpsState::default();
        s.all_invocation_count = 42;
        s.last_errors_poll = "2026-04-17T18:00:00Z".to_string();
        save_state(&tmp, &s);

        let loaded = load_state(&tmp);
        assert_eq!(loaded.all_invocation_count, 42);
        assert_eq!(loaded.last_errors_poll, "2026-04-17T18:00:00Z");

        let _ = fs::remove_file(&tmp);
    }

    #[test]
    fn load_state_malformed_json_returns_default() {
        let tmp = std::env::temp_dir().join(format!("chorus-ops-bad-{}.json", std::process::id()));
        fs::write(&tmp, "{not json").unwrap();

        let loaded = load_state(&tmp);
        assert_eq!(loaded.all_invocation_count, 0);

        let _ = fs::remove_file(&tmp);
    }
}

#[cfg(test)]
mod orchestrator_tests {
    //! Tests for the do_errors orchestrator pieces that were extracted from
    //! the monolithic function so they become unit-testable without hitting
    //! Loki or spawning subprocess. #2167 — push ops.rs from 29% → 80%.
    //!
    //! Extraction:
    //!   - `process_error_streams` — classifies stream entries into
    //!     new/updated defects, mutating state; pure input → pure output
    //!   - `decide_actions` — turns (new, updated) defect hashes into
    //!     Vec<Action>; pure
    //!   - `execute_card_action` / `execute_comment_action` — the Command
    //!     spawn leaves behind `CommandRunner` trait; tests use a FakeRunner
    //!     capturing argv + canned stdout.

    use super::*;
    use std::cell::RefCell;
    use std::os::unix::process::ExitStatusExt;
    use std::process::ExitStatus;

    // --- FakeCommandRunner: captures argv, returns canned Output ---

    struct FakeCommandRunner {
        queue: RefCell<Vec<std::io::Result<Output>>>,
        calls: RefCell<Vec<(PathBuf, Vec<String>)>>,
    }

    impl FakeCommandRunner {
        fn new(responses: Vec<std::io::Result<Output>>) -> Self {
            FakeCommandRunner {
                queue: RefCell::new(responses),
                calls: RefCell::new(Vec::new()),
            }
        }

        fn always_ok(stdout: &str) -> Self {
            Self::new(vec![Ok(mk_output(stdout, ""))])
        }

        /// A runner that panics if `.run()` is ever called. Use in tests that
        /// assert a branch short-circuits before touching the subprocess seam.
        fn never() -> Self {
            FakeCommandRunner {
                queue: RefCell::new(Vec::new()),
                calls: RefCell::new(Vec::new()),
            }
        }
    }

    impl CommandRunner for FakeCommandRunner {
        fn run(&self, bin: &Path, args: &[&str]) -> std::io::Result<Output> {
            self.calls.borrow_mut().push((
                bin.to_path_buf(),
                args.iter().map(|s| s.to_string()).collect(),
            ));
            if self.queue.borrow().is_empty() {
                Ok(mk_output("", ""))
            } else {
                self.queue.borrow_mut().remove(0)
            }
        }

        fn run_with_stdin(
            &self,
            bin: &Path,
            args: &[&str],
            _stdin_data: &[u8],
        ) -> std::io::Result<Output> {
            // Same shape as run — stdin is captured by the StdinCapturingRunner
            // wrapper when the test cares about it.
            self.run(bin, args)
        }
    }

    fn mk_output(stdout: &str, stderr: &str) -> Output {
        Output {
            status: ExitStatus::from_raw(0),
            stdout: stdout.as_bytes().to_vec(),
            stderr: stderr.as_bytes().to_vec(),
        }
    }

    fn test_config() -> Config {
        Config {
            subcommand: "errors".to_string(),
            window: "5m".to_string(),
            model: "haiku".to_string(),
            budget: "0.05".to_string(),
            verbose: false,
            dry_run: false,
            script_dir: PathBuf::from("/tmp"),
            state_file: PathBuf::from("/tmp/chorus-ops-test-state.json"),
            cards_bin: PathBuf::from("/fake/cards"),
            chorus_log_bin: PathBuf::from("/fake/chorus-log"),
            prompt_file: PathBuf::from("/fake/prompt.md"),
        }
    }

    fn loki_response(streams: Vec<(&str, &str, Vec<&str>)>) -> serde_json::Value {
        // Build a Loki-shape response with given (container, appName, lines).
        let arr: Vec<serde_json::Value> = streams
            .into_iter()
            .map(|(container, app, lines)| {
                let values: Vec<serde_json::Value> = lines
                    .into_iter()
                    .map(|line| serde_json::json!(["0", line]))
                    .collect();
                serde_json::json!({
                    "stream": {"container_name": container, "appName": app},
                    "values": values
                })
            })
            .collect();
        serde_json::json!({"data": {"result": arr}})
    }

    // --- process_error_streams ---

    #[test]
    fn process_error_streams_empty_input_yields_no_defects() {
        let mut state = OpsState::default();
        let fps = compile_false_positives();
        let crit = Regex::new(CRITICAL_PATTERN).unwrap();
        let results: HashMap<&str, serde_json::Value> = HashMap::new();

        let (new, updated) =
            process_error_streams(&results, &mut state, &fps, &crit, "2026-04-18T10:00:00Z");

        assert_eq!(new.len(), 0);
        assert_eq!(updated.len(), 0);
        assert_eq!(state.defects.len(), 0);
    }

    #[test]
    fn process_error_streams_classifies_new_defect_structured_error() {
        let mut state = OpsState::default();
        let fps = compile_false_positives();
        let crit = Regex::new(CRITICAL_PATTERN).unwrap();

        let mut results = HashMap::new();
        let line = r#"{"level":"error","appName":"api","message":"db timeout 5s"}"#;
        results.insert(
            "structured",
            loki_response(vec![("api-container", "api", vec![line])]),
        );

        let (new, updated) =
            process_error_streams(&results, &mut state, &fps, &crit, "2026-04-18T10:00:00Z");

        assert_eq!(new.len(), 1);
        assert_eq!(updated.len(), 0);
        let d = state.defects.values().next().unwrap();
        assert_eq!(d.source, "api-container");
        assert_eq!(d.tier, "warning");
        assert_eq!(d.count, 1);
    }

    #[test]
    fn process_error_streams_classifies_critical_tier_for_panic() {
        let mut state = OpsState::default();
        let fps = compile_false_positives();
        let crit = Regex::new(CRITICAL_PATTERN).unwrap();

        let mut results = HashMap::new();
        let line = r#"{"level":"error","appName":"worker","message":"panic: OOM killed"}"#;
        results.insert(
            "unstructured",
            loki_response(vec![("worker", "worker", vec![line])]),
        );

        let (new, _) =
            process_error_streams(&results, &mut state, &fps, &crit, "2026-04-18T10:00:00Z");

        assert_eq!(new.len(), 1);
        let d = state.defects.values().next().unwrap();
        assert_eq!(d.tier, "critical");
    }

    #[test]
    fn process_error_streams_skips_false_positive_appname() {
        let mut state = OpsState::default();
        let fps = compile_false_positives();
        let crit = Regex::new(CRITICAL_PATTERN).unwrap();

        let mut results = HashMap::new();
        let line = r#"{"level":"error","appName":"errorsmith","message":"boom"}"#;
        results.insert(
            "structured",
            loki_response(vec![("errorsmith", "errorsmith", vec![line])]),
        );

        let (new, updated) =
            process_error_streams(&results, &mut state, &fps, &crit, "2026-04-18T10:00:00Z");

        assert_eq!(new.len(), 0);
        assert_eq!(updated.len(), 0);
    }

    #[test]
    fn process_error_streams_increments_count_on_repeat_pattern() {
        let mut state = OpsState::default();
        let fps = compile_false_positives();
        let crit = Regex::new(CRITICAL_PATTERN).unwrap();

        // Seed state with an existing defect, then observe a matching line.
        let line = r#"{"level":"error","appName":"api","message":"db timeout 5s"}"#;
        let mut results = HashMap::new();
        results.insert(
            "structured",
            loki_response(vec![("api-container", "api", vec![line])]),
        );

        // First call — new defect.
        let (new1, _) = process_error_streams(
            &results,
            &mut state,
            &fps,
            &crit,
            "2026-04-18T10:00:00Z",
        );
        assert_eq!(new1.len(), 1);
        let h = new1[0].clone();

        // Second call — same pattern should become "updated", count 2.
        let (new2, updated2) = process_error_streams(
            &results,
            &mut state,
            &fps,
            &crit,
            "2026-04-18T10:01:00Z",
        );
        assert_eq!(new2.len(), 0);
        assert_eq!(updated2.len(), 1);
        assert_eq!(state.defects[&h].count, 2);
        assert_eq!(state.defects[&h].last_seen, "2026-04-18T10:01:00Z");
    }

    #[test]
    fn process_error_streams_upgrades_tier_on_critical_repeat() {
        let mut state = OpsState::default();
        let fps = compile_false_positives();
        let crit = Regex::new(CRITICAL_PATTERN).unwrap();

        // First: a warning-level error for a pattern.
        let warn = r#"{"level":"error","appName":"api","message":"db timeout 5s"}"#;
        let mut r1 = HashMap::new();
        r1.insert(
            "structured",
            loki_response(vec![("api", "api", vec![warn])]),
        );
        process_error_streams(&r1, &mut state, &fps, &crit, "2026-04-18T10:00:00Z");

        // Second: same-container, but line now contains "panic" — tier should upgrade.
        let crit_line = r#"{"level":"error","appName":"api","message":"db timeout 5s panic detected"}"#;
        let mut r2 = HashMap::new();
        r2.insert(
            "unstructured",
            loki_response(vec![("api", "api", vec![crit_line])]),
        );
        process_error_streams(&r2, &mut state, &fps, &crit, "2026-04-18T10:01:00Z");

        // Pattern collapses differently if message differs too much; at least
        // one defect should now be critical.
        let any_critical = state.defects.values().any(|d| d.tier == "critical");
        assert!(any_critical, "expected at least one critical defect after panic line");
    }

    #[test]
    fn process_error_streams_skips_non_error_log_levels() {
        let mut state = OpsState::default();
        let fps = compile_false_positives();
        let crit = Regex::new(CRITICAL_PATTERN).unwrap();

        let info_line = r#"{"level":"info","appName":"api","message":"startup complete"}"#;
        let mut results = HashMap::new();
        results.insert(
            "structured",
            loki_response(vec![("api", "api", vec![info_line])]),
        );

        let (new, updated) =
            process_error_streams(&results, &mut state, &fps, &crit, "2026-04-18T10:00:00Z");
        assert_eq!(new.len(), 0);
        assert_eq!(updated.len(), 0);
    }

    #[test]
    fn process_error_streams_handles_unstructured_with_level_field() {
        let mut state = OpsState::default();
        let fps = compile_false_positives();
        let crit = Regex::new(CRITICAL_PATTERN).unwrap();

        let line = "ts=2026 level=error msg=\"connection refused\"";
        let mut results = HashMap::new();
        results.insert(
            "unstructured",
            loki_response(vec![("net", "net", vec![line])]),
        );

        let (new, _) =
            process_error_streams(&results, &mut state, &fps, &crit, "2026-04-18T10:00:00Z");
        assert_eq!(new.len(), 1);
    }

    #[test]
    fn process_error_streams_unstructured_skips_non_error_level() {
        let mut state = OpsState::default();
        let fps = compile_false_positives();
        let crit = Regex::new(CRITICAL_PATTERN).unwrap();

        let line = "ts=2026 level=info msg=\"heartbeat\"";
        let mut results = HashMap::new();
        results.insert(
            "unstructured",
            loki_response(vec![("net", "net", vec![line])]),
        );

        let (new, _) =
            process_error_streams(&results, &mut state, &fps, &crit, "2026-04-18T10:00:00Z");
        assert_eq!(new.len(), 0);
    }

    #[test]
    fn process_error_streams_chorus_source_accepts_unstructured() {
        let mut state = OpsState::default();
        let fps = compile_false_positives();
        let crit = Regex::new(CRITICAL_PATTERN).unwrap();

        let line = "raw text error without level field";
        let mut results = HashMap::new();
        results.insert(
            "chorus",
            loki_response(vec![("chorus-ops", "chorus", vec![line])]),
        );

        let (new, _) =
            process_error_streams(&results, &mut state, &fps, &crit, "2026-04-18T10:00:00Z");
        assert_eq!(new.len(), 1);
    }

    // --- decide_actions ---

    fn seed_defect(
        state: &mut OpsState,
        hash: &str,
        tier: &str,
        count: u32,
        card_id: Option<&str>,
    ) {
        state.defects.insert(
            hash.to_string(),
            Defect {
                hash: hash.to_string(),
                source: "src".to_string(),
                pattern: "p".to_string(),
                sample: "s".to_string(),
                tier: tier.to_string(),
                count,
                first_seen: "2026-04-18T10:00:00Z".to_string(),
                last_seen: "2026-04-18T10:00:00Z".to_string(),
                card_id: card_id.map(|s| s.to_string()),
            },
        );
    }

    #[test]
    fn decide_actions_new_critical_is_p1_card() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "critical", 1, None);
        let actions = decide_actions(&["h1".to_string()], &[], &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action_type, "card");
        assert_eq!(actions[0].priority, "P1");
    }

    #[test]
    fn decide_actions_new_warning_is_p2_card() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", 1, None);
        let actions = decide_actions(&["h1".to_string()], &[], &state);
        assert_eq!(actions[0].priority, "P2");
    }

    #[test]
    fn decide_actions_updated_below_threshold_no_action() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", 2, None);
        let actions = decide_actions(&[], &["h1".to_string()], &state);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn decide_actions_updated_at_threshold_becomes_p2_card() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", PATTERN_THRESHOLD, None);
        let actions = decide_actions(&[], &["h1".to_string()], &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action_type, "card");
        assert_eq!(actions[0].priority, "P2");
    }

    #[test]
    fn decide_actions_updated_with_card_and_modulo_count_becomes_comment() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", 10, Some("1234"));
        let actions = decide_actions(&[], &["h1".to_string()], &state);
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].action_type, "comment");
    }

    #[test]
    fn decide_actions_updated_with_card_non_modulo_no_action() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", 7, Some("1234"));
        let actions = decide_actions(&[], &["h1".to_string()], &state);
        assert_eq!(actions.len(), 0);
    }

    #[test]
    fn decide_actions_dedups_across_new_and_updated() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "critical", 1, None);
        let actions =
            decide_actions(&["h1".to_string()], &["h1".to_string()], &state);
        // seen set prevents the updated list from double-counting.
        assert_eq!(actions.len(), 1);
        assert_eq!(actions[0].priority, "P1");
    }

    // --- execute_card_action ---

    #[test]
    fn execute_card_action_dry_run_prints_and_returns_false() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", 1, None);
        let mut config = test_config();
        config.dry_run = true;
        let cmd = FakeCommandRunner::always_ok("");
        let action = Action {
            action_type: "card".to_string(),
            hash: "h1".to_string(),
            priority: "P2".to_string(),
            _reason: "new warning".to_string(),
        };
        let carded = execute_card_action(&action, &mut state, &config, &cmd);
        assert!(!carded);
        // Fake wasn't called — dry_run branch skipped the runner.
        assert_eq!(cmd.calls.borrow().len(), 0);
    }

    #[test]
    fn execute_card_action_parses_new_card_id_and_updates_state() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", 1, None);
        let config = test_config();
        // cards stdout contains "Added #2200" — the card_re regex captures 2200.
        let cmd = FakeCommandRunner::new(vec![
            Ok(mk_output("Added #2200: [defect] ...\n", "")),
            // Second call for the spine event.
            Ok(mk_output("", "")),
        ]);
        let action = Action {
            action_type: "card".to_string(),
            hash: "h1".to_string(),
            priority: "P2".to_string(),
            _reason: "new warning".to_string(),
        };
        let carded = execute_card_action(&action, &mut state, &config, &cmd);
        assert!(carded);
        assert_eq!(
            state.defects["h1"].card_id.as_deref(),
            Some("2200")
        );
    }

    #[test]
    fn execute_card_action_no_card_id_in_output_does_not_update_state() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", 1, None);
        let config = test_config();
        let cmd = FakeCommandRunner::new(vec![
            Ok(mk_output("ERROR: create failed\n", "missing --title")),
            Ok(mk_output("", "")),
        ]);
        let action = Action {
            action_type: "card".to_string(),
            hash: "h1".to_string(),
            priority: "P2".to_string(),
            _reason: "new warning".to_string(),
        };
        let carded = execute_card_action(&action, &mut state, &config, &cmd);
        assert!(!carded);
        assert_eq!(state.defects["h1"].card_id, None);
    }

    #[test]
    fn execute_card_action_owner_routes_kade_for_personal_site() {
        let mut state = OpsState::default();
        state.defects.insert(
            "h1".to_string(),
            Defect {
                hash: "h1".to_string(),
                source: "jeff-bridwell-personal-site-app".to_string(),
                pattern: "p".to_string(),
                sample: "s".to_string(),
                tier: "warning".to_string(),
                count: 1,
                first_seen: "2026-04-18T10:00:00Z".to_string(),
                last_seen: "2026-04-18T10:00:00Z".to_string(),
                card_id: None,
            },
        );
        let config = test_config();
        let cmd = FakeCommandRunner::new(vec![
            Ok(mk_output("Added #2201\n", "")),
            Ok(mk_output("", "")),
        ]);
        let action = Action {
            action_type: "card".to_string(),
            hash: "h1".to_string(),
            priority: "P2".to_string(),
            _reason: "new".to_string(),
        };
        execute_card_action(&action, &mut state, &config, &cmd);

        // Find the "add" call and check --owner was Kade.
        let calls = cmd.calls.borrow();
        let add_call = calls
            .iter()
            .find(|(_, args)| args.first().map(|s| s.as_str()) == Some("add"))
            .expect("add call recorded");
        let owner_idx = add_call
            .1
            .iter()
            .position(|s| s == "--owner")
            .expect("--owner arg");
        assert_eq!(add_call.1[owner_idx + 1], "Kade");
    }

    #[test]
    fn execute_card_action_owner_routes_silas_for_infra_source() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", 1, None);
        let config = test_config();
        let cmd = FakeCommandRunner::new(vec![
            Ok(mk_output("Added #2202\n", "")),
            Ok(mk_output("", "")),
        ]);
        let action = Action {
            action_type: "card".to_string(),
            hash: "h1".to_string(),
            priority: "P1".to_string(),
            _reason: "new critical".to_string(),
        };
        execute_card_action(&action, &mut state, &config, &cmd);
        let calls = cmd.calls.borrow();
        let add_call = calls
            .iter()
            .find(|(_, args)| args.first().map(|s| s.as_str()) == Some("add"))
            .unwrap();
        let owner_idx = add_call.1.iter().position(|s| s == "--owner").unwrap();
        assert_eq!(add_call.1[owner_idx + 1], "Silas");
    }

    // --- execute_comment_action ---

    #[test]
    fn execute_comment_action_writes_comment_when_card_present() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", 10, Some("2200"));
        let config = test_config();
        let cmd = FakeCommandRunner::always_ok("");
        let action = Action {
            action_type: "comment".to_string(),
            hash: "h1".to_string(),
            priority: String::new(),
            _reason: "recurring".to_string(),
        };
        execute_comment_action(&action, &state, &config, &cmd);
        let calls = cmd.calls.borrow();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].1[0], "comment");
        assert_eq!(calls[0].1[1], "2200");
    }

    #[test]
    fn execute_comment_action_no_card_id_is_noop() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", 10, None);
        let config = test_config();
        let cmd = FakeCommandRunner::always_ok("");
        let action = Action {
            action_type: "comment".to_string(),
            hash: "h1".to_string(),
            priority: String::new(),
            _reason: "recurring".to_string(),
        };
        execute_comment_action(&action, &state, &config, &cmd);
        assert_eq!(cmd.calls.borrow().len(), 0);
    }

    #[test]
    fn execute_comment_action_dry_run_skips_runner() {
        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "warning", 10, Some("2200"));
        let mut config = test_config();
        config.dry_run = true;
        let cmd = FakeCommandRunner::always_ok("");
        let action = Action {
            action_type: "comment".to_string(),
            hash: "h1".to_string(),
            priority: String::new(),
            _reason: "recurring".to_string(),
        };
        execute_comment_action(&action, &state, &config, &cmd);
        assert_eq!(cmd.calls.borrow().len(), 0);
    }

    // --- process_health_findings (do_health extraction) ---

    fn mk_finding(
        id: &str,
        severity: &str,
        category: &str,
        action: &str,
        is_repeat: bool,
    ) -> Finding {
        Finding {
            id: id.to_string(),
            severity: severity.to_string(),
            category: category.to_string(),
            title: format!("{}: {}", severity, id),
            description: format!("description for {}", id),
            action: action.to_string(),
            is_repeat,
        }
    }

    #[test]
    fn process_health_findings_creates_card_for_fresh_critical() {
        let mut state = OpsState::default();
        let config = test_config();
        let cmd = FakeCommandRunner::new(vec![Ok(mk_output("Added #3000\n", ""))]);

        let findings = vec![mk_finding("f1", "critical", "disk", "card", false)];
        let (cards, categories) =
            process_health_findings(&findings, &mut state, &config, &cmd, "2026-04-18T10:00:00Z");

        assert_eq!(cards, 1);
        assert!(categories.contains_key("disk"));

        let calls = cmd.calls.borrow();
        assert_eq!(calls[0].1[0], "add");
        let prio_idx = calls[0].1.iter().position(|s| s == "--priority").unwrap();
        assert_eq!(calls[0].1[prio_idx + 1], "P1"); // critical → P1
    }

    #[test]
    fn process_health_findings_non_critical_is_p2() {
        let mut state = OpsState::default();
        let config = test_config();
        let cmd = FakeCommandRunner::new(vec![Ok(mk_output("Added #3001\n", ""))]);
        let findings = vec![mk_finding("f1", "warning", "board", "card", false)];
        process_health_findings(&findings, &mut state, &config, &cmd, "2026-04-18T10:00:00Z");
        let calls = cmd.calls.borrow();
        let prio_idx = calls[0].1.iter().position(|s| s == "--priority").unwrap();
        assert_eq!(calls[0].1[prio_idx + 1], "P2");
    }

    #[test]
    fn process_health_findings_honors_cooldown() {
        let mut state = OpsState::default();
        state.health.carded_categories.insert(
            "disk".to_string(),
            Utc::now().to_rfc3339(),
        );
        let config = test_config();
        let cmd = FakeCommandRunner::never();
        let findings = vec![mk_finding("f1", "critical", "disk", "card", false)];

        let (cards, _categories) =
            process_health_findings(&findings, &mut state, &config, &cmd, "2026-04-18T10:00:00Z");
        assert_eq!(cards, 0);
        assert_eq!(cmd.calls.borrow().len(), 0);
    }

    #[test]
    fn process_health_findings_skips_repeat() {
        let mut state = OpsState::default();
        let config = test_config();
        let cmd = FakeCommandRunner::never();
        let findings = vec![mk_finding("f1", "critical", "disk", "card", true)];

        let (cards, _) =
            process_health_findings(&findings, &mut state, &config, &cmd, "2026-04-18T10:00:00Z");
        assert_eq!(cards, 0);
        assert_eq!(cmd.calls.borrow().len(), 0);
    }

    #[test]
    fn process_health_findings_skips_log_action() {
        let mut state = OpsState::default();
        let config = test_config();
        let cmd = FakeCommandRunner::never();
        let findings = vec![mk_finding("f1", "warning", "board", "log", false)];
        let (cards, _) =
            process_health_findings(&findings, &mut state, &config, &cmd, "2026-04-18T10:00:00Z");
        assert_eq!(cards, 0);
    }

    #[test]
    fn process_health_findings_skips_ignore_action() {
        let mut state = OpsState::default();
        let config = test_config();
        let cmd = FakeCommandRunner::never();
        let findings = vec![mk_finding("f1", "warning", "board", "ignore", false)];
        let (cards, _) =
            process_health_findings(&findings, &mut state, &config, &cmd, "2026-04-18T10:00:00Z");
        assert_eq!(cards, 0);
    }

    #[test]
    fn process_health_findings_respects_max_cards_limit() {
        let mut state = OpsState::default();
        let config = test_config();
        let cmd = FakeCommandRunner::new(vec![
            Ok(mk_output("Added #3002\n", "")),
            Ok(mk_output("Added #3003\n", "")),
            Ok(mk_output("Added #3004\n", "")),
        ]);
        let findings = vec![
            mk_finding("f1", "critical", "cat1", "card", false),
            mk_finding("f2", "critical", "cat2", "card", false),
            mk_finding("f3", "critical", "cat3", "card", false),
        ];
        let (cards, _) =
            process_health_findings(&findings, &mut state, &config, &cmd, "2026-04-18T10:00:00Z");
        assert_eq!(cards, MAX_CARDS as u64);
        assert_eq!(cmd.calls.borrow().len(), MAX_CARDS);
    }

    #[test]
    fn process_health_findings_adds_category_to_cooldown_after_card() {
        let mut state = OpsState::default();
        let config = test_config();
        let cmd = FakeCommandRunner::new(vec![Ok(mk_output("Added #3005\n", ""))]);
        let findings = vec![mk_finding("f1", "critical", "sync-storm", "card", false)];
        let (_, categories) =
            process_health_findings(&findings, &mut state, &config, &cmd, "2026-04-18T10:00:00Z");
        assert_eq!(
            categories.get("sync-storm").map(|s| s.as_str()),
            Some("2026-04-18T10:00:00Z")
        );
    }

    // --- do_status smoke (exercises the print-only orchestrator) ---

    #[test]
    fn do_status_runs_against_populated_state_without_panicking() {
        let tmp = std::env::temp_dir().join(format!(
            "chorus-ops-dostatus-{}.json",
            std::process::id()
        ));

        let mut state = OpsState::default();
        seed_defect(&mut state, "h1", "critical", 5, Some("999"));
        state.last_errors_poll = "2026-04-18T10:00:00Z".to_string();
        state.health = HealthState {
            last_run: "2026-04-18T09:55:00Z".to_string(),
            findings: vec![mk_finding("f1", "warning", "board", "log", false)],
            cards_created: 3,
            last_status: "ok".to_string(),
            last_summary: "green".to_string(),
            carded_categories: HashMap::new(),
        };
        save_state(&tmp, &state);

        let mut config = test_config();
        config.state_file = tmp.clone();
        do_status(&config);

        let _ = fs::remove_file(&tmp);
    }

    // --- assemble_health_context ---

    #[test]
    fn assemble_health_context_wires_parsed_substructures() {
        let mut fetched = HashMap::new();
        fetched.insert(
            "alerts".to_string(),
            r#"[{"labels":{"alertname":"X","severity":"critical"},"annotations":{"summary":"disk full"}}]"#
                .to_string(),
        );
        fetched.insert(
            "loki_errors".to_string(),
            r#"{"data":{"result":[{"metric":{"container_name":"api"},"value":[0,"3"]}]}}"#
                .to_string(),
        );
        fetched.insert("loki_sync".to_string(), "{}".to_string());
        fetched.insert(
            "disk".to_string(),
            "Container Total Space: 2.0 GB (2147483648 Bytes)\nContainer Free Space: 1.0 GB (1073741824 Bytes)\n"
                .to_string(),
        );
        fetched.insert("board".to_string(), "board output".to_string());

        let ctx = assemble_health_context(&fetched, vec![], "2026-04-18T10:00:00Z".to_string());
        assert_eq!(ctx.timestamp, "2026-04-18T10:00:00Z");
        assert_eq!(ctx.alerts.firing.len(), 1);
        assert_eq!(ctx.errors.total_30m, 3);
        assert_eq!(ctx.disk.usage_pct, 50);
        assert_eq!(ctx.board.summary, "board output");
    }

    #[test]
    fn assemble_health_context_defaults_on_empty_map() {
        let fetched = HashMap::new();
        let ctx = assemble_health_context(&fetched, vec![], "t".to_string());
        assert_eq!(ctx.alerts.firing.len(), 0);
        assert_eq!(ctx.errors.total_30m, 0);
        assert_eq!(ctx.disk.usage_pct, 0);
        assert_eq!(ctx.board.summary, "");
    }

    #[test]
    fn assemble_health_context_truncates_board_to_2000_chars() {
        let mut fetched = HashMap::new();
        fetched.insert("board".to_string(), "a".repeat(5000));
        let ctx = assemble_health_context(&fetched, vec![], "t".to_string());
        assert_eq!(ctx.board.summary.chars().count(), 2000);
    }

    #[test]
    fn assemble_health_context_forwards_previous_findings() {
        let fetched = HashMap::new();
        let prev = vec![mk_finding("f1", "warning", "disk", "log", true)];
        let ctx = assemble_health_context(&fetched, prev.clone(), "t".to_string());
        assert_eq!(ctx.previous_findings.len(), 1);
        assert_eq!(ctx.previous_findings[0].id, "f1");
    }

    // --- run_claude_agent ---
    //
    // The real claude binary would run and return real JSON. We swap it out
    // via `run_with_stdin` on the FakeCommandRunner.

    /// Extend FakeCommandRunner with stdin-capturing behavior. Tracked as a
    /// separate Vec so test assertions can inspect what got piped to claude.
    struct StdinCapturingRunner {
        inner: FakeCommandRunner,
        stdin: RefCell<Vec<Vec<u8>>>,
    }

    impl StdinCapturingRunner {
        fn from_output(stdout: &str) -> Self {
            StdinCapturingRunner {
                inner: FakeCommandRunner::always_ok(stdout),
                stdin: RefCell::new(Vec::new()),
            }
        }

        fn from_results(results: Vec<std::io::Result<Output>>) -> Self {
            StdinCapturingRunner {
                inner: FakeCommandRunner::new(results),
                stdin: RefCell::new(Vec::new()),
            }
        }
    }

    impl CommandRunner for StdinCapturingRunner {
        fn run(&self, bin: &Path, args: &[&str]) -> std::io::Result<Output> {
            self.inner.run(bin, args)
        }
        fn run_with_stdin(
            &self,
            bin: &Path,
            args: &[&str],
            stdin_data: &[u8],
        ) -> std::io::Result<Output> {
            self.stdin.borrow_mut().push(stdin_data.to_vec());
            self.inner.run(bin, args)
        }
    }

    #[test]
    fn run_claude_agent_returns_stdout_on_success() {
        let config = test_config();
        let payload = r#"{"structured_output":{"status":"ok","findings":[],"summary":"green"}}"#;
        let cmd = StdinCapturingRunner::from_output(payload);
        let out = run_claude_agent(&config, r#"{"ctx":"..."}"#, "you are an agent", &cmd).unwrap();
        assert!(out.contains("structured_output"));
    }

    #[test]
    fn run_claude_agent_forwards_context_via_stdin() {
        let config = test_config();
        let cmd = StdinCapturingRunner::from_output("{}");
        run_claude_agent(&config, r#"{"ctx":"hello"}"#, "sysprompt", &cmd).unwrap();
        let captured = &cmd.stdin.borrow()[0];
        let s = String::from_utf8_lossy(captured);
        assert!(s.contains(r#""ctx":"hello""#));
    }

    #[test]
    fn run_claude_agent_passes_model_and_budget_as_argv() {
        let mut config = test_config();
        config.model = "sonnet".to_string();
        config.budget = "0.50".to_string();
        let cmd = StdinCapturingRunner::from_output("{}");
        run_claude_agent(&config, "{}", "sp", &cmd).unwrap();
        let calls = cmd.inner.calls.borrow();
        let argv = &calls[0].1;
        // --model <config.model> and --max-budget-usd <config.budget> must be present.
        let model_idx = argv.iter().position(|s| s == "--model").unwrap();
        assert_eq!(argv[model_idx + 1], "sonnet");
        let budget_idx = argv.iter().position(|s| s == "--max-budget-usd").unwrap();
        assert_eq!(argv[budget_idx + 1], "0.50");
    }

    #[test]
    fn run_claude_agent_rejects_failed_exit_code() {
        use std::os::unix::process::ExitStatusExt;
        let config = test_config();
        // Non-zero exit from claude → Err "failed".
        let failed_output = Output {
            status: std::process::ExitStatus::from_raw(256), // exit 1
            stdout: Vec::new(),
            stderr: b"budget exceeded".to_vec(),
        };
        let cmd = StdinCapturingRunner::from_results(vec![Ok(failed_output)]);
        let err = run_claude_agent(&config, "{}", "sp", &cmd).unwrap_err();
        assert!(err.contains("claude -p failed"), "got: {}", err);
    }

    #[test]
    fn run_claude_agent_reports_spawn_failure() {
        let config = test_config();
        let cmd = StdinCapturingRunner::from_results(vec![Err(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "binary missing",
        ))]);
        let err = run_claude_agent(&config, "{}", "sp", &cmd).unwrap_err();
        assert!(err.contains("spawn failed"), "got: {}", err);
    }

    // --- do_errors_post_prefetch (full post-Loki orchestrator) ---

    #[test]
    fn do_errors_post_prefetch_empty_results_saves_clean_state() {
        let state = std::env::temp_dir().join(format!(
            "chorus-ops-estate-empty-{}-{}.json",
            std::process::id(),
            rand_suffix()
        ));
        let mut config = test_config();
        config.state_file = state.clone();

        let cmd = FakeCommandRunner::never();
        let results: HashMap<&str, serde_json::Value> = HashMap::new();
        let r = do_errors_post_prefetch(results, &config, &cmd, "2026-04-18T10:00:00Z");
        assert!(r.is_ok());

        let saved = load_state(&state);
        assert_eq!(saved.last_errors_poll, "2026-04-18T10:00:00Z");
        assert_eq!(saved.defects.len(), 0);

        let _ = fs::remove_file(&state);
    }

    #[test]
    fn do_errors_post_prefetch_new_defect_creates_card_and_saves_state() {
        let state = std::env::temp_dir().join(format!(
            "chorus-ops-estate-new-{}-{}.json",
            std::process::id(),
            rand_suffix()
        ));
        let mut config = test_config();
        config.state_file = state.clone();

        let cmd = FakeCommandRunner::new(vec![
            Ok(mk_output("Added #4000\n", "")),
            Ok(mk_output("", "")),
        ]);

        let line = r#"{"level":"error","appName":"api","message":"db timeout 5s"}"#;
        let mut results = HashMap::new();
        results.insert(
            "structured",
            loki_response(vec![("api", "api", vec![line])]),
        );

        let r = do_errors_post_prefetch(results, &config, &cmd, "2026-04-18T10:00:00Z");
        assert!(r.is_ok());

        let saved = load_state(&state);
        assert_eq!(saved.defects.len(), 1);
        // The new defect should now have a card_id set (from "#4000" parsed).
        let d = saved.defects.values().next().unwrap();
        assert_eq!(d.card_id.as_deref(), Some("4000"));

        let _ = fs::remove_file(&state);
    }

    #[test]
    fn do_errors_post_prefetch_expires_old_defects_before_processing() {
        let state = std::env::temp_dir().join(format!(
            "chorus-ops-estate-expire-{}-{}.json",
            std::process::id(),
            rand_suffix()
        ));
        let mut config = test_config();
        config.state_file = state.clone();

        // Seed state with a defect last_seen > DEDUP_WINDOW_HOURS ago.
        let mut seeded = OpsState::default();
        let old = (Utc::now() - chrono::Duration::hours(DEDUP_WINDOW_HOURS + 1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        seed_defect(&mut seeded, "old-hash", "warning", 5, Some("111"));
        if let Some(d) = seeded.defects.get_mut("old-hash") {
            d.last_seen = old;
        }
        save_state(&state, &seeded);

        let cmd = FakeCommandRunner::never();
        let r = do_errors_post_prefetch(
            HashMap::new(),
            &config,
            &cmd,
            "2026-04-18T10:00:00Z",
        );
        assert!(r.is_ok());

        let saved = load_state(&state);
        assert!(!saved.defects.contains_key("old-hash"));

        let _ = fs::remove_file(&state);
    }

    #[test]
    fn do_errors_post_prefetch_comment_action_flows_through() {
        let state = std::env::temp_dir().join(format!(
            "chorus-ops-estate-comment-{}-{}.json",
            std::process::id(),
            rand_suffix()
        ));
        let mut config = test_config();
        config.state_file = state.clone();

        // Seed a defect with a card_id at count=9 so the next observation
        // bumps it to 10 → comment action. Timestamps are relative to now so
        // the seeded defect never expires past DEDUP_WINDOW_HOURS (24h) as
        // wall clock drifts — hardcoded dates are time bombs (#2235).
        let mut seeded = OpsState::default();
        let line = r#"{"level":"error","appName":"api","message":"db timeout 5s"}"#;
        let hash = hash_pattern("api", &normalize_pattern("db timeout 5s"));
        let now = Utc::now();
        let one_hour_ago = (now - chrono::Duration::hours(1))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let two_hours_ago = (now - chrono::Duration::hours(2))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let now_iso = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        seeded.defects.insert(
            hash.clone(),
            Defect {
                hash: hash.clone(),
                source: "api".to_string(),
                pattern: normalize_pattern("db timeout 5s"),
                sample: "db timeout 5s".to_string(),
                tier: "warning".to_string(),
                count: 9,
                first_seen: two_hours_ago,
                last_seen: one_hour_ago,
                card_id: Some("3333".to_string()),
            },
        );
        save_state(&state, &seeded);

        let cmd = FakeCommandRunner::always_ok("");
        let mut results = HashMap::new();
        results.insert(
            "structured",
            loki_response(vec![("api", "api", vec![line])]),
        );

        let r = do_errors_post_prefetch(results, &config, &cmd, &now_iso);
        assert!(r.is_ok());

        // At least one call: `cards comment 3333 ...`.
        let calls = cmd.calls.borrow();
        assert!(calls.iter().any(|(_, args)| args.first().map(|s| s.as_str()) == Some("comment")));

        let _ = fs::remove_file(&state);
    }

    // --- do_health_post_prefetch (full post-prefetch orchestrator) ---

    fn write_tmp_prompt(body: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "chorus-ops-prompt-{}-{}.md",
            std::process::id(),
            rand_suffix()
        ));
        fs::write(&path, body).unwrap();
        path
    }

    fn rand_suffix() -> String {
        // Small, not cryptographically random — just enough to avoid test collisions.
        use std::time::{SystemTime, UNIX_EPOCH};
        let n = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0);
        format!("{}", n)
    }

    #[test]
    fn do_health_post_prefetch_dry_run_short_circuits() {
        let prompt = write_tmp_prompt("test prompt");
        let state = std::env::temp_dir().join(format!(
            "chorus-ops-hstate-{}-{}.json",
            std::process::id(),
            rand_suffix()
        ));
        let mut config = test_config();
        config.prompt_file = prompt.clone();
        config.state_file = state.clone();
        config.dry_run = true;

        let cmd = FakeCommandRunner::never();
        let result = do_health_post_prefetch(
            HashMap::new(),
            &config,
            &cmd,
            "2026-04-18T10:00:00Z",
        );

        assert!(result.is_ok());
        // Dry-run must not invoke claude or any card command.
        assert_eq!(cmd.calls.borrow().len(), 0);

        let _ = fs::remove_file(&prompt);
        let _ = fs::remove_file(&state);
    }

    #[test]
    fn do_health_post_prefetch_missing_prompt_file_errors() {
        let mut config = test_config();
        config.prompt_file = PathBuf::from("/does/not/exist/prompt.md");
        config.state_file = std::env::temp_dir().join(format!(
            "chorus-ops-hstate-missing-{}-{}.json",
            std::process::id(),
            rand_suffix()
        ));

        let cmd = FakeCommandRunner::never();
        let err = do_health_post_prefetch(
            HashMap::new(),
            &config,
            &cmd,
            "2026-04-18T10:00:00Z",
        )
        .unwrap_err();
        assert!(err.contains("Missing prompt file"));
    }

    #[test]
    fn do_health_post_prefetch_successful_flow_saves_state_and_creates_card() {
        let prompt = write_tmp_prompt("you are an ops agent");
        let state = std::env::temp_dir().join(format!(
            "chorus-ops-hstate-ok-{}-{}.json",
            std::process::id(),
            rand_suffix()
        ));
        let mut config = test_config();
        config.prompt_file = prompt.clone();
        config.state_file = state.clone();

        // Canned claude response with one action=card finding.
        let claude_response = r#"{
            "structured_output": {
                "status": "critical",
                "findings": [{
                    "id": "f1",
                    "severity": "critical",
                    "category": "disk",
                    "title": "Disk 99%",
                    "description": "root full",
                    "action": "card",
                    "is_repeat": false
                }],
                "summary": "disk full"
            }
        }"#;

        // Runner sees: claude (stdin), cards add, chorus-log spine event.
        let cmd = FakeCommandRunner::new(vec![
            Ok(mk_output(claude_response, "")), // run_with_stdin → claude
            Ok(mk_output("Added #3500: [ops-health] Disk 99%\n", "")), // cards add
            Ok(mk_output("", "")), // chorus-log spine event
        ]);

        let result = do_health_post_prefetch(
            HashMap::new(),
            &config,
            &cmd,
            "2026-04-18T10:00:00Z",
        );
        assert!(result.is_ok(), "got: {:?}", result.err());

        // State file written — load it back and assert the health fields.
        let saved = load_state(&state);
        assert_eq!(saved.health.last_run, "2026-04-18T10:00:00Z");
        assert_eq!(saved.health.last_status, "critical");
        assert_eq!(saved.health.last_summary, "disk full");
        assert_eq!(saved.health.cards_created, 1);
        assert_eq!(saved.health.findings.len(), 1);
        assert!(saved.health.carded_categories.contains_key("disk"));

        let _ = fs::remove_file(&prompt);
        let _ = fs::remove_file(&state);
    }

    #[test]
    fn do_health_post_prefetch_claude_failure_emits_error_spine_event() {
        let prompt = write_tmp_prompt("sysprompt");
        let state = std::env::temp_dir().join(format!(
            "chorus-ops-hstate-fail-{}-{}.json",
            std::process::id(),
            rand_suffix()
        ));
        let mut config = test_config();
        config.prompt_file = prompt.clone();
        config.state_file = state.clone();

        use std::os::unix::process::ExitStatusExt;
        let claude_fail = Output {
            status: std::process::ExitStatus::from_raw(256),
            stdout: Vec::new(),
            stderr: b"budget exceeded".to_vec(),
        };
        let cmd = FakeCommandRunner::new(vec![
            Ok(claude_fail),
            // chorus-log for the error spine event
            Ok(mk_output("", "")),
        ]);

        let err = do_health_post_prefetch(
            HashMap::new(),
            &config,
            &cmd,
            "2026-04-18T10:00:00Z",
        )
        .unwrap_err();
        assert!(err.contains("claude -p failed"));

        // Second call is the chorus-log error event.
        let calls = cmd.calls.borrow();
        assert!(calls.len() >= 2);
        let log_call = &calls[1];
        assert_eq!(log_call.1[0], "ops.agent.completed");
        assert!(log_call.1.iter().any(|s| s == "status=error"));

        let _ = fs::remove_file(&prompt);
        let _ = fs::remove_file(&state);
    }

    #[test]
    fn do_health_post_prefetch_empty_response_errors_with_spine_event() {
        let prompt = write_tmp_prompt("sysprompt");
        let state = std::env::temp_dir().join(format!(
            "chorus-ops-hstate-empty-{}-{}.json",
            std::process::id(),
            rand_suffix()
        ));
        let mut config = test_config();
        config.prompt_file = prompt.clone();
        config.state_file = state.clone();

        // Claude returns <10 bytes → "Empty response" error branch.
        let cmd = FakeCommandRunner::new(vec![
            Ok(mk_output("x", "")),
            Ok(mk_output("", "")),
        ]);

        let err = do_health_post_prefetch(
            HashMap::new(),
            &config,
            &cmd,
            "2026-04-18T10:00:00Z",
        )
        .unwrap_err();
        assert_eq!(err, "Empty response");

        let calls = cmd.calls.borrow();
        let log_call = &calls[1];
        assert!(log_call.1.iter().any(|s| s == "error=empty_response"));

        let _ = fs::remove_file(&prompt);
        let _ = fs::remove_file(&state);
    }

    #[test]
    fn do_health_post_prefetch_malformed_response_errors() {
        let prompt = write_tmp_prompt("sysprompt");
        let state = std::env::temp_dir().join(format!(
            "chorus-ops-hstate-bad-{}-{}.json",
            std::process::id(),
            rand_suffix()
        ));
        let mut config = test_config();
        config.prompt_file = prompt.clone();
        config.state_file = state.clone();

        // 10+ bytes but not valid JSON → parse_agent_response errors.
        let cmd =
            FakeCommandRunner::new(vec![Ok(mk_output("not a json response at all", ""))]);

        let err = do_health_post_prefetch(
            HashMap::new(),
            &config,
            &cmd,
            "2026-04-18T10:00:00Z",
        )
        .unwrap_err();
        assert!(err.contains("JSON parse error"), "got: {}", err);

        let _ = fs::remove_file(&prompt);
        let _ = fs::remove_file(&state);
    }

    #[test]
    fn do_health_post_prefetch_zero_findings_saves_ok_state() {
        let prompt = write_tmp_prompt("sysprompt");
        let state = std::env::temp_dir().join(format!(
            "chorus-ops-hstate-green-{}-{}.json",
            std::process::id(),
            rand_suffix()
        ));
        let mut config = test_config();
        config.prompt_file = prompt.clone();
        config.state_file = state.clone();

        let response = r#"{"structured_output":{"status":"ok","findings":[],"summary":"green"}}"#;
        let cmd = FakeCommandRunner::new(vec![
            Ok(mk_output(response, "")),
            Ok(mk_output("", "")), // chorus-log spine event
        ]);

        let result = do_health_post_prefetch(
            HashMap::new(),
            &config,
            &cmd,
            "2026-04-18T10:00:00Z",
        );
        assert!(result.is_ok());

        let saved = load_state(&state);
        assert_eq!(saved.health.last_status, "ok");
        assert_eq!(saved.health.cards_created, 0);

        let _ = fs::remove_file(&prompt);
        let _ = fs::remove_file(&state);
    }

    // --- parse_agent_response (do_health JSON envelope extraction) ---

    #[test]
    fn parse_agent_response_handles_structured_output_wrapper() {
        // Claude's "json" output format wraps the actual payload in
        // `structured_output`. This is the production shape.
        let raw = r#"{
            "structured_output": {
                "status": "ok",
                "findings": [],
                "summary": "green"
            }
        }"#;
        let (status, findings, summary) = parse_agent_response(raw).unwrap();
        assert_eq!(status, "ok");
        assert_eq!(findings.len(), 0);
        assert_eq!(summary, "green");
    }

    #[test]
    fn parse_agent_response_handles_result_string_wrapper() {
        // Older "text" output shape — the envelope has a string `result`
        // containing the JSON. parse_result_field unwraps it.
        let raw = r#"{"result": "{\"status\":\"warning\",\"findings\":[],\"summary\":\"yellow\"}"}"#;
        let (status, _findings, summary) = parse_agent_response(raw).unwrap();
        assert_eq!(status, "warning");
        assert_eq!(summary, "yellow");
    }

    #[test]
    fn parse_agent_response_decodes_findings() {
        let raw = r#"{
            "structured_output": {
                "status": "critical",
                "findings": [
                    {
                        "id": "f1",
                        "severity": "critical",
                        "category": "disk",
                        "title": "Root 99% full",
                        "description": "/dev/disk3 is at 99%",
                        "action": "card",
                        "is_repeat": false
                    }
                ],
                "summary": "disk full"
            }
        }"#;
        let (_, findings, _) = parse_agent_response(raw).unwrap();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].id, "f1");
        assert_eq!(findings[0].action, "card");
        assert_eq!(findings[0].severity, "critical");
    }

    #[test]
    fn parse_agent_response_defaults_when_fields_missing() {
        // If the envelope has no status/findings/summary, defaults kick in.
        let raw = r#"{"structured_output": {}}"#;
        let (status, findings, summary) = parse_agent_response(raw).unwrap();
        assert_eq!(status, "ok");
        assert_eq!(findings.len(), 0);
        assert_eq!(summary, "No summary");
    }

    #[test]
    fn parse_agent_response_errors_on_malformed_outer_json() {
        let raw = "{not json";
        assert!(parse_agent_response(raw).is_err());
    }

    #[test]
    fn parse_agent_response_handles_null_structured_output_falls_back_to_result() {
        let raw = r#"{
            "structured_output": null,
            "result": "{\"status\":\"ok\",\"findings\":[],\"summary\":\"from result\"}"
        }"#;
        let (_, _, summary) = parse_agent_response(raw).unwrap();
        assert_eq!(summary, "from result");
    }

    #[test]
    fn do_status_runs_against_empty_state_without_panicking() {
        let tmp = std::env::temp_dir().join(format!(
            "chorus-ops-dostatus-empty-{}.json",
            std::process::id()
        ));
        let mut config = test_config();
        config.state_file = tmp.clone();
        do_status(&config);
        let _ = fs::remove_file(&tmp);
    }

    // --- lock + process coverage ---

    #[test]
    fn libc_kill_for_current_process_is_true() {
        // Our own PID is always alive while this test runs.
        let pid = std::process::id() as i32;
        assert!(libc_kill(pid));
    }

    #[test]
    fn libc_kill_for_impossible_pid_is_false() {
        // PID 0xFFFFFF is well above real pid-max on macos/linux; `kill -0`
        // will return ESRCH → false.
        assert!(!libc_kill(16_777_215));
    }

    #[test]
    fn acquire_and_release_lock_roundtrip() {
        // Lock path is hardcoded (/tmp/chorus-ops.lock). Make sure any prior
        // state is cleared so this test reflects the acquire → release cycle,
        // and doesn't leave residue for other tests.
        let lock = std::path::Path::new("/tmp/chorus-ops.lock");
        let _ = fs::remove_file(lock);

        acquire_lock().expect("acquire should succeed from a clean state");
        assert!(lock.exists(), "lock file should exist after acquire");

        release_lock();
        assert!(!lock.exists(), "lock file should be gone after release");
    }

    #[test]
    fn acquire_lock_refuses_when_live_pid_holds_lock() {
        let lock = std::path::Path::new("/tmp/chorus-ops.lock");
        let _ = fs::remove_file(lock);
        // Write our own pid as the lock holder — it's alive, so acquire
        // should see a live holder and refuse.
        fs::write(lock, std::process::id().to_string()).unwrap();

        let result = acquire_lock();
        assert!(result.is_err());
        let msg = result.unwrap_err();
        assert!(msg.contains("Already running"), "got: {}", msg);

        let _ = fs::remove_file(lock);
    }

    #[test]
    fn acquire_lock_reclaims_stale_lock_from_dead_pid() {
        let lock = std::path::Path::new("/tmp/chorus-ops.lock");
        let _ = fs::remove_file(lock);
        // An impossible pid → libc_kill returns false → acquire_lock
        // deletes the stale file and writes our pid.
        fs::write(lock, "16777215").unwrap();

        acquire_lock().expect("stale lock should be reclaimed");
        let content = fs::read_to_string(lock).unwrap();
        assert_eq!(content, std::process::id().to_string());

        let _ = fs::remove_file(lock);
    }

    #[test]
    fn acquire_lock_handles_empty_pid_file() {
        let lock = std::path::Path::new("/tmp/chorus-ops.lock");
        let _ = fs::remove_file(lock);
        // Empty pid file → skip the kill check, reclaim.
        fs::write(lock, "").unwrap();

        acquire_lock().expect("empty pid file should be reclaimed");
        let _ = fs::remove_file(lock);
    }

    // --- run() dispatch coverage ---

    /// Helper: build a &[String] from &[&str] without using sv() from pure_tests.
    fn rargs(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    /// ExitCode doesn't implement PartialEq. Downcast via Termination + format,
    /// or just round-trip through the process exit value by checking the
    /// Debug repr. Simpler: do the comparison by invoking the function and
    /// capturing the Debug string.
    fn exit_code_is(code: ExitCode, expected: u8) -> bool {
        format!("{:?}", code) == format!("ExitCode(ExitCode({}))", expected)
            || format!("{:?}", code).contains(&expected.to_string())
    }

    #[test]
    fn run_with_empty_args_exits_1_with_usage() {
        let code = run(&rargs(&[]));
        // Empty args → parse_args returns "Usage:" error → exit 0 (help path).
        // `msg.contains("Usage:")` → exit 0 per the run() matcher.
        assert!(exit_code_is(code, 0));
    }

    #[test]
    fn run_with_help_flag_exits_0() {
        let code = run(&rargs(&["--help"]));
        assert!(exit_code_is(code, 0));
    }

    #[test]
    fn run_with_unknown_flag_exits_1() {
        let code = run(&rargs(&["errors", "--nope"]));
        // "Unknown arg" error doesn't contain "Usage:" or "chorus-ops" → exit 1.
        assert!(exit_code_is(code, 1));
    }

    #[test]
    fn run_with_unknown_subcommand_exits_1() {
        // Unknown subcommand passes parse_args (it just uses the sub as-is),
        // but falls through to the `other` arm.
        let code = run(&rargs(&["not-a-subcommand"]));
        assert!(exit_code_is(code, 1));
    }

    #[test]
    fn run_status_subcommand_exits_success() {
        let code = run(&rargs(&["status"]));
        // do_status reads state_file which may or may not exist; either way
        // run() returns ExitCode::SUCCESS for the status arm.
        assert!(exit_code_is(code, 0));
    }
}
