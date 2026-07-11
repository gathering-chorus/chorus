//! werk-test — the test gate as a werk verb (#3190).
//!
//! Promotes #3397's inline `werk.yml` test step into a real verb (ADR-032 §1:
//! zero-dep, std-only; git/cargo/jest/tsc/clippy/doc-coherence are subprocesses,
//! never code deps). Three things the inline step could not do:
//!   1. BLOCKING — a red floor stops the land (the inline step was advisory:
//!      "No exit 1 here on purpose").
//!   2. Bootstrap escape — a card that modifies the test gate's OWN surface
//!      (`werk.yml` or this crate) runs ADVISORY, so it can't deadlock against
//!      the CANONICAL `werk.yml` it is trying to fix (#3397 deadlock class:
//!      `chorus_werk` runs act with `-W $CHORUS_HOME/.github/workflows/werk.yml`,
//!      so a self-modifying card never validates on its own run).
//!   3. Affected-unit detection on the card's DIFF, not the whole workspace.
//!
//! The pure decision core (`affected_units` / `is_self_modifying` / `gate_outcome`)
//! is unit-tested with no subprocess; the git-diff + runners wire on top.

pub type R<T> = Result<T, String>;

/// Known TS packages with their own jest config + node_modules (matches #3397's
/// hardcoded set — the only packages whose tests are runnable in the werk).
///
/// ⚠️ Part of the TRANSITIONAL BRIDGE (see `affected_units`): this hand-maintained
/// list is the pre-graph approach. Stage 2 derives the testable units from the
/// tests-domain graph (`Test covers Service`), not a hardcoded array — gated on
/// #2818's populated instances. Don't grow this list as if it were permanent.
pub const TS_PACKAGES: &[&str] = &[
    "platform/api",
    "platform/chorus-sdk",
    "platform/pulse",
    "platform/workflow-engine",
];

/// The test gate's OWN surface. A card whose diff touches these is "self-
/// modifying": it cannot be hard-gated by the canonical `werk.yml` it is fixing,
/// so the gate degrades to advisory (the #3397 bootstrap-deadlock escape). A
/// trailing `/` means "this path prefix"; no slash means an exact file match.
pub const SELF_SURFACE: &[&str] = &[
    ".github/workflows/werk.yml",
    "platform/services/werk-test/",
];

/// A unit whose tests must run because the card's diff touched it.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TestUnit {
    /// Rust crate at `platform/services/<name>/` — `cargo test --lib --bins`.
    RustCrate(String),
    /// TS package (one of `TS_PACKAGES`) — `jest` in the package dir.
    TsPackage(String),
}

/// Classify the card's changed files into the test units that must run.
///
/// ⚠️ TRANSITIONAL BRIDGE (#3190 re-scope, Jeff 2026-06-21). Hardcoded git-diff path
/// matching — the PRE-graph approach. The coherent end-state gets the test list from the
/// TESTS-DOMAIN GRAPH: card's changed services → the `Test` instances that cover them
/// (`Test covers Service` / `Test inFile SourceFile`) → run those, at the tier their
/// `pyramidLayer` + `hermeticity` declare. That's stage 2 (= #3148 semantic blast-radius,
/// the query-from-model thesis), gated on #2818 populating the 489 Test instances + `covers`
/// edges. Do NOT let this hardcoded core (also `TS_PACKAGES`, `check_plan`) calcify into a
/// hand-maintained list that drifts — it is a stepping stone the graph query replaces.
///
/// Mirrors #3397's git-diff heuristic, made deterministic: Rust crates first
/// (sorted, deduped by crate name), then TS packages in `TS_PACKAGES` order. A
/// path is a Rust crate iff it sits under `platform/services/<name>/`; the runner
/// later confirms a `Cargo.toml` exists (a pure fn can't touch the fs). A TS
/// package matches iff a changed file is under `<pkg>/`.
pub fn affected_units(changed: &[String]) -> Vec<TestUnit> {
    let mut crates: Vec<String> = Vec::new();
    for f in changed {
        if let Some(rest) = f.strip_prefix("platform/services/") {
            if let Some(name) = rest.split('/').next() {
                if !name.is_empty() && !crates.iter().any(|c| c == name) {
                    crates.push(name.to_string());
                }
            }
        }
    }
    crates.sort();
    let mut units: Vec<TestUnit> = crates.into_iter().map(TestUnit::RustCrate).collect();
    for pkg in TS_PACKAGES {
        if changed.iter().any(|f| f.starts_with(&format!("{}/", pkg))) {
            units.push(TestUnit::TsPackage((*pkg).to_string()));
        }
    }
    units
}

