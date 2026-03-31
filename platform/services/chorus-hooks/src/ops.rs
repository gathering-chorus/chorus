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
use std::process::{Command, ExitCode};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use chrono::{DateTime, Utc};
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
    "unhealthy containers.*promtail",
    "chorus-audit",
    "grafana-alerts",
    "INFO Fuseki.*PUT",
    "INFO Fuseki.*GET",
    "C3 memory usage",
    "traffic spike",
    "Deploy time",
    "Build time exceeds",
    "container unhealthy.*transient",
    "SPARQL query.*slow",
    "SPARQL query.*degraded",
    "XA crash recov",
    "command not found",
];

// --- State types ---

#[derive(Debug, Serialize, Deserialize, Clone)]
struct OpsState {
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
struct Finding {
    id: String,
    severity: String,
    category: String,
    title: String,
    description: String,
    action: String,
    is_repeat: bool,
}

// --- CLI parsing ---

struct Config {
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
    candidates.push(PathBuf::from("/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts"));

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

    // Build regex caches
    let fp_regexes = compile_false_positives();
    let critical_re = Regex::new(CRITICAL_PATTERN).unwrap();

    // Load state
    let mut state = load_state(&config.state_file);
    let now_iso = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();

    // Expire old entries
    let cutoff = (Utc::now() - chrono::Duration::hours(DEDUP_WINDOW_HOURS))
        .format("%Y-%m-%dT%H:%M:%SZ")
        .to_string();
    state.defects.retain(|_, d| d.last_seen >= cutoff);

    // Parse results
    let mut new_defects: Vec<String> = Vec::new(); // hashes
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
            let stream_fp = is_false_positive(stream_app, &fp_regexes);

            let values = stream
                .get("values")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();

