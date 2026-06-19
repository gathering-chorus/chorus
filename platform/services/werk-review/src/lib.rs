//! werk-review — the cold-eyes gate's BINARY half (#3193 v2; ADR-032 §1 blueprint:
//! zero-dep std-only Rust, subprocess CLIs, witness + spine emits, typed refusals).
//!
//! Two layers (the #3284 lesson — no LLM inside headless act):
//! - THIS BINARY owns the STRUCTURED FLOOR (objective checks no lazy run can skip)
//!   and the witness: `werk-review <card> <role>` computes the card's merge-base
//!   diff and records `review.floor` with findings; `werk-review verdict <card>
//!   pass|fail <findings…>` is the cold-eyes agent's recorder, guarded by the
//!   anti-ceremony rules; `werk-review check <card>` reads the latest verdict back
//!   (exit 0 pass / 1 fail-or-missing) — defined NOW so the later advisory→hard-gate
//!   flip needs no rewiring.
//! - The COLD-EYES AGENT (one Explore-type subagent in the demoer's prework batch)
//!   reads ONLY the diff + the card AC, adversarially, and records through this
//!   binary. Anti-ceremony: a fail with no findings is rejected; ANY verdict
//!   recorded before the floor ran is rejected ("lgtm" without the objective
//!   minimum on the witness is ceremony, not review).
//!
//! Floor checks (objective, evidence-grade — judgment stays with the agent):
//! 1. unchecked AC boxes (a checked-box-with-no-code hunt needs the boxes first);
//! 2. src changed without any test change (the missing-tests heuristic);
//! 3. removed pub/exported symbols, cross-checked for survivors via ast-grep when
//!    available (the grep-blind-removal class, #3331's manual flow mechanized) —
//!    degrades to a named warning when ast-grep is absent, never hard-requires it.

use std::env;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

// #3513 — the ONE shared failure classifier (failure_class / fail_extra).
include!("../../shared/failure_class.rs");

pub type R<T> = Result<T, String>;

// --- pure helpers (unit-tested) ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Mode {
    Floor { card: u64, role: String },
    Verdict { card: u64, pass: bool, findings: String },
    Check { card: u64 },
}

/// #3193 — the CLI seam (the #3294 pattern): three subcommand shapes, pure + tested.
pub fn parse_review_args(args: &[String], deploy_role: Option<String>) -> R<Mode> {
    let usage = "usage: werk-review <card> <role>  |  werk-review verdict <card> pass|fail <findings…>  |  werk-review check <card>";
    match args.first().map(|s| s.as_str()) {
        Some("verdict") => {
            let card: u64 = args.get(1).ok_or(usage)?.parse().map_err(|_| usage.to_string())?;
            let pass = match args.get(2).map(|s| s.as_str()) {
                Some("pass") => true,
                Some("fail") => false,
                _ => return Err(format!("verdict must be pass|fail. {}", usage)),
            };
            let findings = args.get(3..).map(|s| s.join(" ")).unwrap_or_default();
            Ok(Mode::Verdict { card, pass, findings })
        }
        Some("check") => {
            let card: u64 = args.get(1).ok_or(usage)?.parse().map_err(|_| usage.to_string())?;
            Ok(Mode::Check { card })
        }
        Some(first) => {
            let card: u64 = first.parse().map_err(|_| format!("card id is not a number: {}. {}", first, usage))?;
            let role = args
                .get(1)
                .cloned()
                .or(deploy_role)
                .ok_or_else(|| format!("{} (or set DEPLOY_ROLE)", usage))?;
            Ok(Mode::Floor { card, role })
        }
        None => Err(usage.to_string()),
    }
}

/// Open `- [ ]` checkbox lines from the cards human view — the floor's first input
/// to the #1 hunt (a checked box with no code behind it needs the boxes enumerated).
pub fn unchecked_ac(card_view: &str) -> Vec<String> {
    card_view
        .lines()
        .filter_map(|l| l.trim().strip_prefix("- [ ]").map(|rest| rest.trim().to_string()))
        .filter(|s| !s.is_empty())
        .collect()
}

fn is_src(p: &str) -> bool {
    let code = p.ends_with(".rs") || p.ends_with(".ts") || p.ends_with(".tsx") || p.ends_with(".py") || p.ends_with(".sh");
    let test = is_test(p);
    code && !test && (p.contains("/src/") || p.starts_with("src/") || p.contains("/scripts/"))
}