/// Bootstrap escape: does the diff touch the test gate's own surface? If so, the
/// gate runs advisory (a self-modifying card can't validate against the canonical
/// `werk.yml` it is fixing — #3397).
pub fn is_self_modifying(changed: &[String]) -> bool {
    changed.iter().any(|f| {
        SELF_SURFACE.iter().any(|s| match s.strip_suffix('/') {
            Some(dir) => f.starts_with(&format!("{}/", dir)),
            None => f == s,
        })
    })
}

/// The blocking gate decision (#3190 advisory→blocking).
#[derive(Debug, PartialEq, Eq)]
pub enum GateOutcome {
    /// No affected units — nothing to test; passes.
    NoUnits,
    /// All affected units passed.
    Pass,
    /// A unit failed and the gate BLOCKS the land (exit 1).
    Block,
    /// A unit failed but the diff is self-modifying — advisory, does NOT block.
    AdvisoryFail,
}

/// Decide the gate outcome. A failure blocks the land UNLESS the diff is
/// self-modifying, where it degrades to advisory to avoid the canonical-`werk.yml`
/// bootstrap deadlock (#3397). No affected units → nothing to prove → passes.
pub fn gate_outcome(unit_count: usize, any_failed: bool, self_modifying: bool) -> GateOutcome {
    if unit_count == 0 {
        GateOutcome::NoUnits
    } else if !any_failed {
        GateOutcome::Pass
    } else if self_modifying {
        GateOutcome::AdvisoryFail
    } else {
        GateOutcome::Block
    }
}

/// A single check the verb runs. Per-unit checks carry their unit; workspace-level
/// ratchets (`ClippyRatchet`, `DocCoherence`) run once with `unit == None`.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum CheckKind {
    /// `cargo test --lib --bins` — per Rust crate.
    CargoTest,
    /// `npx tsc --noEmit` — per TS package.
    Tsc,
    /// `jest --ci` — per TS package.
    Jest,
    /// `clippy-ratchet.sh` — per-lint counts only decrease; workspace-wide, once.
    ClippyRatchet,
    /// `doc-coherence-ratchet.test.sh` — doc-inventory floor; workspace-wide, once.
    DocCoherence,
}

impl CheckKind {
    pub fn label(self) -> &'static str {
        match self {
            CheckKind::CargoTest => "cargo-test",
            CheckKind::Tsc => "tsc",
            CheckKind::Jest => "jest",
            CheckKind::ClippyRatchet => "clippy-ratchet",
            CheckKind::DocCoherence => "doc-coherence",
        }
    }
}

/// One planned check: its kind + the unit it runs against (None = workspace-level).
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct PlannedCheck {
    pub unit: Option<TestUnit>,
    pub kind: CheckKind,
}

/// Build the full check plan from the affected units (#3190 — #3397's cargo+jest
/// PLUS the three it didn't wire: tsc, clippy-ratchet, doc-coherence). Per unit: a
/// Rust crate → `cargo-test`; a TS package → `tsc` + `jest`. Workspace-level, once:
/// `clippy-ratchet` iff any Rust crate changed; `doc-coherence` iff anything is
/// affected (the repo-wide doc floor). Deterministic order: per-unit checks in
/// `units` order, then clippy, then doc-coherence.
///
/// ⚠️ TRANSITIONAL BRIDGE (see `affected_units`): the per-unit check mapping is
/// hardcoded here. Stage 2 derives which gate runs at which tier from the model
/// (`pyramidLayer` + `hermeticity`) rather than this fixed table — Silas's #3540
/// selection is the first wedge. Deterministic by design either way (same units →
/// same plan), per #3190's governing bar.
pub fn check_plan(units: &[TestUnit]) -> Vec<PlannedCheck> {
    let mut plan = Vec::new();
    let mut any_rust = false;
    for u in units {
        match u {
            TestUnit::RustCrate(_) => {
                any_rust = true;
                plan.push(PlannedCheck { unit: Some(u.clone()), kind: CheckKind::CargoTest });
            }
            TestUnit::TsPackage(_) => {
                plan.push(PlannedCheck { unit: Some(u.clone()), kind: CheckKind::Tsc });
                plan.push(PlannedCheck { unit: Some(u.clone()), kind: CheckKind::Jest });
            }
        }
    }
    if any_rust {
        plan.push(PlannedCheck { unit: None, kind: CheckKind::ClippyRatchet });
    }
    if !units.is_empty() {
        plan.push(PlannedCheck { unit: None, kind: CheckKind::DocCoherence });
    }
    plan
}