            for entry in &values {
                let arr = match entry.as_array() {
                    Some(a) if a.len() >= 2 => a,
                    _ => continue,
                };
                let line = arr[1].as_str().unwrap_or("");

                if stream_fp || is_false_positive(line, &fp_regexes) {
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
                    // Unstructured
                    if *env_type == "STRUCTURED" {
                        continue;
                    }
                    if *env_type == "CHORUS" {
                        msg = line.to_string();
                    } else {
                        // Check for level= in unstructured
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
                let tier = classify_tier(line, &critical_re);

                if let Some(existing) = state.defects.get_mut(&h) {
                    existing.count += 1;
                    existing.last_seen = now_iso.clone();
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
                            first_seen: now_iso.clone(),
                            last_seen: now_iso.clone(),
                            card_id: None,
                        },
                    );
                    new_defects.push(h);
                }
            }
        }
    }

    // Decide what to card
    struct Action {
        action_type: String, // "card" or "comment"
        hash: String,
        priority: String,
        _reason: String,
    }

    let mut actions: Vec<Action> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for h in &new_defects {
        if seen.contains(h) {
            continue;
        }
        seen.insert(h.clone());
        let d = &state.defects[h];
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

    for h in &updated_defects {
        if seen.contains(h) {
            continue;
        }
        let d = &state.defects[h];
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

    // Execute actions
    let mut carded = 0u32;
    for act in &actions {
        let d = match state.defects.get(&act.hash) {
            Some(d) => d.clone(),
            None => continue,
        };
        let title_prefix = if act.priority == "P1" {
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

        if act.action_type == "card" {
            if config.dry_run {
                println!("DRY-RUN: would card [{}] {}", act.priority, title);
                continue;
            }

            let owner = if d.source.contains("personal-site-app")
                || d.source.contains("wordpress")
            {
                "Kade"
            } else {
                "Silas"
            };

            let desc = format!(
                "Auto-detected by chorus-ops (errors).\n\nPattern: {}\nSample: {}\nFirst seen: {}\nCount: {}\nHash: {}",
                &d.pattern[..d.pattern.len().min(200)],
                &d.sample[..d.sample.len().min(300)],
                d.first_seen,
                d.count,
                d.hash
            );

            let output = Command::new(&config.cards_bin)
                .args([
                    "add",
                    &title,
                    "--owner",
                    owner,
                    "--priority",
                    &act.priority,
                    "--status",
                    "ops",
                    "--domain",
                    "infrastructure",
                    "--type",
                    "fix",
                    "--quick",
                    "--description",
                    &desc,
                ])
                .output();

            if let Ok(out) = output {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let card_re = Regex::new(r"#(\d+)").unwrap();
                let combined = format!("{} {}", stdout, stderr);
                if let Some(caps) = card_re.captures(&combined) {
                    let card_id = caps[1].to_string();
                    if let Some(defect) = state.defects.get_mut(&act.hash) {
                        defect.card_id = Some(card_id.clone());
                    }
                    println!(
                        "CARDED: #{} [{}] {}: {}",
                        card_id,
                        act.priority,
                        d.source,
                        &d.pattern[..d.pattern.len().min(60)]
                    );
                    carded += 1;
                } else {
                    eprintln!("ERROR: Card creation returned: {}", combined.trim());
                }
            } else if let Err(e) = output {
                eprintln!("ERROR: Failed to spawn cards: {}", e);
            }

            // Emit spine event
            let _ = Command::new(&config.chorus_log_bin)
                .args([
                    "ops.defect.detected",
                    "system",
                    &format!("source={}", d.source),
                    &format!("tier={}", d.tier),
                    &format!("hash={}", d.hash),
                    &format!(
                        "card_id={}",
                        state
                            .defects
                            .get(&act.hash)
                            .and_then(|d| d.card_id.as_deref())
                            .unwrap_or("none")
                    ),
                    &format!("pattern={}", &d.pattern[..d.pattern.len().min(80)]),
                ])
                .output();
        } else if act.action_type == "comment" {
            if let Some(card_id) = &d.card_id {
                if config.dry_run {
                    println!("DRY-RUN: would comment on #{} ({}x)", card_id, d.count);
                    continue;
                }
                let comment = format!(
                    "Defect recurring: {}x since {}. Latest: {}",
                    d.count, d.first_seen, d.last_seen
                );
                let _ = Command::new(&config.cards_bin)
                    .args(["comment", card_id, &comment])
                    .output();
                println!("COMMENT: #{} ({}x)", card_id, d.count);
            }
        }
    }

    // Save state
    state.last_errors_poll = now_iso;
    save_state(&config.state_file, &state);

    // Summary
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

// --- HEALTH subcommand ---

/// Data collected during pre-fetch phase
#[derive(Serialize)]
struct HealthContext {
    timestamp: String,
    containers: ContainerInfo,
    alerts: AlertInfo,
    errors: ErrorInfo,
    disk: DiskInfo,
    board: BoardInfo,
    previous_findings: Vec<Finding>,
}

#[derive(Serialize)]
struct ContainerInfo {
    total: u32,
    running: u32,
    unhealthy: Vec<String>,
    stopped: Vec<String>,
    missing: Vec<String>,
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

const EXPECTED_CONTAINERS: &[&str] = &[
    "jeff-bridwell-personal-site-app",
    "jeff-bridwell-personal-site-fuseki",
    "jeff-bridwell-personal-site-navidrome",
    "jeff-bridwell-personal-site-css",
    "jeff-bridwell-personal-site-webvowl",
    "prometheus",
    "alertmanager",
    "grafana",
    "loki",
    "promtail",
    "blackbox-exporter",
    "mysqld-exporter",
    "node-exporter",
    "vikunja",
    "wordpress-mysql",
    "wordpress-blog",
    "wordpress-mailhog",
];

fn do_health(config: &Config) -> Result<(), String> {
    if config.verbose {
        log_msg("Phase 1: Pre-fetching system state");
    }

    // Parallel pre-fetch using threads
    let (tx, rx) = mpsc::channel();

    // Docker ps
    {
        let tx = tx.clone();
        thread::spawn(move || {
            let out = Command::new("docker")
                .args(["ps", "-a", "--format", "json"])
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
                .unwrap_or_default();
            let _ = tx.send(("docker", out));
        });
    }

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

    // Assemble context
    let container_info = parse_containers(fetched.get("docker").map(|s| s.as_str()).unwrap_or(""));
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

    let state = load_state(&config.state_file);
    let previous_findings = state.health.findings.clone();

    let context = HealthContext {
        timestamp: Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string(),
        containers: container_info,
        alerts: alert_info,
        errors: error_info,
        disk: disk_info,
        board: board_info,
        previous_findings,
    };

    // Dry run: show context and exit
    if config.dry_run {
        println!("[health] Dry run — context JSON:");
        if let Ok(json) = serde_json::to_string_pretty(&context) {
            println!("{}", json);
        }
        return Ok(());
    }

    // Phase 2: Claude reasoning
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

    let json_schema = r#"{"type":"object","properties":{"status":{"type":"string"},"findings":{"type":"array","items":{"type":"object","properties":{"id":{"type":"string"},"severity":{"type":"string"},"category":{"type":"string"},"title":{"type":"string"},"description":{"type":"string"},"action":{"type":"string"},"is_repeat":{"type":"boolean"}},"required":["id","severity","category","title","description","action","is_repeat"]}},"summary":{"type":"string"}},"required":["status","findings","summary"]}"#;

    let claude_output = Command::new("claude")
        .args([
            "-p",
            "--model",
            &config.model,
            "--permission-mode",
            "dontAsk",
            "--no-session-persistence",
            "--max-budget-usd",
            &config.budget,
            "--output-format",
            "json",
            "--json-schema",
            json_schema,
            "--disallowedTools",
            "Bash,Edit,Write,Glob,Grep,WebFetch,WebSearch,NotebookEdit,Task",
            "--system-prompt",
            &system_prompt,
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null())
        .env_remove("CLAUDECODE")
        .spawn()
        .and_then(|mut child| {
            if let Some(ref mut stdin) = child.stdin {
                let _ = stdin.write_all(context_json.as_bytes());
            }
            child.wait_with_output()
        });

    let response_raw = match claude_output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).to_string(),
        Ok(out) => {
            log_msg(&format!("ERROR: claude -p failed (exit code {:?})", out.status.code()));
            emit_chorus_log(config, &["ops.agent.completed", "system", "status=error", &format!("model={}", config.model)]);
            return Err("claude -p failed".to_string());
        }
        Err(e) => {
            log_msg(&format!("ERROR: claude -p spawn failed: {}", e));
            return Err(e.to_string());
        }
    };

    if response_raw.len() < 10 {
        log_msg(&format!(
            "ERROR: Claude response too small ({} bytes)",
            response_raw.len()
        ));
        emit_chorus_log(config, &["ops.agent.completed", "system", "status=error", &format!("model={}", config.model), "error=empty_response"]);
        return Err("Empty response".to_string());
    }

    // Save last response for debugging
    let _ = fs::write("/Users/jeffbridwell/Library/Logs/Chorus/chorus-ops-last-health-response.json", &response_raw);

    if config.verbose {
        log_msg(&format!(
            "Phase 2: Claude response received ({} bytes)",
            response_raw.len()
        ));
    }

    // Parse response — handle claude JSON envelope
    let response: serde_json::Value =
        serde_json::from_str(&response_raw).map_err(|e| format!("JSON parse error: {}", e))?;

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

    // Phase 3: Act on findings
    if config.verbose {
        log_msg("Phase 3: Processing findings");
    }

    let mut state = load_state(&config.state_file);
    let now_iso = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
    let mut cards_created: u64 = 0;
    let mut carded_categories = state.health.carded_categories.clone();

    for f in &findings {
        if f.action == "card" && !f.is_repeat && cards_created < MAX_CARDS as u64 && !is_on_cooldown(&f.category, &carded_categories) {
            let priority = if f.severity == "critical" { "P1" } else { "P2" };
            let card_desc = format!(
                "Auto-detected by chorus-ops (health).\n\n{}\n\nFinding ID: {}\nSeverity: {}\nCategory: {}",
                f.description, f.id, f.severity, f.category
            );

            let output = Command::new(&config.cards_bin)
                .args([
                    "add",
                    &format!("[ops-health] {}", f.title),
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
                    "--quick",
                    "--description",
                    &card_desc,
                ])
                .output();

            if let Ok(out) = output {
                let stdout = String::from_utf8_lossy(&out.stdout);
                let stderr = String::from_utf8_lossy(&out.stderr);
                let combined = format!("{} {}", stdout, stderr);
                let card_re = Regex::new(r"#(\d+)").unwrap();
                let card_id = card_re
                    .captures(&combined)
                    .map(|c| c[1].to_string())
                    .unwrap_or("?".to_string());
                cards_created += 1;
                carded_categories.insert(f.category.clone(), now_iso.clone());
                println!("CARD: #{} [{}] {}", card_id, priority, f.title);
            } else if let Err(e) = output {
                eprintln!("ERROR: card creation failed: {}", e);
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

    // Emit spine event
    emit_chorus_log(config, &[
        "ops.agent.completed",
        "system",
        &format!("status={}", status),
        &format!("findings={}", findings.len()),
        &format!("cards={}", cards_created),
        &format!("model={}", config.model),
        &format!("summary={}", &summary[..summary.len().min(100)]),
    ]);

    // Save health state
    state.health = HealthState {
        last_run: now_iso,
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

fn emit_chorus_log(config: &Config, args: &[&str]) {
    let _ = Command::new(&config.chorus_log_bin)
        .args(args)
        .output();
}

// --- Pre-fetch parsers ---

fn parse_containers(docker_output: &str) -> ContainerInfo {
    let mut info = ContainerInfo {
        total: 0,
        running: 0,
        unhealthy: Vec::new(),
        stopped: Vec::new(),
        missing: Vec::new(),
    };
    let mut running_names: std::collections::HashSet<String> = std::collections::HashSet::new();

    for line in docker_output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(c) = serde_json::from_str::<serde_json::Value>(line) {
            info.total += 1;
            let name = c
                .get("Names")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let state = c
                .get("State")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_lowercase();
            let status = c
                .get("Status")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_lowercase();

            running_names.insert(name.clone());
            if state == "running" {
                info.running += 1;
                if status.contains("unhealthy") {
                    info.unhealthy.push(name);
                }
            } else {
                info.stopped.push(name);
            }
        }
    }

    info.missing = EXPECTED_CONTAINERS
        .iter()
        .filter(|name| !running_names.contains(**name))
        .map(|s| s.to_string())
        .collect();
    info.missing.sort();

    info
}

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

            if count % HEALTH_THROTTLE_EVERY == 0 {
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