fn is_test(p: &str) -> bool {
    p.contains("/tests/")
        || p.contains(".test.")
        || p.contains("_test.")
        || p.contains("/test-")
        || p.ends_with(".bats")
        || p.contains("/features/")
}

/// The missing-tests heuristic: source changed and NO test surface changed with it.
pub fn src_without_test(diff_names: &str) -> bool {
    let lines: Vec<&str> = diff_names.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let src_changed = lines.iter().any(|l| is_src(l));
    let test_changed = lines.iter().any(|l| is_test(l));
    src_changed && !test_changed
}

/// Removed pub/exported symbols, read from the unified diff: `-` lines (not `---`
/// headers) declaring `pub fn|struct|enum|trait` or `export function|const|class`
/// whose symbol does not reappear on any `+` line (a rename/move is not a removal).
pub fn removed_pub_symbols(diff: &str) -> Vec<String> {
    fn symbol_of(decl: &str) -> Option<String> {
        let decl = decl.trim_start();
        for prefix in ["pub fn ", "pub struct ", "pub enum ", "pub trait ", "export function ", "export const ", "export class "] {
            if let Some(rest) = decl.strip_prefix(prefix) {
                let name: String = rest
                    .chars()
                    .take_while(|c| c.is_alphanumeric() || *c == '_')
                    .collect();
                if !name.is_empty() {
                    return Some(name);
                }
            }
        }
        None
    }
    let mut removed: Vec<String> = Vec::new();
    let mut added: Vec<String> = Vec::new();
    for line in diff.lines() {
        if let Some(body) = line.strip_prefix('-') {
            if !line.starts_with("---") {
                if let Some(s) = symbol_of(body) {
                    removed.push(s);
                }
            }
        } else if let Some(body) = line.strip_prefix('+') {
            if !line.starts_with("+++") {
                if let Some(s) = symbol_of(body) {
                    added.push(s);
                }
            }
        }
    }
    removed.retain(|s| !added.contains(s));
    removed.dedup();
    removed
}

/// The anti-ceremony rules, pure: a FAIL demands specific findings; ANY verdict
/// demands the floor ran first (the objective minimum exists on the witness).
pub fn validate_verdict(pass: bool, findings: &str, floor_ran: bool) -> R<()> {
    if !floor_ran {
        return Err("verdict rejected: the floor has not run for this card — `werk-review <card> <role>` first (a verdict without the objective minimum is ceremony)".to_string());
    }
    if !pass && findings.trim().is_empty() {
        return Err("verdict rejected: a FAIL demands specific findings (file:line or 'AC item N not covered') — empty findings is a non-review".to_string());
    }
    Ok(())
}

pub fn jsonl_line(ts: u128, event: &str, role: &str, card: u64, trace: &str, extra: &str) -> String {
    format!(
        "{{\"ts\":{},\"event\":\"{}\",\"role\":\"{}\",\"card_id\":{},\"trace_id\":\"{}\"{}}}\n",
        ts, event, role, card, trace, extra
    )
}

pub fn spine_args(event: &str, role: &str, card: u64, trace: &str, extras: &[(&str, &str)]) -> Vec<String> {
    let mut v = vec![
        event.to_string(),
        role.to_string(),
        format!("card={}", card),
        format!("trace={}", trace),
    ];
    for (k, val) in extras {
        v.push(format!("{}={}", k, val));
    }
    v
}

fn trace_id() -> String {
    let ns = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}-{:x}", ns, std::process::id())
}

pub fn resolve_trace(card: u64) -> String {
    if let Ok(t) = env::var("CHORUS_TRACE_ID") {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    if let Ok(t) = std::fs::read_to_string(format!("/tmp/{}-trace", card)) {
        if !t.trim().is_empty() {
            return t.trim().to_string();
        }
    }
    trace_id()
}

// --- side-effecting helpers ---

const WITNESS: &str = "ops/logs/werk-review.jsonl";

fn jsonl(home: &Path, role: &str, card: u64, trace: &str, event: &str, extra: &str) {
    let p = home.join(WITNESS);
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let ts = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&p) {
        let _ = f.write_all(jsonl_line(ts, event, role, card, trace, extra).as_bytes());
    }
}