/// A test case quarantined out of the BLOCKING gate: flaky, with a tracked reason
/// and an expiry (#2530, absorbed into #2819). Quarantine is the governed middle
/// path between "a flaky test blocks everyone" and "someone silently comments it
/// out and it rots": the gate SKIPS it (a quarantined case can't block a land), the
/// nightly still RUNS it to collect flakiness signal and reports it separately, and
/// an expiry query (`until` < today) auto-files a card so it can't be forgotten.
///
/// This is a value in the `validityClass` family on `chorus:Test` (the field is
/// already on TestShape, empty today), written via the DAL authored lane with audit
/// stamps. `case` matches the test's `testName` — the handle cargo/jest recognize.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct Quarantined {
    pub case: String,
    pub reason: String,
    pub until: String,
}

/// Build the cargo args that EXCLUDE quarantined cases from a `cargo test` run:
/// `cargo test --lib --bins -- --skip <case> --skip <case>`. Empty in → empty out,
/// with NO trailing `--`, so an un-quarantined run is byte-identical to today's
/// invocation. cargo's libtest harness matches each `--skip` value as a substring
/// against the test path, so the case's `testName` is the right handle.
///
/// Rust-only by design: jest has no clean exclude-by-name CLI, so jest enforcement
/// is a separate mechanism (a generated skip-list the jest setup reads) — an
/// explicit follow-on, not faked here.
pub fn cargo_skip_args(quarantined: &[&str]) -> Vec<String> {
    if quarantined.is_empty() {
        return Vec::new();
    }
    let mut v = vec!["--".to_string()];
    for c in quarantined {
        v.push("--skip".to_string());
        v.push((*c).to_string());
    }
    v
}

/// Parse the `curl … | jq -r '… | @tsv'` output (one `testName\treason\tuntil`
/// row per quarantined case) into `Quarantined`. Pure, so the read-wiring is
/// testable rather than a thin shell. A row with a blank case (no `testName`)
/// can't be `--skip`'d by name, so it's dropped — better to run a mis-recorded
/// hold than to skip the wrong thing.
pub fn parse_quarantine_rows(tsv: &str) -> Vec<Quarantined> {
    tsv.lines()
        .filter_map(|line| {
            let mut f = line.split('\t');
            let case = f.next().unwrap_or("").trim();
            if case.is_empty() {
                return None;
            }
            Some(Quarantined {
                case: case.to_string(),
                reason: f.next().unwrap_or("").to_string(),
                until: f.next().unwrap_or("").to_string(),
            })
        })
        .collect()
}

/// The quarantines whose `until` is strictly before `today` (ISO `YYYY-MM-DD`,
/// lexically ordered = chronologically ordered) — the auto-file-card candidates
/// (#2530). `until == today` is NOT expired (the hold runs through end of day); a
/// blank/malformed `until` is never expired (don't silently file on bad data).
pub fn expired_cases<'a>(q: &'a [Quarantined], today: &str) -> Vec<&'a Quarantined> {
    q.iter()
        .filter(|x| !x.until.is_empty() && x.until.as_str() < today)
        .collect()
}

/// One-line, ALWAYS-printed report of the cases the gate skipped because they're
/// quarantined — a skip must be VISIBLE, never a silent absence (#3443 "I don't see
/// it" bar). Empty set → an explicit `quarantined: none`, not blank.
pub fn quarantine_report(q: &[Quarantined]) -> String {
    if q.is_empty() {
        return "quarantined: none".to_string();
    }
    let items: Vec<String> = q
        .iter()
        .map(|x| format!("{} ({}, until {})", x.case, x.reason, x.until))
        .collect();
    format!("quarantined ({} skipped): {}", q.len(), items.join("; "))
}

/// Pure arg-builder for a chorus-log spine emit (mirrors werk-build #3166). The
/// verb emits a typed `test.failed` per failed check on the inherited trace
/// (#3162), so a red gate is queryable, not just a pipeline exit code.
pub fn spine_args(event: &str, role: &str, card: &str, trace: &str, extras: &[(&str, &str)]) -> Vec<String> {
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

/// #3621 — the canonical wide `test.completed` field set, emitted on EVERY run.
/// Green earns evidence too: werk-test used to emit only `test.failed`, so a
/// passing blocking gate left zero spine trace — "all tests passed" and "the
/// step never ran" were indistinguishable (the #3609 trace-visibility gap).
/// Pure builder → unit-testable; the impure emit stays in main.
pub fn completed_extras(
    outcome: &GateOutcome,
    units: usize,
    checks_run: usize,
    checks_failed: usize,
    duration_ms: u128,
    self_modifying: bool,
) -> Vec<(String, String)> {
    let mut v = vec![
        ("verdict".to_string(), outcome.label().to_string()),
        ("units".to_string(), units.to_string()),
        ("checks_run".to_string(), checks_run.to_string()),
        ("checks_failed".to_string(), checks_failed.to_string()),
        ("duration_ms".to_string(), duration_ms.to_string()),
    ];
    if checks_failed > 0 {
        v.push(("failureClass".to_string(), "change".to_string()));
    }
    if self_modifying {
        v.push(("advisory".to_string(), "true".to_string()));
    }
    v
}

impl GateOutcome {
    /// Process exit code: only `Block` stops the land (exit 1). `AdvisoryFail`
    /// is honest-red-but-non-blocking → exit 0 (the bootstrap escape). `Pass` /
    /// `NoUnits` → exit 0.
    pub fn exit_code(&self) -> i32 {
        match self {
            GateOutcome::Block => 1,
            _ => 0,
        }
    }

    /// Human label for the verb's summary line + the gh status description.
    pub fn label(&self) -> &'static str {
        match self {
            GateOutcome::NoUnits => "no-affected-units",
            GateOutcome::Pass => "pass",
            GateOutcome::Block => "BLOCK",
            GateOutcome::AdvisoryFail => "advisory-fail (self-modifying)",
        }
    }
}