fn emit_spine(home: &Path, event: &str, role: &str, card: u64, trace: &str, extras: &[(&str, &str)]) {
    let log = home.join("platform/scripts/chorus-log");
    if !log.exists() {
        return;
    }
    let mut c = Command::new("bash");
    c.arg(&log);
    for a in spine_args(event, role, card, trace, extras) {
        c.arg(a);
    }
    let _ = c.output();
}

fn run_in(dir: &str, cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd)
        .args(args)
        .current_dir(dir)
        .output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!("{} {}: {}", cmd, args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn sanitize(s: &str) -> String {
    s.replace('\\', " ").replace('"', "'").replace('\n', "; ")
}

/// Does the witness carry an event line for this card (comma-terminated key —
/// the anti #3/#31 collision rule shared with werk-demo)?
fn witness_has(home: &Path, event: &str, card: u64) -> bool {
    let w = std::fs::read_to_string(home.join(WITNESS)).unwrap_or_default();
    let card_key = format!("\"card_id\":{},", card);
    w.lines()
        .any(|l| l.contains(&format!("\"event\":\"{}\"", event)) && l.contains(&card_key))
}

/// The STRUCTURED FLOOR — the objective minimum even a lazy agent run leaves on
/// the witness. Returns the human-facing findings summary.
pub fn floor(card: u64, role: &str, home: &Path, werk_base: &Path) -> R<String> {
    let trace = resolve_trace(card);
    let werk = werk_base.join(format!("{}-{}", role, card));
    if !werk.is_dir() {
        jsonl(home, role, card, &trace, "review.refused", &fail_extra("no-werk"));
        return Err(format!("no-werk: no werk for #{} at {} — pull the card first", card, werk.display()));
    }
    let werk_s = werk.to_string_lossy().to_string();

    // the card diff, exactly as deploy_canonical computes it (merge-base survives
    // squash). An unreadable diff is a typed refusal — the floor cannot attest to
    // inputs it could not read (dirty-floor-inputs, AC1).
    let _ = run_in(&werk_s, "git", &["fetch", "-q", "origin", "main"]);
    let base = run_in(&werk_s, "git", &["merge-base", "origin/main", "HEAD"])
        .map_err(|e| {
            jsonl(home, role, card, &trace, "review.refused", &fail_extra("dirty-floor-inputs"));
            format!("dirty-floor-inputs: cannot read the card diff (merge-base failed): {}", e)
        })?
        .trim()
        .to_string();
    let range = format!("{}..HEAD", base);
    let names = run_in(&werk_s, "git", &["diff", "--name-only", &range]).map_err(|e| {
        jsonl(home, role, card, &trace, "review.refused", &fail_extra("dirty-floor-inputs"));
        format!("dirty-floor-inputs: cannot read the card diff: {}", e)
    })?;
    let diff = run_in(&werk_s, "git", &["diff", &range]).unwrap_or_default();

    let mut findings: Vec<String> = Vec::new();

    // 1) open AC boxes (cards human view — same source werk-demo's ac_counts reads).
    // A card with NO checkboxes at all is a typed refusal: there is no contract to
    // review the diff against (no-ac, AC1) — mirrors werk-demo's no-ac refuse.
    let view = run(&script(home, "cards"), &["view", &card.to_string()]).unwrap_or_default();
    let total_boxes = view
        .lines()
        .filter(|l| {
            let t = l.trim();
            t.starts_with("- [ ]") || t.starts_with("- [x]") || t.starts_with("- [X]")
        })
        .count();
    if total_boxes == 0 {
        jsonl(home, role, card, &trace, "review.refused", &fail_extra("no-ac"));
        return Err(format!("no-ac: card #{} has no AC checkboxes — nothing to review the diff against", card));
    }
    let open = unchecked_ac(&view);
    if !open.is_empty() {
        findings.push(format!("unchecked AC ({}): {}", open.len(), open.join(" | ")));
    }

    // 2) the missing-tests heuristic.
    if src_without_test(&names) {
        findings.push("src changed without any test change".to_string());
    }

    // 3) removed pub symbols, survivor-checked via ast-grep when available — the
    // grep-blind-removal class. Degrades to a named warning, never hard-requires.
    let gone = removed_pub_symbols(&diff);
    if !gone.is_empty() {
        let have_ast_grep = Command::new("ast-grep").arg("--version").output().map(|o| o.status.success()).unwrap_or(false);
        for sym in &gone {
            if have_ast_grep {
                let no_survivors = Command::new("ast-grep")
                    .args(["run", "--pattern", &format!("{}($$$)", sym), &werk_s])
                    .output()
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().is_empty())
                    .unwrap_or(true);
                if no_survivors {
                    findings.push(format!("removed pub symbol '{}' (no surviving references — verify intentional)", sym));
                } else {
                    findings.push(format!("removed pub symbol '{}' STILL REFERENCED in the werk (grep-blind removal?)", sym));
                }
            } else {
                findings.push(format!("removed pub symbol '{}' (ast-grep unavailable — survivor check skipped)", sym));
            }
        }
    }

    let summary = if findings.is_empty() {
        "floor clean — no objective findings".to_string()
    } else {
        format!("floor findings ({}):\n  {}", findings.len(), findings.join("\n  "))
    };
    let files_changed = names.lines().filter(|l| !l.trim().is_empty()).count();
    jsonl(home, role, card, &trace, "review.floor",
        &format!(",\"findings_count\":{},\"files_changed\":{},\"findings\":\"{}\"",
            findings.len(), files_changed, sanitize(&findings.join(" | "))));
    emit_spine(home, "review.floor", role, card, &trace,
        &[("findings", &findings.len().to_string()), ("files", &files_changed.to_string())]);
    Ok(summary)
}

fn run(cmd: &str, args: &[&str]) -> R<String> {
    let out = Command::new(cmd)
        .args(args)
        .output()
        .map_err(|e| format!("{} failed to start: {}", cmd, e))?;
    if !out.status.success() {
        return Err(format!("{}: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim()));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

fn script(home: &Path, name: &str) -> String {
    home.join("platform/scripts").join(name).to_string_lossy().to_string()
}

/// The cold-eyes agent's recorder, behind the anti-ceremony guard.
pub fn verdict(card: u64, pass: bool, findings: &str, home: &Path) -> R<String> {
    let trace = resolve_trace(card);
    let role = env::var("DEPLOY_ROLE").or_else(|_| env::var("CHORUS_ROLE")).unwrap_or_else(|_| "system".to_string());
    validate_verdict(pass, findings, witness_has(home, "review.floor", card))?;
    let v = if pass { "pass" } else { "fail" };
    jsonl(home, &role, card, &trace, "review.verdict",
        &format!(",\"verdict\":\"{}\",\"findings\":\"{}\"", v, sanitize(findings)));
    emit_spine(home, "review.verdict", &role, card, &trace, &[("verdict", v)]);
    Ok(format!("review.verdict {} recorded for #{}", v, card))
}

/// The future hard-gate read: latest review.verdict for the card. pass → Ok (exit 0),
/// fail or MISSING → Err (exit 1) — missing means not-reviewed, which a hard gate
/// must treat as fail, never as pass-by-default.
pub fn check(card: u64, home: &Path) -> R<String> {
    let w = std::fs::read_to_string(home.join(WITNESS)).unwrap_or_default();
    let card_key = format!("\"card_id\":{},", card);
    let latest = w
        .lines()
        .rev()
        .find(|l| l.contains("\"event\":\"review.verdict\"") && l.contains(&card_key));
    match latest {
        Some(l) if l.contains("\"verdict\":\"pass\"") => Ok(format!("review: pass on record for #{}", card)),
        Some(_) => Err(format!("review: latest verdict for #{} is FAIL — fix and re-review", card)),
        None => Err(format!("review: no verdict on record for #{} — not reviewed", card)),
    }
}

/// Entry: parse + dispatch.
pub fn run_review() -> R<String> {
    let args: Vec<String> = env::args().skip(1).collect();
    let mode = parse_review_args(&args, env::var("DEPLOY_ROLE").ok())?;
    let home = PathBuf::from(env::var("CHORUS_HOME").map_err(|_| "CHORUS_HOME not set".to_string())?);
    match mode {
        Mode::Floor { card, role } => {
            let werk_base = PathBuf::from(
                env::var("CHORUS_WERK_BASE").map_err(|_| "CHORUS_WERK_BASE not set".to_string())?,
            );
            floor(card, &role, &home, &werk_base)
        }
        Mode::Verdict { card, pass, findings } => verdict(card, pass, &findings, &home),
        Mode::Check { card } => check(card, &home),
    }
}