// ── #3634 — model-driven plan from the tests domain ────────────────────────

/// One row of the tests-domain read: which file a test lives in, which domain
/// it covers. Fetched via curl|jq at the boundary (the quarantine pattern,
/// ADR-032 §6) and handed here as TSV `filePath\tcovers`.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct TestRow {
    pub file_path: String,
    pub covers: String,
}

/// Parse `filePath\tcovers` TSV rows; incomplete rows are dropped, never a panic.
pub fn parse_test_rows(tsv: &str) -> Vec<TestRow> {
    tsv.lines()
        .filter_map(|l| {
            let mut it = l.trim().split('\t');
            match (it.next(), it.next()) {
                (Some(f), Some(c)) if !f.trim().is_empty() && !c.trim().is_empty() => {
                    Some(TestRow { file_path: f.trim().to_string(), covers: c.trim().to_string() })
                }
                _ => None,
            }
        })
        .collect()
}

/// The unit a test file lives in — same classification rules as `affected_units`.
pub fn unit_of_path(path: &str) -> Option<TestUnit> {
    if let Some(rest) = path.strip_prefix("platform/services/") {
        if let Some(name) = rest.split('/').next() {
            if !name.is_empty() {
                return Some(TestUnit::RustCrate(name.to_string()));
            }
        }
    }
    for pkg in TS_PACKAGES {
        if path.starts_with(&format!("{}/", pkg)) {
            return Some(TestUnit::TsPackage((*pkg).to_string()));
        }
    }
    None
}

/// #3634 stage-2 (the query-from-model thesis the `affected_units` bridge note
/// anticipates): touched domains = the `covers` of tests living in the card's
/// legacy-derived units; every unit holding tests that cover a touched domain
/// joins the plan. Always a UNION with legacy — in v1 the model only ADDS
/// coverage (the superset AC holds by construction); shrinking to pure
/// blast-radius selection is the follow-on flip once a week of runs proves it.
pub fn model_units(rows: &[TestRow], legacy: &[TestUnit]) -> Vec<TestUnit> {
    let mut units: Vec<TestUnit> = legacy.to_vec();
    let touched: Vec<&str> = rows
        .iter()
        .filter(|r| unit_of_path(&r.file_path).map_or(false, |u| legacy.contains(&u)))
        .map(|r| r.covers.as_str())
        .collect();
    let mut additions: Vec<TestUnit> = rows
        .iter()
        .filter(|r| touched.contains(&r.covers.as_str()))
        .filter_map(|r| unit_of_path(&r.file_path))
        .filter(|u| !units.contains(u))
        .collect();
    additions.sort_by_key(|u| match u {
        TestUnit::RustCrate(n) => (0, n.clone()),
        TestUnit::TsPackage(p) => (1, p.clone()),
    });
    additions.dedup();
    units.extend(additions);
    units
}

/// The TestSuiteRun instance a completed run posts back to the tests domain
/// (#3634 write side): the graph's durable answer to "what ran, what failed".
/// Name is unique per card+timestamp; the caller supplies verdict + counts.
#[allow(clippy::too_many_arguments)]
pub fn suite_run_payload(
    card: &str,
    role: &str,
    trace: &str,
    plan_source: &str,
    checks_planned: usize,
    checks_failed: usize,
    duration_ms: u128,
    verdict: &str,
) -> String {
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    format!(
        "{{\"name\":\"testsuiterun-{}-{}\",\"card\":\"{}\",\"role\":\"{}\",\"traceId\":\"{}\",\
         \"planSource\":\"{}\",\"checksPlanned\":{},\"checksFailed\":{},\"durationMs\":{},\
         \"verdict\":\"{}\"}}",
        card, ts, card, role, trace, plan_source, checks_planned, checks_failed, duration_ms, verdict
    )
}
