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
    // #3974 — the packages the nightly's npm walker ran that the gate never
    // selected: registered rows existed, unit_of_path dropped them. mcp-server
    // runs node:test (not jest); the executor picks the runner per package.
    "platform/mcp-server",
    "directing/clearing",
    "directing/products/cards",
    // #3974 — the cucumber package: platform/tests runs cucumber-js via its
    // own `npm test`; its .bats files resolve to script units first.
    "platform/tests",
];

/// The test gate's OWN surface. A card whose diff touches these is "self-
/// modifying": it cannot be hard-gated by the canonical `werk.yml` it is fixing,
/// so the gate degrades to advisory (the #3397 bootstrap-deadlock escape). A
/// trailing `/` means "this path prefix"; no slash means an exact file match.
pub const SELF_SURFACE: &[&str] = &[
    ".github/workflows/werk.yml",
    "platform/services/werk-test/",
    // #3787 — the ESLint-ratchet surface: a card fixing the ratchet itself must
    // run advisory, not deadlock on its own gate (#3197 bootstrap lesson).
    "platform/scripts/lint-ratchet.js",
    "eslint.config.js",
    ".eslint-baseline.json",
];

/// A unit whose tests must run because the card's diff touched it.
#[derive(Debug, PartialEq, Eq, Clone)]
pub enum TestUnit {
    /// Rust crate at `platform/services/<name>/` — `cargo test --lib --bins`.
    RustCrate(String),
    /// TS package (one of `TS_PACKAGES`) — `jest` in the package dir.
    TsPackage(String),
    /// Bats suite under `platform/tests/` — `bats <path>` (#3917). Before this
    /// existed a bats-only diff selected nothing and the gate exited 0.
    BatsSuite(String),
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
        // #3974 — a changed .bats file selects its SUITE unit, never the
        // package around it (platform/tests holds both the cucumber package
        // and dozens of bats suites; editing one suite must not rerun all).
        if changed.iter().any(|f| f.starts_with(&format!("{}/", pkg)) && !f.ends_with(".bats")) {
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
    /// `lint-ratchet.js` — per-rule ESLint counts only decrease (#3787); workspace-wide, once.
    LintRatchet,
    /// `doc-coherence-ratchet.test.sh` — doc-inventory floor; workspace-wide, once.
    DocCoherence,
    /// `bats <suite>` — per changed bats suite (#3917).
    Bats,
    /// #3920 — `npx playwright test <registered ui files>` — workspace-level, once.
    UiFlows,
}

impl CheckKind {
    pub fn label(self) -> &'static str {
        match self {
            CheckKind::CargoTest => "cargo-test",
            CheckKind::Tsc => "tsc",
            CheckKind::Jest => "jest",
            CheckKind::ClippyRatchet => "clippy-ratchet",
            CheckKind::LintRatchet => "lint-ratchet",
            CheckKind::DocCoherence => "doc-coherence",
            CheckKind::Bats => "bats",
            CheckKind::UiFlows => "ui-flows",
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
            TestUnit::BatsSuite(_) => {
                plan.push(PlannedCheck { unit: Some(u.clone()), kind: CheckKind::Bats });
            }
        }
    }
    if any_rust {
        plan.push(PlannedCheck { unit: None, kind: CheckKind::ClippyRatchet });
    }
    // #3787 — the ESLint mirror of clippy-ratchet: any TS change runs the
    // workspace lint ratchet once, so drift refuses at land instead of
    // surfacing as an anonymous 03:55 nightly red.
    if units.iter().any(|u| matches!(u, TestUnit::TsPackage(_))) {
        plan.push(PlannedCheck { unit: None, kind: CheckKind::LintRatchet });
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

/// #3929 — nextest run args for a crate. ALL targets, not `--lib --bins`
/// (Wren's (c), 2026-08-20): the old lane never ran tests/ suites — the gate
/// could not see integration tests. `--no-tests=fail` is nextest's default,
/// spelled out: a selected crate with zero tests is a red, not a silence.
/// Quarantine translates to an exact-name filterset instead of libtest's
/// substring `--skip`; empty quarantine adds no filterset at all.
pub fn nextest_run_args(quarantined: &[&str], exclude_bins: &[&str]) -> Vec<String> {
    let mut v: Vec<String> = ["nextest", "run", "--no-tests=fail"]
        .iter().map(|s| s.to_string()).collect();
    let mut terms: Vec<String> = quarantined.iter()
        .map(|c| format!("not test(={})", c)).collect();
    // #3919 — stack-down: needs-stack integration binaries excluded, typed skip
    terms.extend(exclude_bins.iter().map(|b| format!("not binary(={})", b)));
    if !terms.is_empty() {
        v.push("-E".to_string());
        v.push(terms.join(" and "));
    }
    v
}

/// Parse the pinned nextest version out of `.config/nextest.toml` — the ONE
/// home of the pin (Wren's call, 2026-08-20). None = file missing or no
/// `nextest-version` line, which the caller must treat as a refusal, not a
/// free pass: an unpinned lane cannot prove it runs what the team pinned.
pub fn parse_nextest_pin(toml: &str) -> Option<(u32, u32, u32)> {
    let line = toml.lines().map(str::trim)
        .find(|l| l.starts_with("nextest-version"))?;
    let ver = line.split('"').nth(1)?;
    let mut it = ver.split('.').map(|p| p.parse::<u32>().ok());
    Some((it.next()??, it.next()??, it.next()??))
}

/// Classify a `cargo nextest --version` probe against the pin. Absence and
/// staleness are both LOUD refusals — there is no fallback lane to plain
/// `cargo test` (Silas's ruling on the card: never a vacuous whole-crate green).
pub fn classify_nextest_probe(ok: bool, out: &str, pin: (u32, u32, u32)) -> Result<(), String> {
    let pin_s = format!("{}.{}.{}", pin.0, pin.1, pin.2);
    if !ok {
        if out.contains("no such command") {
            return Err(format!(
                "nextest-missing: cargo has no `nextest` subcommand — install with \
                 `cargo install cargo-nextest --locked --version {}`; the cargo lane \
                 refuses rather than fall back to whole-crate `cargo test`", pin_s));
        }
        return Err(format!("nextest-probe-failed: {}",
            out.lines().next().unwrap_or("(no output)")));
    }
    let ver = out.split_whitespace().nth(1).unwrap_or("");
    let mut it = ver.split('.').filter_map(|p| p.parse::<u32>().ok());
    let got = (it.next().unwrap_or(0), it.next().unwrap_or(0), it.next().unwrap_or(0));
    if got < pin {
        return Err(format!("nextest-older-than-pin: installed {} < pinned {}", ver, pin_s));
    }
    Ok(())
}

/// #3929 — per-case lines from nextest's human output:
/// `    PASS [   0.005s] <binary-id> path::to::case`. Bare fn name is the
/// handle the inventory join uses, same as parse_cargo_cases.
pub fn parse_nextest_cases(out: &str) -> Vec<(String, String)> {
    out.lines()
        .filter_map(|l| {
            let t = l.trim();
            let verdict = if t.starts_with("PASS ") { "pass" }
                else if t.starts_with("FAIL ") { "fail" }
                else if t.starts_with("SKIP ") { "skip" }
                else { return None };
            let after = t.split(']').nth(1)?.trim();
            let path = after.split_whitespace().last()?;
            let bare = path.rsplit("::").next().unwrap_or(path).to_string();
            Some((bare, verdict.to_string()))
        })
        .collect()
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
            // #3917 AC4 — "no-affected-units" read as green for months. It is
            // not a pass; it is the absence of a measurement, and the label now
            // says so out loud.
            GateOutcome::NoUnits => "SELECTED 0 UNITS — nothing was measured",
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
    /// #3661 — the model's pyramidLayer (unit/integration/bdd/e2e). Empty on a
    /// 2-col legacy row; scoping by --type needs the 3-col fetch.
    pub pyramid_layer: String,
    /// #3919 — the model's hermeticity ("hermetic" / "needs-stack"). Empty on
    /// legacy rows; only the literal "needs-stack" gates on the live stack.
    pub hermeticity: String,
    /// #3920 — the model's testConcern ("ui"/"security"/…). Empty on legacy rows;
    /// "ui" routes the file into the browser lane.
    pub test_concern: String,
}

/// Parse `filePath\tcovers[\tpyramidLayer]` TSV rows; incomplete rows are
/// dropped, never a panic. The layer column is optional (2-col back-compat).
pub fn parse_test_rows(tsv: &str) -> Vec<TestRow> {
    tsv.lines()
        .filter_map(|l| {
            let mut it = l.trim().split('\t');
            match (it.next(), it.next()) {
                (Some(f), Some(c)) if !f.trim().is_empty() && !c.trim().is_empty() => {
                    Some(TestRow {
                        file_path: f.trim().to_string(),
                        covers: c.trim().to_string(),
                        pyramid_layer: it.next().map(|x| x.trim().to_string()).unwrap_or_default(),
                        hermeticity: String::new(),
                        test_concern: String::new(),
                    })
                }
                _ => None,
            }
        })
        .collect()
}

/// #3661 AC2 — filter the declared set by domain (`covers`) and/or type
/// (`pyramidLayer`). `None` on either axis means no filter on that axis.
pub fn scope_rows(rows: &[TestRow], domain: Option<&str>, ttype: Option<&str>) -> Vec<TestRow> {
    rows.iter()
        .filter(|r| domain.is_none_or(|d| r.covers == d))
        .filter(|r| ttype.is_none_or(|t| r.pyramid_layer == t))
        .cloned()
        .collect()
}

/// #3661 AC1 — the units a set of DECLARED tests live in: a scoped plan derives
/// from the model rows, not from filesystem globs. Deterministic order (crates
/// sorted, then packages), deduped; rows outside any known unit are ignored
/// (this runner can't run them — the gap surface names such files instead).
pub fn plan_units_from_rows(rows: &[TestRow]) -> Vec<TestUnit> {
    let mut units: Vec<TestUnit> = rows.iter().filter_map(|r| unit_of_path(&r.file_path)).collect();
    units.sort_by_key(|u| match u {
        TestUnit::RustCrate(n) => (0, n.clone()),
        TestUnit::TsPackage(p) => (1, p.clone()),
        TestUnit::BatsSuite(s) => (2, s.clone()),
    });
    units.dedup();
    units
}

/// #3661 AC2 guard — a scoped run (--domain/--type) can only be honored by the
/// model: the scope is a model predicate, and the legacy lanes can't evaluate
/// it. Scoped + non-model plan ⇒ refuse loudly; unscoped keeps the degrade path.
pub fn scoped_requires_model(scoped: bool, plan_source: &str) -> bool {
    scoped && plan_source != "model"
}

/// #3661 AC3 — the on-disk-but-undeclared diff: test files present in the werk
/// that the tests domain does not declare. Sorted + deduped so the report (and
/// its spine event) is deterministic. Tagging SURFACES gaps, never spawns
/// structure — these are named, not auto-registered and not silently run.
pub fn undeclared_gaps(on_disk: &[String], declared: &[TestRow]) -> Vec<String> {
    let mut gaps: Vec<String> = on_disk
        .iter()
        .filter(|f| !declared.iter().any(|r| &r.file_path == *f))
        .cloned()
        .collect();
    gaps.sort();
    gaps.dedup();
    gaps
}

/// #3661 AC3 — visible either way, mirroring `quarantine_report`'s explicit-none.
pub fn gap_report(gaps: &[String]) -> String {
    if gaps.is_empty() {
        return "undeclared: none".to_string();
    }
    format!("undeclared ({} on disk, absent from the tests domain): {}", gaps.len(), gaps.join("; "))
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
    // #3974 — script suites resolve BEFORE the package loop: platform/tests
    // holds both bats suites (script units) and the cucumber package; a .bats
    // path must never collapse into the package unit.
    if path.ends_with(".bats") {
        return Some(TestUnit::BatsSuite(path.to_string()));
    }
    for pkg in TS_PACKAGES {
        if path.starts_with(&format!("{}/", pkg)) {
            return Some(TestUnit::TsPackage((*pkg).to_string()));
        }
    }
    // Shell suites (platform/scripts/test-*.sh) ride the BatsSuite variant:
    // both are "a script path run as a suite"; the executor picks bats vs
    // bash by extension.
    if let Some(name) = path.strip_prefix("platform/scripts/") {
        if name.starts_with("test-") && name.ends_with(".sh") && !name.contains('/') {
            return Some(TestUnit::BatsSuite(path.to_string()));
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
        .filter(|r| unit_of_path(&r.file_path).is_some_and(|u| legacy.contains(&u)))
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
        TestUnit::BatsSuite(s) => (2, s.clone()),
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
    // #3592 — the DAL refuses off-model properties, so the payload is EXACTLY
    // the deployed TestSuiteRunShape: cardId + ts + result. The richer run
    // context (role/trace/planSource/checks/duration) stays in the spine events
    // this verb already emits (test.started/test.completed carry all of it);
    // growing the shape is a model change, not a payload hack. Signature kept
    // so call-sites/tests don't churn when the shape grows.
    let _ = (role, trace, plan_source, checks_planned, checks_failed, duration_ms);
    format!(
        "{{\"name\":\"testsuiterun-{}-{}\",\"cardId\":\"{}\",\"ts\":{},\"result\":\"{}\"}}",
        json_escape(card), ts, json_escape(card), ts, json_escape(verdict),
    )
}

/// #3634 — the POST argv for the TestSuiteRun write-back, pure so the exact
/// curl contract (auth header, content type, endpoint, fail-fast flags) is
/// pinned by tests without a network.
pub fn suite_run_post_args(endpoint: &str, token: &str, payload: &str) -> Vec<String> {
    vec![
        "-sf".into(), "--max-time".into(), "10".into(), "-X".into(), "POST".into(),
        "-H".into(), format!("Authorization: Bearer {}", token),
        "-H".into(), "Content-Type: application/json".into(),
        // #3592 — #3573 scope enforcement: a scoped token may write only graphs
        // its scope names, and the request must NAME the target graph. Test +
        // TestResult + TestSuiteRun all resolve (via the tests domain\'s
        // definesVocabulary) to urn:chorus:domains:tests; mint_token requests
        // exactly that scope.
        "-H".into(), "x-target-graph: urn:chorus:domains:tests".into(),
        "-d".into(), payload.into(), endpoint.into(),
    ]
}

/// #3634 gather hardening (silas): minimal JSON string escaper — quotes,
/// backslashes, and control characters. The verb builds its payload by hand
/// (zero-dep blueprint), so every string field passes through here.
pub fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

/// #3634 gather hardening (wren): the plan-source label as a pure decision —
/// "model" only when the fetch succeeded AND returned rows; anything else is a
/// witnessed fallback to the legacy lanes.
pub fn plan_source_label(fetch_ok: bool, rows_len: usize) -> &'static str {
    if fetch_ok && rows_len > 0 { "model" } else { "fallback" }
}

// ---- #3592 — per-test result capture + reconcile ----

/// One executed test case, keyed to the registered chorus:Test identity
/// (filePath + testName). `result` is the graph vocab: pass | fail | skip.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct CaseResult {
    pub file_path: String,
    pub test_name: String,
    pub result: String,
}

/// Parse jq-extracted jest rows (`file\tfullName\tstatus`). Jest statuses map
/// onto the graph vocab (passed→pass, failed→fail, everything held→skip);
/// incomplete rows are dropped, never a panic (parse_test_rows discipline).
pub fn parse_case_tsv(tsv: &str) -> Vec<CaseResult> {
    tsv.lines()
        .filter_map(|l| {
            let mut it = l.trim_end().split('\t');
            match (it.next(), it.next(), it.next()) {
                (Some(f), Some(n), Some(s)) if !f.is_empty() && !n.is_empty() => Some(CaseResult {
                    file_path: f.to_string(),
                    test_name: n.to_string(),
                    result: match s {
                        "passed" => "pass",
                        "failed" => "fail",
                        _ => "skip",
                    }
                    .to_string(),
                }),
                _ => None,
            }
        })
        .collect()
}

/// Join a cargo case onto the registered inventory: the UNIQUE row whose
/// filePath sits inside the crate dir and whose registered testName equals the
/// bare fn name. Ambiguous (same fn name in two files of one crate) or missing
/// → None; the caller counts these loudly instead of guessing an identity.
pub fn match_cargo_case(
    bare: &str,
    crate_dir: &str,
    rows: &[TestRow],
    names: &[String],
) -> Option<String> {
    let hits: Vec<&TestRow> = rows
        .iter()
        .zip(names.iter())
        .filter(|(r, n)| r.file_path.starts_with(crate_dir) && n.as_str() == bare)
        .map(|(r, _)| r)
        .collect();
    match hits.as_slice() {
        [one] => Some(one.file_path.clone()),
        _ => None,
    }
}

/// Jest reports absolute paths; the registered identity is werk-relative.
pub fn rel_path(abs: &str, werk_root: &str) -> String {
    abs.strip_prefix(werk_root)
        .map(|s| s.trim_start_matches('/'))
        .unwrap_or(abs)
        .to_string()
}

/// chorus:TestResult write payload — identity fields (filePath+testName) + the
/// verdict + run context. `name` is mint-unique per (card, run-ts, index).
#[allow(clippy::too_many_arguments)]
pub fn test_result_payload(
    file_path: &str,
    test_name: &str,
    result: &str,
    of_test: &str,
    card: &str,
    role: &str,
    trace: &str,
    ts_millis: u128,
    idx: usize,
) -> String {
    // #3925 — runTs and cardId are REAL FIELDS now (shape bump: minCount 0
    // this cycle, tightened to required once the 137k-row backfill lands).
    // The minted name keeps its old form so existing joins survive, but the
    // data no longer lives ONLY smuggled inside it. runTs is offset-ISO
    // Boston (#3880), derived from the same ts_millis that keys the name —
    // one clock, two representations, never two clocks.
    // A nightly run has no card: cardId is OMITTED (typed absence), never "".
    let _ = (role, trace);
    let run_ts = boston_offset_iso(ts_millis);
    let card_field = if card.is_empty() || card == "0" {
        String::new()
    } else {
        format!(",\"cardId\":\"{}\"", json_escape(card))
    };
    format!(
        "{{\"name\":\"testresult-{}-{}-{}\",\"filePath\":\"{}\",\"testName\":\"{}\",\
         \"result\":\"{}\",\"ofTest\":\"{}\",\"runTs\":\"{}\"{}}}",
        json_escape(card), ts_millis, idx,
        json_escape(file_path), json_escape(test_name), json_escape(result), json_escape(of_test),
        run_ts, card_field,
    )
}

/// #3880 — offset-ISO in America/New_York from epoch millis, no chrono dep.
/// EST/EDT boundary via the 2007+ US rule (2nd Sun Mar / 1st Sun Nov, UTC).
pub fn boston_offset_iso(ts_millis: u128) -> String {
    let secs = (ts_millis / 1000) as i64;
    fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
        let y = if m <= 2 { y - 1 } else { y };
        let era = if y >= 0 { y } else { y - 399 } / 400;
        let yoe = (y - era * 400) as u64;
        let mp = (if m > 2 { m - 3 } else { m + 9 }) as u64;
        let doy = (153 * mp + 2) / 5 + d as u64 - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146097 + doe as i64 - 719468
    }
    fn civil(z: i64) -> (i64, u32, u32) {
        let z = z + 719468;
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u64;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let y = yoe as i64 + era * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        (if m <= 2 { y + 1 } else { y }, m, d)
    }
    fn nth_sun(y: i64, m: u32, nth: i64) -> i64 {
        let d = days_from_civil(y, m, 1);
        let first_dow = (d + 4).rem_euclid(7); // 0 = Sunday
        d + (7 - first_dow) % 7 + (nth - 1) * 7
    }
    let (y, _, _) = civil(secs.div_euclid(86400));
    let dst_start = nth_sun(y, 3, 2) * 86400 + 7 * 3600; // 2nd Sun Mar 07:00 UTC
    let dst_end = nth_sun(y, 11, 1) * 86400 + 6 * 3600; // 1st Sun Nov 06:00 UTC
    let (off, tag) = if secs >= dst_start && secs < dst_end {
        (-4 * 3600, "-04:00")
    } else {
        (-5 * 3600, "-05:00")
    };
    let local = secs + off;
    let (ly, lm, ld) = civil(local.div_euclid(86400));
    let tod = local.rem_euclid(86400);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}{}",
        ly, lm, ld, tod / 3600, (tod % 3600) / 60, tod % 60, tag
    )
}

/// registered ∖ executed, keyed (filePath, testName). Order preserved from the
/// registered side so the report is stable.
pub fn reconcile_gap(
    registered: &[(String, String)],
    executed: &[(String, String)],
) -> Vec<(String, String)> {
    let ran: std::collections::HashSet<&(String, String)> = executed.iter().collect();
    registered.iter().filter(|k| !ran.contains(k)).cloned().collect()
}

/// Visible, never silent — explicit-none style (quarantine_report/gap_report).
pub fn reconcile_report(registered_total: usize, gap: &[(String, String)]) -> String {
    if gap.is_empty() {
        return format!("reconcile: registered {}, never-run: none", registered_total);
    }
    let mut out = format!(
        "reconcile: registered {}, never-run ({}):",
        registered_total,
        gap.len()
    );
    for (f, n) in gap.iter().take(20) {
        out.push_str(&format!("\n  {} :: {}", f, n));
    }
    if gap.len() > 20 {
        out.push_str(&format!("\n  … +{} more", gap.len() - 20));
    }
    out
}

/// #3592 — 5-col fetch: `filePath\tcovers\tpyramidLayer\ttestName\tname`.
/// Returns rows + the PARALLEL testName vec + the PARALLEL minted entity-name
/// vec (same filter → same length, always aligned). Shorter rows get empty
/// strings (legacy fetch back-compat). The entity name is what a TestResult's
/// required ofTest edge must reference.
pub fn parse_rows_and_names(tsv: &str) -> (Vec<TestRow>, Vec<String>, Vec<String>) {
    let mut rows = Vec::new();
    let mut names = Vec::new();
    let mut entities = Vec::new();
    for l in tsv.lines() {
        let mut it = l.trim_end().split('\t');
        if let (Some(f), Some(c)) = (it.next(), it.next()) {
            if !f.trim().is_empty() && !c.trim().is_empty() {
                rows.push(TestRow {
                    file_path: f.trim().to_string(),
                    covers: c.trim().to_string(),
                    pyramid_layer: it.next().map(|x| x.trim().to_string()).unwrap_or_default(),
                    hermeticity: String::new(),
                    test_concern: String::new(),
                });
                names.push(it.next().map(|x| x.trim().to_string()).unwrap_or_default());
                entities.push(it.next().map(|x| x.trim().to_string()).unwrap_or_default());
                if let Some(h) = it.next() {
                    if let Some(r) = rows.last_mut() {
                        r.hermeticity = h.trim().to_string();
                    }
                }
                if let Some(tc) = it.next() {
                    if let Some(r) = rows.last_mut() {
                        r.test_concern = tc.trim().to_string();
                    }
                }
            }
        }
    }
    (rows, names, entities)
}

/// #3592 — join executed cases onto the registered inventory by identity
/// (filePath, testName) → the registered entity name (for the mandatory ofTest
/// edge). Unjoined cases come back separately — counted loudly, never posted
/// with a fabricated identity.
pub fn join_cases(
    cases: &[CaseResult],
    rows: &[TestRow],
    test_names: &[String],
    entities: &[String],
) -> (Vec<(CaseResult, String)>, usize) {
    let mut index: std::collections::HashMap<(&str, &str), &str> = std::collections::HashMap::new();
    for ((r, n), e) in rows.iter().zip(test_names.iter()).zip(entities.iter()) {
        if !e.is_empty() {
            index.insert((r.file_path.as_str(), n.as_str()), e.as_str());
        }
    }
    let mut joined = Vec::new();
    let mut unjoined = 0usize;
    for c in cases {
        match index.get(&(c.file_path.as_str(), c.test_name.as_str())) {
            Some(e) => joined.push((c.clone(), (*e).to_string())),
            None => unjoined += 1,
        }
    }
    (joined, unjoined)
}

/// #3787 — run the workspace ESLint ratchet (`lint-ratchet.js`) for the werk.
/// The ESLint mirror of `run_clippy_ratchet`: counts may only decrease; a rule
/// pushed over baseline REFUSES the land with rule+count+file:line in the
/// script's output. `LINT_RATCHET_ROOT` pins measurement to the werk (#3701:
/// the session env otherwise points ratchets at canonical, measuring main
/// instead of this card's diff). A fresh worktree has no root `node_modules`,
/// so eslint falls back to canonical's binary via `$CHORUS_HOME` unless the
/// caller already pinned `LINT_RATCHET_ESLINT_BIN` (hermetic tests do).
/// Missing script + missing baseline = a pre-#3787 tree — pass. Missing script
/// with the baseline PRESENT = the ratchet was deleted out of a post-#3787
/// tree — refuse loudly, never pass vacuously (#3734: a guard whose target is
/// deleted must fail, not evaporate).
pub fn run_lint_ratchet(werk: &str) -> bool {
    let script = format!("{}/platform/scripts/lint-ratchet.js", werk);
    if !std::path::Path::new(&script).is_file() {
        let baseline = format!("{}/.eslint-baseline.json", werk);
        if std::path::Path::new(&baseline).is_file() {
            eprintln!(
                "lint-ratchet: REFUSED — {} carries .eslint-baseline.json but platform/scripts/lint-ratchet.js is missing (guard target deleted, #3734)",
                werk
            );
            return false;
        }
        return true;
    }
    let mut cmd = std::process::Command::new("node");
    cmd.arg(&script).current_dir(werk).env("LINT_RATCHET_ROOT", werk);
    let werk_eslint = format!("{}/node_modules/.bin/eslint", werk);
    if std::env::var("LINT_RATCHET_ESLINT_BIN").is_err()
        && !std::path::Path::new(&werk_eslint).is_file()
    {
        if let Ok(home) = std::env::var("CHORUS_HOME") {
            let canon = format!("{}/node_modules/.bin/eslint", home);
            if std::path::Path::new(&canon).is_file() {
                cmd.env("LINT_RATCHET_ESLINT_BIN", canon);
            }
        }
    }
    cmd.status().map(|s| s.success()).unwrap_or(false)
}

// ---- #3808 — wire-back token re-mint: expiry mid-stream is recoverable ----

/// What to do with one case-post's HTTP status (#3808). Decision table, not
/// policy-in-the-loop: 2xx accepts; the FIRST 401 of a run re-mints (the token
/// outlived chorus-identity-token's ~600s cache mid-stream — measured on #3802,
/// 594 posted then 534 401s); a 401 AFTER re-minting means the identity itself
/// is refused — fail loudly, never retry forever. Anything else (5xx, 000) is a
/// server-side refusal a fresh token cannot cure.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum PostStep {
    Accept,
    Remint,
    Fail,
}

pub fn remint_decision(http_code: &str, reminted_already: bool) -> PostStep {
    if http_code.starts_with('2') {
        return PostStep::Accept;
    }
    if http_code == "401" && !reminted_already {
        return PostStep::Remint;
    }
    PostStep::Fail
}

/// #3808 — per-case POST argv that YIELDS the status code (`-w %{http_code}`,
/// stdout) instead of `-sf`'s swallow-and-exit — the 401 trigger needs the code,
/// and this retires #3725's second diagnostic request per failure.
pub fn post_args_with_code(endpoint: &str, token: &str, payload: &str) -> Vec<String> {
    vec![
        "-s".into(), "-o".into(), "/dev/null".into(), "-w".into(), "%{http_code}".into(),
        "--max-time".into(), "10".into(), "-X".into(), "POST".into(),
        "-H".into(), format!("Authorization: Bearer {}", token),
        "-H".into(), "Content-Type: application/json".into(),
        "-H".into(), "x-target-graph: urn:chorus:domains:tests".into(),
        "-d".into(), payload.into(), endpoint.into(),
    ]
}

/// Outcome of a posting loop (#3808) — what the caller banners + puts on the
/// spine. `remint_attempts` counts re-mint ATTEMPTS (0 or 1 by construction)
/// — an attempt fires per expiry-shaped 401 whether or not the mint succeeded,
/// so the field measures expiry frequency, not recovery success (reviewers'
/// catch on the first round: a mint that refused still counted).
#[derive(Debug, Default, PartialEq, Eq)]
pub struct PostStats {
    pub posted: usize,
    pub failed: usize,
    pub remint_attempts: usize,
    pub first_fail_code: Option<String>,
}

/// #3808 — the posting loop with expired-token recovery. Pure orchestration
/// over curl: posts each payload, and on the first 401 asks `mint` for a fresh
/// token ONCE, retries the same case, and resumes. A 401 that survives the
/// re-mint — or a `mint` that refuses — is a real refusal: every remaining case
/// counts failed (loud banner + spine at the caller), never an endless retry.
pub fn post_results_loop(
    endpoint: &str,
    initial_token: &str,
    payloads: &[String],
    mint: &dyn Fn() -> Option<String>,
) -> PostStats {
    let mut stats = PostStats::default();
    let mut token = initial_token.to_string();
    let mut reminted_already = false;
    for payload in payloads {
        let mut code = post_once(endpoint, &token, payload);
        if remint_decision(&code, reminted_already) == PostStep::Remint {
            stats.remint_attempts += 1;
            reminted_already = true;
            if let Some(fresh) = mint() {
                token = fresh;
                code = post_once(endpoint, &token, payload);
            }
        }
        match remint_decision(&code, reminted_already) {
            PostStep::Accept => stats.posted += 1,
            _ => {
                stats.failed += 1;
                if stats.first_fail_code.is_none() {
                    stats.first_fail_code = Some(code);
                }
            }
        }
    }
    stats
}

fn post_once(endpoint: &str, token: &str, payload: &str) -> String {
    std::process::Command::new("curl")
        .args(post_args_with_code(endpoint, token, payload))
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_else(|_| "000".to_string())
}

// #3821 — the ONE shared diff→units scoping core (also compiled by werk-build).
include!("../../shared/scope_units.rs");

/// #3821 — the diff-scoped TEST plan. Some(units) = scoped (possibly empty:
/// an asset-only diff runs nothing); None = FULL fallback (unmapped file,
/// empty diff, or forced) — the caller logs the reason and runs the legacy
/// widened plan. Same core as werk-build's scoping, so what a diff can affect
/// is one answer asked twice.
pub fn scoped_test_units(
    changed: &[String],
    units: &[ScopeUnit],
    edges: &[(String, String)],
) -> Option<Vec<ScopeUnit>> {
    let force_full = std::env::var("WERK_TEST_FULL").map(|v| v == "1").unwrap_or(false);
    match scope_unit_names(changed, units, edges, force_full) {
        ScopeVerdict::Scoped(names) => Some(
            units.iter().filter(|u| names.contains(&u.name)).cloned().collect(),
        ),
        ScopeVerdict::Full(_) => None,
    }
}

/// #3892 — the runner hands every spawned suite a self-contained world so a
/// lazy subprocess test resolving a prod surface lands in a tempdir instead
/// of a MEMBRANE REFUSED panic in the werk log. Three misdiagnoses in 48h
/// came from those panic lines outshouting the fatal step (Wren on #3863,
/// Kade twice on #3884) — and they appeared in GREEN runs too.
///
/// Deliberately NOT overridden (the membrane's teeth stay in):
/// - FUSEKI_URL — integration tests gate on RUN_LIVE_INTEGRATION explicitly
/// - CHORUS_HOME — redirecting it re-homes far more than test writes
/// - RoleState — has no override by design; touching it must stay loud
pub fn suite_world_env(tmp: &str) -> Vec<(String, String)> {
    vec![
        ("CHORUS_LOG_FILE".into(), format!("{tmp}/chorus.log")),
        ("CHORUS_SESSIONS_DIR".into(), format!("{tmp}/sessions")),
        ("CHORUS_DB_PATH".into(), format!("{tmp}/index.db")),
        ("CHORUS_LANCE_DIR".into(), format!("{tmp}/lance")),
        ("CHORUS_MESSAGES_DB".into(), format!("{tmp}/messages.db")),
        // #3995 — the HTTP lane: the membrane guards stores via env seams, but a
        // suite POSTing to a live service walks around it (a werk-demo e2e paged
        // Jeff's live Clearing twice, 2026-08-23). Dead-port every outbound-HTTP
        // seam: 127.0.0.1:9 (discard) refuses instantly, so a leaky suite fails
        // loudly instead of speaking with the team's live voice. Genuinely-live
        // integration tests gate via RUN_LIVE_INTEGRATION and set their own URLs.
        ("CHORUS_MCP_URL".into(), "http://127.0.0.1:9/mcp".into()),
        ("CHORUS_BRIDGE_URL".into(), "http://127.0.0.1:9/api/message".into()),
        // #4005 — PULSE_URL escaped #3995's cage: a spawned Clearing forwards
        // Jeff-input to it, defaulting to the LIVE pulse :3475, so a test marker
        // reached Jeff's session 12h after the cage landed. Same dead port.
        ("PULSE_URL".into(), "http://127.0.0.1:9".into()),
    ]
}

#[cfg(test)]
mod suite_world_tests {
    use super::suite_world_env;

    /// #3995 negative proof (#3734): under the suite world, an HTTP POST to the
    /// seam URL must REFUSE — the violation state (a suite reaching a live
    /// service) is shown unreachable, not assumed.
    /// #4005 — the cage must cover EVERY outbound seam a spawned suite can
    /// reach, not most of them: PULSE_URL was missing and a marker walked into
    /// Jeff's live session 12h after #3995. Negative proof: the seam value is
    /// a dead port AND a POST to it refuses.
    #[test]
    fn pulse_seam_is_caged_and_refuses() {
        let env = suite_world_env("/tmp/w");
        let pulse = env.iter().find(|(k, _)| k == "PULSE_URL")
            .expect("PULSE_URL must be in the suite world (#4005)").1.clone();
        assert!(pulse.starts_with("http://127.0.0.1:9"), "dead port, not live pulse: {pulse}");
        assert!(!pulse.contains("3475"), "the LIVE pulse must never be the suite-world default");
        let rc = std::process::Command::new("curl")
            .args(["-s", "-m", "2", "-X", "POST", &format!("{pulse}/api/jeff-input"), "-d", "{}"])
            .status()
            .expect("curl spawns");
        assert!(!rc.success(), "POST to the suite-world pulse seam must refuse — else a test can page Jeff");
    }

    #[test]
    fn http_seams_are_dead_ported_and_refuse() {
        let env = suite_world_env("/tmp/w");
        let mcp = env.iter().find(|(k, _)| k == "CHORUS_MCP_URL").expect("mcp seam present").1.clone();
        assert!(mcp.starts_with("http://127.0.0.1:9/"), "dead port, not a live default: {mcp}");
        let rc = std::process::Command::new("curl")
            .args(["-s", "-m", "2", "-X", "POST", &mcp, "-d", "{}"])
            .status()
            .expect("curl spawns");
        assert!(!rc.success(), "POST to the suite-world MCP seam must refuse (got success — a suite could page Jeff)");
    }

    #[test]
    fn overridable_surfaces_point_into_the_tempdir() {
        let env = suite_world_env("/tmp/w");
        let keys: Vec<&str> = env.iter().map(|(k, _)| k.as_str()).collect();
        for k in ["CHORUS_LOG_FILE", "CHORUS_SESSIONS_DIR", "CHORUS_DB_PATH", "CHORUS_LANCE_DIR", "CHORUS_MESSAGES_DB"] {
            assert!(keys.contains(&k), "missing {k}");
        }
        // #3995 — path seams live in the tempdir; the HTTP seams are dead-ports,
        // not paths. Every value is one or the other, nothing may point at prod.
        assert!(
            env.iter().all(|(_, v)| v.starts_with("/tmp/w/") || v.starts_with("http://127.0.0.1:9")),
            "{env:?}"
        );
    }

    #[test]
    fn membrane_teeth_stay_negative_proof() {
        // #3734 — the surfaces whose refusal must SURVIVE this change are
        // provably absent from the override set.
        let env = suite_world_env("/tmp/w");
        for k in ["FUSEKI_URL", "CHORUS_HOME"] {
            assert!(!env.iter().any(|(key, _)| key == k), "{k} must not be overridden");
        }
    }
}

// ── #3912 phase 1 — jest selection from the registry ────────────────────────
// Jeff: "pipelines that pull registered tests." The jest leg stops running
// whole packages from TS_PACKAGES; it runs the REGISTERED unit-layer test
// files that cover the diff. Mechanical coverage comes from jest's own
// import graph (--findRelatedTests); the registry is the authority on what
// is a unit test at all. Selection = related ∩ registered(unit).
//
// Fallback contract (AC): registry unreachable → FULL package run, loudly
// labeled — never a silent narrow. Under-selection is the failure the
// negative fixture guards: a test registered as covering a changed file's
// domain must appear when jest names it related.

/// Structural TS-package resolution — NO hardcoded list (TS_PACKAGES is the
/// surface this phase retires): the package is the path prefix before the
/// first `tests/` or `src/` segment. `directing/clearing/tests/x.test.ts` →
/// `directing/clearing`. Registry rows always carry such paths.
pub fn ts_package_of(path: &str) -> Option<String> {
    let parts: Vec<&str> = path.split('/').collect();
    let cut = parts.iter().position(|s| *s == "tests" || *s == "src")?;
    if cut == 0 {
        return None;
    }
    Some(parts[..cut].join("/"))
}

/// One selected jest invocation: package dir + the test files to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JestSelection {
    pub package: String,
    pub test_files: Vec<String>,
}

/// Intersect jest-related test files with the registry's unit-layer rows.
/// `related` are repo-relative test file paths (from --findRelatedTests
/// --listTests, normalized). Returns per-package selections, deterministic
/// order. Files jest names related but the registry doesn't know stay OUT of
/// the selection — they surface via the undeclared-gap channel instead of
/// silently riding (registration is the contract, #3190 tagging rule).
pub fn jest_selection(rows: &[TestRow], related: &[String]) -> Vec<JestSelection> {
    let registered_unit: std::collections::BTreeSet<&str> = rows
        .iter()
        .filter(|r| r.pyramid_layer == "unit")
        .map(|r| r.file_path.as_str())
        .collect();
    let mut by_pkg: std::collections::BTreeMap<String, Vec<String>> = Default::default();
    for f in related {
        if !registered_unit.contains(f.as_str()) {
            continue;
        }
        if let Some(p) = ts_package_of(f) {
            by_pkg.entry(p).or_default().push(f.clone());
        }
    }
    by_pkg
        .into_iter()
        .map(|(package, mut test_files)| {
            test_files.sort();
            test_files.dedup();
            JestSelection { package, test_files }
        })
        .collect()
}

/// The fallback decision, typed so the label can't be forgotten: a selection
/// plan is either Selected (registry answered) or FullFallback with the
/// REASON the registry could not be honored.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JestPlan {
    Selected(Vec<JestSelection>),
    FullFallback { reason: String },
}

/// Build the jest plan: registry rows present → selection (possibly empty —
/// an empty selection is a VALID answer meaning no registered unit test
/// covers the diff); rows absent/unfetchable → full fallback, reason carried.
pub fn jest_plan(rows_fetched: bool, rows: &[TestRow], related: &[String]) -> JestPlan {
    if !rows_fetched {
        return JestPlan::FullFallback {
            reason: "registry unreachable — running FULL jest per affected package (loud fallback, #3912)".to_string(),
        };
    }
    JestPlan::Selected(jest_selection(rows, related))
}

#[cfg(test)]
mod jest_selection_3912 {
    use super::*;

    fn row(f: &str, layer: &str) -> TestRow {
        TestRow { file_path: f.into(), covers: "messages-domain".into(), pyramid_layer: layer.into(), hermeticity: String::new(), test_concern: String::new() }
    }

    #[test]
    fn selection_is_related_intersect_registered_unit() {
        let rows = vec![
            row("directing/clearing/tests/tiles.test.ts", "unit"),
            row("directing/clearing/tests/room.e2e.test.ts", "e2e"),
        ];
        let related = vec![
            "directing/clearing/tests/tiles.test.ts".to_string(),
            "directing/clearing/tests/room.e2e.test.ts".to_string(), // e2e: not unit-layer
            "directing/clearing/tests/unregistered.test.ts".to_string(), // unknown to registry
        ];
        let sel = jest_selection(&rows, &related);
        assert_eq!(sel.len(), 1);
        assert_eq!(sel[0].package, "directing/clearing");
        assert_eq!(sel[0].test_files, vec!["directing/clearing/tests/tiles.test.ts"]);
    }

    #[test]
    fn under_selection_negative_proof() {
        // #3734 — the guarded condition: a registered unit test jest names
        // related MUST be selected. If this assertion can pass with the file
        // absent, the selector is under-selecting and the gate is hollow.
        let rows = vec![row("directing/clearing/tests/spine-tail.test.ts", "unit")];
        let related = vec!["directing/clearing/tests/spine-tail.test.ts".to_string()];
        let sel = jest_selection(&rows, &related);
        assert!(
            sel.iter().any(|s| s.test_files.iter().any(|f| f.ends_with("spine-tail.test.ts"))),
            "known covering test was dropped: under-selection"
        );
    }

    #[test]
    fn registry_unreachable_is_loud_full_fallback() {
        let plan = jest_plan(false, &[], &[]);
        match plan {
            JestPlan::FullFallback { reason } => assert!(reason.contains("registry unreachable")),
            _ => panic!("must fall back FULL when registry is unreachable"),
        }
    }

    #[test]
    fn empty_selection_is_a_valid_answer_not_a_fallback() {
        let rows = vec![row("directing/clearing/tests/tiles.test.ts", "unit")];
        let plan = jest_plan(true, &rows, &[]);
        assert_eq!(plan, JestPlan::Selected(vec![]));
    }
}

#[cfg(test)]
mod ts_packages_retirement_3912 {
    /// #3912 retirement gate — the SELECTION path must stay free of the
    /// hardcoded TS_PACKAGES list (the five-drifting-lists disease). This
    /// reads the crate's own source: the jest-selection region (from
    /// `pub fn ts_package_of` to the end of `jest_plan`) may not reference
    /// TS_PACKAGES. If someone re-couples selection to the list, this goes
    /// RED — and if the region markers are renamed it fails LOUDLY on the
    /// missing marker rather than passing vacuously (#3734).
    #[test]
    fn jest_selection_region_never_consults_ts_packages() {
        let src = include_str!("lib.rs");
        let start = src.find("pub fn ts_package_of").expect("region start marker missing — retirement gate must fail loud, not vacuously");
        let end = src[start..].find("mod jest_selection_3912").map(|i| start + i).expect("region end marker missing — retirement gate must fail loud, not vacuously");
        let region = &src[start..end];
        assert!(
            !region.contains("TS_PACKAGES"),
            "jest selection re-coupled to TS_PACKAGES — the retired list is back"
        );
    }
}

// ---------------------------------------------------------------------------
// #3917 — the bats/script blind spot.
//
// Before this, `affected_units` recognised exactly two shapes: a Rust crate and
// a TS package. A card whose whole diff was `platform/tests/*.bats` and
// `platform/scripts/*.sh` therefore selected ZERO units and the blocking gate
// exited 0 — not because nothing needed running, but because nothing could be
// seen. #3915's own land proved it: five bats suites fixing nightly reds landed
// under a green `werk-test: no-affected-units`.
// ---------------------------------------------------------------------------

/// One row of the script→suite coverage index: a bats suite and the repo-relative
/// script paths its body references. Built by the runner (which may touch the fs);
/// kept out of the pure selection so selection stays testable without a repo.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct SuiteCoverage {
    pub suite: String,
    pub covers: Vec<String>,
}

/// Is this changed path a bats suite?
pub fn is_bats_suite(path: &str) -> bool {
    path.starts_with("platform/tests/") && path.ends_with(".bats")
}

/// Is this changed path a shell script the gate should account for?
pub fn is_shell_script(path: &str) -> bool {
    path.ends_with(".sh") || path.starts_with("platform/scripts/")
}

/// #3934 — a GOVERNED SURFACE: a non-script file whose content a suite asserts
/// on. #3917 taught the gate to see changed test FILES; it still could not see
/// what a test is ABOUT. A one-line change to `.github/workflows/werk.yml`
/// (#3918) implicated no test, so the land was green and the 04:19 nightly was
/// red on `membrane-guard.bats` — a suite that greps that exact line.
pub fn is_governed_surface(path: &str) -> bool {
    path.starts_with(".github/workflows/") && (path.ends_with(".yml") || path.ends_with(".yaml"))
        || path.starts_with("platform/hooks/")
}

/// Any changed path a suite may reference by name: scripts plus governed surfaces.
pub fn is_referenceable(path: &str) -> bool {
    (is_shell_script(path) || is_governed_surface(path)) && !is_bats_suite(path)
}

/// Bats suites implicated by the diff: every changed suite itself, plus every
/// suite whose body references a changed script. Deterministic (sorted, deduped).
pub fn affected_bats_suites(changed: &[String], index: &[SuiteCoverage]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in changed {
        if is_bats_suite(f) && !out.contains(f) {
            out.push(f.clone());
        }
    }
    for f in changed {
        if !is_referenceable(f) {
            continue;
        }
        for row in index {
            if row.covers.iter().any(|c| c == f) && !out.contains(&row.suite) {
                out.push(row.suite.clone());
            }
        }
    }
    out.sort();
    out
}

/// The full selection: the pre-#3917 crate/package units PLUS the bats suites
/// the diff implicates. `affected_units` is left untouched so the old shape
/// stays independently testable (and so the regression test can show what it
/// could not see).
pub fn affected_units_full(changed: &[String], index: &[SuiteCoverage]) -> Vec<TestUnit> {
    let mut units = affected_units(changed);
    for suite in affected_bats_suites(changed, index) {
        units.push(TestUnit::BatsSuite(suite));
    }
    units
}

/// Changed shell scripts that NO suite covers. AC2: these are reported loudly
/// rather than dissolving into an empty selection — an uncovered script is a
/// known gap, not a pass.
pub fn uncovered_scripts(changed: &[String], index: &[SuiteCoverage]) -> Vec<String> {
    let mut out: Vec<String> = changed
        .iter()
        .filter(|f| is_referenceable(f))
        .filter(|f| !index.iter().any(|r| r.covers.iter().any(|c| c == *f)))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

#[cfg(test)]
mod bats_selection_3917 {
    use super::*;

    fn index() -> Vec<SuiteCoverage> {
        vec![
            SuiteCoverage {
                suite: "platform/tests/gate-spine-vikunja-e2e.bats".to_string(),
                covers: vec!["platform/scripts/gate-spine-vikunja-bridge.sh".to_string()],
            },
            SuiteCoverage {
                suite: "platform/tests/nudge-health.bats".to_string(),
                covers: vec!["platform/scripts/nudge-health.sh".to_string()],
            },
        ]
    }

    /// The exact file list #3915 landed with. This is the regression: it MUST
    /// select units now, and it selected none before.
    #[test]
    fn card_3915_file_list_now_selects_its_suites() {
        let changed: Vec<String> = [
            "platform/scripts/gate-spine-vikunja-bridge.sh",
            "platform/tests/3370-lan-ip-baseline.txt",
            "platform/tests/3370-no-new-hardcoded-lan-ips.bats",
            "platform/tests/gate-spine-vikunja-e2e.bats",
            "platform/tests/nudge-health.bats",
            "platform/tests/products-3603-migration.bats",
            "platform/tests/session-start-orchestration-e2e.bats",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();

        let suites = affected_bats_suites(&changed, &index());
        assert_eq!(suites.len(), 5, "expected all five changed suites, got {:?}", suites);
        let units = affected_units_full(&changed, &index());
        assert!(!units.is_empty(), "#3915's diff must not select zero units");
        assert_eq!(gate_outcome(units.len(), false, false), GateOutcome::Pass);
    }

    /// NEGATIVE PROOF (#3734, AC3): with the fix in place a RED suite must
    /// BLOCK. If this ever passes as non-blocking, the gate is decorative.
    #[test]
    fn a_red_bats_suite_blocks_the_land() {
        let changed = vec!["platform/tests/nudge-health.bats".to_string()];
        let units = affected_units_full(&changed, &index());
        assert_eq!(units.len(), 1);
        assert_eq!(
            gate_outcome(units.len(), true, false),
            GateOutcome::Block,
            "a failing bats suite must block, not advise"
        );
    }

    /// NEGATIVE PROOF: the OLD selection could not do this. Guards against a
    /// future refactor quietly dropping bats back out of the unit shapes.
    #[test]
    fn bats_only_diff_is_not_no_units() {
        let changed = vec!["platform/tests/session-start-orchestration-e2e.bats".to_string()];
        assert!(
            affected_units(&changed).is_empty(),
            "documents the old behaviour this card fixes"
        );
        let units = affected_units_full(&changed, &index());
        assert_ne!(
            gate_outcome(units.len(), false, false),
            GateOutcome::NoUnits,
            "a bats-only diff must be measured, not waved through"
        );
    }

    /// A changed script pulls in the suite that covers it (AC2, covered half).
    #[test]
    fn changed_script_pulls_in_its_covering_suite() {
        let changed = vec!["platform/scripts/gate-spine-vikunja-bridge.sh".to_string()];
        let suites = affected_bats_suites(&changed, &index());
        assert_eq!(suites, vec!["platform/tests/gate-spine-vikunja-e2e.bats"]);
        assert!(uncovered_scripts(&changed, &index()).is_empty());
    }

    /// An UNCOVERED script is named, not silently dropped (AC2, uncovered half).
    #[test]
    fn uncovered_script_is_named_not_silently_dropped() {
        let changed = vec!["platform/scripts/no-suite-touches-this.sh".to_string()];
        assert!(affected_bats_suites(&changed, &index()).is_empty());
        assert_eq!(
            uncovered_scripts(&changed, &index()),
            vec!["platform/scripts/no-suite-touches-this.sh"]
        );
    }

    /// A bats suite is planned as a bats check — not silently unplanned.
    #[test]
    fn bats_unit_gets_a_bats_check() {
        let units = vec![TestUnit::BatsSuite("platform/tests/nudge-health.bats".to_string())];
        let plan = check_plan(&units);
        assert!(plan.iter().any(|p| p.kind == CheckKind::Bats));
    }

    /// AC4: selecting zero units must not read the same as passing units.
    #[test]
    fn no_units_label_does_not_read_as_green() {
        let label = GateOutcome::NoUnits.label();
        assert!(
            label.contains("0"),
            "NoUnits must state that nothing was measured, got {:?}",
            label
        );
        assert_ne!(label, GateOutcome::Pass.label());
    }
}

// ---------------------------------------------------------------------------
// #3918 — the land lane's own telemetry.
//
// #3745 wrote the principle down: "The prod declaration is the RUNNER's identity,
// never the tests'." It implemented that by clearing CHORUS_CONTEXT on the shell
// line that invokes werk-test — which clears it for the RUNNER too. So werk-test
// runs under act's CI=true with no declaration, the membrane classifies it Build,
// and every spine event it emits panics on the way out. The land happens; the
// record does not. Same principle, one level lower: the runner keeps its prod
// identity, and clears the variable per spawned test child.
// ---------------------------------------------------------------------------

/// What a spawned child should inherit for `CHORUS_CONTEXT`.
///
/// `None` = leave the runner's own value in place (the runner IS prod work).
/// `Some("")` = explicitly cleared, so the child classifies from its own ambient
/// markers (BATS_TEST_TMPDIR / CARGO / NODE_ENV) exactly as #3745 intended.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum ChildContext {
    /// A test child: clear the declaration so the membrane sees the test.
    Test,
    /// A runner-side child (chorus-log, git, curl): keep the runner's identity.
    Runner,
}

impl ChildContext {
    /// The value to set on the child, or None to leave inherited.
    pub fn env_value(self) -> Option<&'static str> {
        match self {
            ChildContext::Test => Some(""),
            ChildContext::Runner => None,
        }
    }
}

/// Classify a spawned command by what it IS, not where it runs. The check that
/// decides whether a process may write the production spine has to be able to
/// separate these two — that is the whole defect.
pub fn child_context(program: &str) -> ChildContext {
    match program {
        "cargo" | "bats" => ChildContext::Test,
        p if p.ends_with("jest") || p.ends_with("tsc") => ChildContext::Test,
        _ => ChildContext::Runner,
    }
}

#[cfg(test)]
mod governed_surface_selection_3934 {
    use super::*;

    fn index() -> Vec<SuiteCoverage> {
        vec![
            SuiteCoverage {
                suite: "platform/tests/membrane-guard.bats".to_string(),
                covers: vec![".github/workflows/werk.yml".to_string()],
            },
            SuiteCoverage {
                suite: "platform/tests/gate-spine-vikunja-e2e.bats".to_string(),
                covers: vec!["platform/scripts/gate-spine-vikunja-bridge.sh".to_string()],
            },
        ]
    }

    /// The #3918 regression: its diff changed werk.yml, membrane-guard.bats
    /// asserts on that exact line, and NOTHING connected them. The land was
    /// green; the nightly was red.
    #[test]
    fn a_werkyml_change_now_selects_the_suite_that_asserts_on_it() {
        let changed = vec![".github/workflows/werk.yml".to_string()];
        let suites = affected_bats_suites(&changed, &index());
        assert_eq!(suites, vec!["platform/tests/membrane-guard.bats"]);
        let units = affected_units_full(&changed, &index());
        assert_ne!(gate_outcome(units.len(), false, false), GateOutcome::NoUnits);
    }

    /// NEGATIVE PROOF (#3734): widening must not become select-everything. A
    /// governed surface NO suite references still selects nothing — and is
    /// named as an uncovered gap rather than passing silently.
    #[test]
    fn an_unreferenced_governed_surface_selects_nothing_and_is_named() {
        let changed = vec![".github/workflows/nobody-tests-this.yml".to_string()];
        assert!(affected_bats_suites(&changed, &index()).is_empty());
        assert_eq!(
            uncovered_scripts(&changed, &index()),
            vec![".github/workflows/nobody-tests-this.yml"]
        );
    }

    /// NEGATIVE PROOF: ordinary source files are NOT governed surfaces — this
    /// must not quietly turn every .ts/.rs change into a bats sweep.
    #[test]
    fn ordinary_source_is_not_a_governed_surface() {
        for p in [
            "platform/api/src/server.ts",
            "platform/services/werk-test/src/lib.rs",
            "designing/schemas/spine-events.json",
            "knowledge/doc-coherence.md",
        ] {
            assert!(!is_governed_surface(p), "{} must not be governed", p);
            assert!(!is_referenceable(p), "{} must not be referenceable", p);
        }
    }

    /// A bats suite is never ALSO a referenceable target — it is the unit.
    #[test]
    fn a_suite_is_not_its_own_coverage_target() {
        assert!(!is_referenceable("platform/tests/membrane-guard.bats"));
    }

    /// The #3917 script path still works — widening did not replace it.
    #[test]
    fn script_coverage_still_selects() {
        let changed = vec!["platform/scripts/gate-spine-vikunja-bridge.sh".to_string()];
        assert_eq!(
            affected_bats_suites(&changed, &index()),
            vec!["platform/tests/gate-spine-vikunja-e2e.bats"]
        );
    }
}

#[cfg(test)]
mod land_lane_telemetry_3918 {
    use super::*;

    /// The runner's own emissions keep prod identity — this is the bug: they
    /// were being cleared along with the tests', so they died on the membrane.
    #[test]
    fn the_runners_own_spine_emit_keeps_prod_identity() {
        assert_eq!(child_context("bash"), ChildContext::Runner);
        assert_eq!(child_context("bash").env_value(), None);
    }

    /// NEGATIVE PROOF (#3734), the other half: a genuine test child is STILL
    /// cleared. If this ever returns Runner, the membrane's teeth are gone and
    /// a test could write the production spine — the exact thing #3615 exists
    /// to prevent. Both directions, or the distinction is unproven.
    #[test]
    fn a_real_test_child_is_still_cleared_and_therefore_still_refused() {
        for p in ["cargo", "bats", "/usr/local/bin/jest", "node_modules/.bin/tsc"] {
            assert_eq!(child_context(p), ChildContext::Test, "{} must classify as test", p);
            assert_eq!(child_context(p).env_value(), Some(""), "{} must be cleared", p);
        }
    }

    /// The classifier keys on what the child IS. A runner-side helper that
    /// merely has a test-ish name must not be mistaken for a test.
    #[test]
    fn runner_helpers_are_not_tests() {
        for p in ["git", "curl", "jq", "gh"] {
            assert_eq!(child_context(p), ChildContext::Runner, "{} is runner-side", p);
        }
    }
}

// ── #3931 — the selection is VISIBLE: what ran, and why each thing ran ───────
//
// The runner used to print a count ("3 registered unit test file(s) cover the
// diff") — nobody could see whether it selected the RIGHT three. This record
// names every selected file with its reason, and names a fallback's reason in
// the same shape, so an under-selection is inspectable from the record alone.

/// One line of selection evidence: test file + why it was chosen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectionDetail {
    pub file: String,
    pub reason: String, // "related-file+registered" | "full-fallback: <why>"
}

/// Flatten a jest plan into its per-file evidence. Pure — the emit site just
/// serializes this. A Selected plan yields one row per file (the only path
/// into a selection today is jest --findRelatedTests ∩ registry, so the
/// reason is uniform); a FullFallback yields ONE row with the fallback reason
/// so the record never reads as "nothing happened".
pub fn selection_details(plan: &JestPlan) -> Vec<SelectionDetail> {
    match plan {
        JestPlan::Selected(sels) => sels
            .iter()
            .flat_map(|s| s.test_files.iter().map(|f| SelectionDetail {
                file: f.clone(),
                reason: "related-file+registered".to_string(),
            }))
            .collect(),
        JestPlan::FullFallback { reason } => vec![SelectionDetail {
            file: "*".to_string(),
            reason: format!("full-fallback: {}", reason),
        }],
    }
}

#[cfg(test)]
mod selection_visibility_tests {
    use super::*;

    #[test]
    fn selected_plan_names_every_file_with_its_reason() {
        let plan = JestPlan::Selected(vec![JestSelection {
            package: "platform/api".into(),
            test_files: vec!["platform/api/tests/a.test.ts".into(), "platform/api/tests/b.test.ts".into()],
        }]);
        let d = selection_details(&plan);
        assert_eq!(d.len(), 2);
        assert!(d.iter().all(|r| r.reason == "related-file+registered"));
        assert!(d.iter().any(|r| r.file.ends_with("a.test.ts")));
    }

    #[test]
    fn fallback_names_its_reason_never_empty() {
        // NEGATIVE PROOF (#3734): the failure mode this exists for is a record
        // that reads as empty when the runner actually ran EVERYTHING. A
        // fallback must produce a row carrying why.
        let plan = JestPlan::FullFallback { reason: "tests-domain-unreachable".into() };
        let d = selection_details(&plan);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].file, "*");
        assert!(d[0].reason.contains("tests-domain-unreachable"));
    }

    #[test]
    fn empty_selection_is_a_valid_visible_answer() {
        // Zero files selected is an ANSWER (no registered unit test covers the
        // diff) — the record is empty-by-truth, distinct from fallback.
        let d = selection_details(&JestPlan::Selected(vec![]));
        assert!(d.is_empty());
    }
}

#[cfg(test)]
mod test_result_clock_tests {
    use super::*;

    #[test]
    fn payload_carries_run_ts_and_card_as_real_fields() {
        let p = test_result_payload("a/b.test.ts", "case one", "pass", "test-x", "3925", "kade", "t", 1787264000000, 0);
        assert!(p.contains("\"runTs\":\"2026-08-"), "offset-ISO runTs present: {p}");
        assert!(p.contains("-04:00"), "August is EDT: {p}");
        assert!(p.contains("\"cardId\":\"3925\""), "card as a real field: {p}");
        assert!(p.contains("\"name\":\"testresult-3925-1787264000000-0\""), "old name form survives: {p}");
    }

    #[test]
    fn nightly_run_omits_card_id_typed_absence_never_empty_string() {
        // NEGATIVE PROOF (#3734): the failure mode is cardId:"" polluting the
        // graph — absence must be OMISSION.
        let p = test_result_payload("a/b.test.ts", "c", "pass", "t-x", "", "system", "t", 1787264000000, 1);
        assert!(!p.contains("cardId"), "no cardId key at all: {p}");
        assert!(p.contains("runTs"), "runTs still present on nightly rows: {p}");
    }

    #[test]
    fn boston_clock_handles_both_offsets() {
        // 2026-01-15 12:00 UTC → EST (-05:00) 07:00
        assert_eq!(boston_offset_iso(1768478400000), "2026-01-15T07:00:00-05:00");
        // 2026-07-15 12:00 UTC → EDT (-04:00) 08:00
        assert_eq!(boston_offset_iso(1784116800000), "2026-07-15T08:00:00-04:00");
    }
}

/// #3919 — the registered files whose tests DECLARE needs-stack hermeticity.
/// Deduped + sorted so lane exclusions are deterministic.
pub fn needs_stack_files(rows: &[TestRow]) -> std::collections::BTreeSet<String> {
    rows.iter()
        .filter(|r| r.hermeticity == "needs-stack")
        .map(|r| r.file_path.clone())
        .collect()
}

/// #3919 — the live-stack verdict from named probes. Down services are NAMED
/// in the error; the caller renders the typed SKIPPED state from it.
pub fn stack_verdict(probes: &[(&str, bool)]) -> Result<(), String> {
    let down: Vec<&str> = probes.iter().filter(|(_, ok)| !ok).map(|(n, _)| *n).collect();
    if down.is_empty() { Ok(()) } else { Err(down.join(", ")) }
}

/// #3919 — the ONE line the summary prints for the integration tier. A skip is
/// a typed, counted state ("SKIPPED (stack-down: …)"), never silence and never
/// a pass; zero registered needs-stack tests is its own explicit state (#3443).
pub fn integration_report(count: usize, stack_down: Option<&str>) -> String {
    match (count, stack_down) {
        (0, _) => "integration: none registered needs-stack for this diff".to_string(),
        (n, Some(down)) => format!(
            "integration: SKIPPED (stack-down: {}) — {} registered needs-stack test(s) NOT run; typed skip, counted, not a verdict", down, n),
        (n, None) => format!("integration: {} needs-stack test(s) ran with the live stack", n),
    }
}

/// #3919 — exclude integration-test binaries from a nextest run: tests/foo.rs
/// compiles to binary `foo`, so file-level exclusion is binary-level exact.
pub fn nextest_exclude_binaries(bins: &[&str]) -> Vec<String> {
    if bins.is_empty() {
        return Vec::new();
    }
    let expr = bins.iter().map(|b| format!("not binary(={})", b))
        .collect::<Vec<_>>().join(" and ");
    vec!["-E".to_string(), expr]
}

/// #3919 — jest exclusion args for the needs-stack files inside one package;
/// paths are made package-relative so jest's pattern matches its rootDir.
pub fn jest_ignore_args(needs_stack: &[&str], pkg: &str) -> Vec<String> {
    let prefix = format!("{}/", pkg);
    let rel: Vec<String> = needs_stack.iter()
        .filter_map(|f| f.strip_prefix(&prefix).map(|s| s.to_string()))
        .collect();
    if rel.is_empty() {
        return Vec::new();
    }
    vec!["--testPathIgnorePatterns".to_string(), rel.join("|")]
}

/// #3953 — parse a phase's budget (seconds) from werk-phase-budgets.tsv
/// (#3921's measured table). None = phase not in the table; the caller treats
/// that as no-budget rather than inventing one.
pub fn phase_budget(tsv: &str, phase: &str) -> Option<f64> {
    tsv.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            match (it.next(), it.next()) {
                (Some(p), Some(b)) if p.trim() == phase => b.trim().parse::<f64>().ok(),
                _ => None,
            }
        })
        .next()
}

/// #3953 — the IN-RUN enforcement target (col3, only where declared). The
/// p95 column is the nightly drift ceiling; this is the gate werk-test holds
/// the phase to while running.
pub fn phase_target(tsv: &str, phase: &str) -> Option<f64> {
    tsv.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            match (it.next(), it.next(), it.next()) {
                (Some(p), Some(_), Some(t)) if p.trim() == phase => {
                    t.split('#').next().unwrap_or("").trim().parse::<f64>().ok()
                }
                _ => None,
            }
        })
        .next()
}

/// #3953/#3955 — the budget verdict. Jeff's ruling (2026-08-21, after run 66
/// blocked a 1642-green suite at 701s): over-budget WARNS LOUDLY — spine event,
/// cost table, named number — but NEVER blocks a green run. "Let it run 10
/// minutes then fail" is the worst option of all; the fix for slow is faster
/// tests, not a wasted run. Some(text) = over-budget warning to shout; None = within.
pub fn budget_verdict(elapsed_s: f64, budget_s: f64) -> Option<String> {
    if elapsed_s <= budget_s {
        return None;
    }
    Some(format!(
        "!! budget-over: test phase took {:.1}s against the {:.0}s target — LOUD, not blocking          (Jeff 2026-08-21: a green run is never failed for being slow); the fix is faster selection",
        elapsed_s, budget_s))
}

/// #3953 — per-unit cost table, largest first: a red phase must point at the
/// unit that blew it, not shrug at the total.
pub fn unit_cost_report(costs: &[(String, f64)]) -> String {
    let mut sorted: Vec<&(String, f64)> = costs.iter().collect();
    sorted.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut out = String::from("per-unit cost (largest first):\n");
    for (name, secs) in sorted {
        out.push_str(&format!("   {:7.1}s  {}\n", secs, name));
    }
    out
}

/// #3920 — the registered browser-lane files (testConcern=ui), deduped+sorted.
pub fn ui_files(rows: &[TestRow]) -> std::collections::BTreeSet<String> {
    rows.iter()
        .filter(|r| r.test_concern == "ui")
        .map(|r| r.file_path.clone())
        .collect()
}

/// #3920 — surfaces whose diffs pay the browser lane: Clearing, the served
/// chorus pages, and the specs themselves. Anything else skips it — a browser
/// run on a Rust-only diff is pure cost.
pub fn ui_lane_fires(changed: &[String]) -> bool {
    const UI_SURFACES: [&str; 4] = [
        "directing/clearing/",
        "platform/api/public/",
        "platform/pages/",
        "proving/flows/",
    ];
    changed.iter().any(|f| UI_SURFACES.iter().any(|p| f.starts_with(p)))
}

/// #3920 — one workspace-level ui check, only when the lane fired AND ui tests
/// are registered. Zero registered = None (explicit absence, never vacuous).
pub fn ui_plan(fired: bool, registered: usize) -> Option<CheckKind> {
    if fired && registered > 0 { Some(CheckKind::UiFlows) } else { None }
}

/// #3920 — playwright's terminal summary: "N passed" / "M failed" lines.
/// None = no recognizable summary (crash before running — caller fails loud).
pub fn parse_playwright_summary(out: &str) -> Option<(usize, usize)> {
    let mut passed: Option<usize> = None;
    let mut failed: usize = 0;
    for l in out.lines() {
        let t = l.trim();
        if let Some(rest) = t.split_whitespace().next() {
            if let Ok(n) = rest.parse::<usize>() {
                let words: Vec<&str> = t.split_whitespace().collect();
                if words.len() >= 2 {
                    if words[1].starts_with("passed") { passed = Some(n); }
                    if words[1].starts_with("failed") { failed = n; }
                }
            }
        }
    }
    passed.map(|p| (p, failed))
}

/// #3920 fold — the nightly's cargo lane plan: every Rust crate holding a
/// REGISTERED test. This IS the "full selection" — the registry is the one
/// selection engine; there is no glob fallback (a fallback would be the second
/// walker this fold retires).
pub fn nightly_cargo_crates(rows: &[TestRow]) -> Vec<String> {
    plan_units_from_rows(rows)
        .into_iter()
        .filter_map(|u| match u {
            TestUnit::RustCrate(c) => Some(c),
            _ => None,
        })
        .collect()
}

/// #3920 fold — one selection engine means the nightly REFUSES without the
/// model. Degrading to discovery globs would silently reintroduce the second
/// walker; a refused nightly is a red the morning read can see.
pub fn nightly_requires_model(plan_source: &str) -> bool {
    plan_source != "model"
}

/// #3920 fold — the machine line nightly-suites.sh folds into its SUITE report.
/// Verdict vocabulary is the gate's own (pass/fail from gate-counted cases;
/// stack-down is the #3919 typed skip), so the nightly summary and the werk
/// gate can never disagree on what red means.
pub fn nightly_unit_line(crate_name: &str, ok: bool, passed: usize, failed: usize, ns_skipped: usize) -> String {
    nightly_lane_line("cargo", crate_name, ok, passed, failed, ns_skipped)
}

/// #3974 — the generalized fold line: every lane (cargo/npm/bats) speaks the
/// same machine vocabulary to nightly-suites.sh. One verdict language, every
/// tier; the wrapper only translates, never judges.
pub fn nightly_lane_line(kind: &str, unit: &str, ok: bool, passed: usize, failed: usize, ns_skipped: usize) -> String {
    let verdict = if ok { "pass" } else { "fail" };
    let skip = if ns_skipped > 0 {
        format!(", {} SKIPPED (stack-down, typed #3919)", ns_skipped)
    } else {
        String::new()
    };
    format!(
        "nightly-unit|{}|{}|{}|{} pass, {} fail{}",
        kind, unit, verdict, passed, failed, skip
    )
}

/// #3974 — full-selection TS packages: every registered package holding tests.
pub fn nightly_ts_packages(rows: &[TestRow]) -> Vec<String> {
    plan_units_from_rows(rows)
        .into_iter()
        .filter_map(|u| match u {
            TestUnit::TsPackage(p) => Some(p),
            _ => None,
        })
        .collect()
}

/// #3974 — full-selection bats suites from the registry.
pub fn nightly_bats_suites(rows: &[TestRow]) -> Vec<String> {
    plan_units_from_rows(rows)
        .into_iter()
        .filter_map(|u| match u {
            TestUnit::BatsSuite(s) => Some(s),
            _ => None,
        })
        .collect()
}

/// #3974 — pass/fail counts from a shell suite's summary line
/// ("=== Results: N passed, M failed ===" — the consumer-parseable form every
/// test-*.sh emits). Shell scripts carry no per-case identities, so counts are
/// the honest grain; None = no parseable summary (caller fails loud on rc).
pub fn parse_shell_counts(out: &str) -> Option<(usize, usize)> {
    for l in out.lines().rev() {
        if let Some(i) = l.find("Results:") {
            let rest = &l[i + 8..];
            let nums: Vec<usize> = rest
                .split(|c: char| !c.is_ascii_digit())
                .filter(|t| !t.is_empty())
                .filter_map(|t| t.parse().ok())
                .collect();
            if nums.len() >= 2 {
                return Some((nums[0], nums[1]));
            }
        }
    }
    None
}

/// #3974 — per-case results from bats TAP output ("ok N desc" / "not ok N desc"),
/// so the bats lane stops being boolean-only and its cases post like cargo/jest.
pub fn parse_bats_cases(out: &str) -> Vec<(String, String)> {
    let mut cases = Vec::new();
    for l in out.lines() {
        let l = l.trim();
        if let Some(rest) = l.strip_prefix("not ok ") {
            if let Some((_, name)) = rest.split_once(' ') {
                cases.push((name.trim().trim_start_matches("- ").trim().to_string(), "fail".to_string()));
            }
        } else if let Some(rest) = l.strip_prefix("ok ") {
            if let Some((num, name)) = rest.split_once(' ') {
                if num.chars().all(|c| c.is_ascii_digit()) {
                    // bats: "ok 1 desc" · node:test TAP: "ok 1 - desc"
                    let name = name.trim().trim_start_matches("- ").trim_start_matches("# skip").trim();
                    cases.push((name.to_string(), "pass".to_string()));
                }
            }
        }
    }
    cases
}

#[cfg(test)]
mod nightly_via_runner_3920 {
    use super::*;

    fn row(path: &str) -> TestRow {
        TestRow {
            file_path: path.to_string(),
            covers: String::new(),
            pyramid_layer: String::new(),
            hermeticity: String::new(),
            test_concern: String::new(),
        }
    }

    #[test]
    fn full_selection_is_every_registered_crate_deduped() {
        let rows = vec![
            row("platform/services/werk-test/src/lib.rs"),
            row("platform/services/chorus-hooks/tests/guard.rs"),
            row("platform/services/werk-test/tests/cli.rs"),
            row("platform/api/tests/x.test.ts"), // TS — not the cargo lane
        ];
        assert_eq!(nightly_cargo_crates(&rows), vec!["chorus-hooks", "werk-test"]);
    }

    #[test]
    fn nightly_refuses_without_the_model_no_glob_fallback() {
        assert!(nightly_requires_model("fallback"));
        assert!(!nightly_requires_model("model"));
    }

    #[test]
    fn red_under_werk_test_is_red_in_the_nightly_line() {
        // NEGATIVE PROOF (#3734): the guarded condition — a suite red under the
        // runner — must be VISIBLY red in the line the nightly folds. A pass
        // line and a fail line must be distinguishable by the |fail| token the
        // nightly's SUITE parser keys on.
        let red = nightly_unit_line("chorus-hooks", false, 40, 2, 0);
        let green = nightly_unit_line("chorus-hooks", true, 42, 0, 0);
        assert!(red.contains("|fail|"), "red line carries the fail verdict: {red}");
        assert!(!green.contains("|fail|"), "green line must not: {green}");
        assert_eq!(red, "nightly-unit|cargo|chorus-hooks|fail|40 pass, 2 fail");
    }

    #[test]
    fn stack_down_is_the_typed_skip_never_a_verdict_change() {
        let line = nightly_unit_line("chorus-api-client", true, 10, 0, 3);
        assert!(line.contains("3 SKIPPED (stack-down, typed #3919)"), "{line}");
        assert!(line.contains("|pass|"), "typed skip is not a fail: {line}");
    }
}

#[cfg(test)]
mod nightly_all_lanes_3974 {
    use super::*;

    fn row(path: &str) -> TestRow {
        TestRow { file_path: path.to_string(), covers: String::new(),
            pyramid_layer: String::new(), hermeticity: String::new(), test_concern: String::new() }
    }

    #[test]
    fn ts_and_bats_full_selection_from_the_registry() {
        let rows = vec![
            row("platform/api/tests/a.test.ts"),
            row("platform/mcp-server/tests/b.test.ts"),
            row("platform/tests/guard.bats"),
            row("platform/services/werk-test/tests/c.rs"),
        ];
        assert_eq!(nightly_ts_packages(&rows), vec!["platform/api", "platform/mcp-server"]);
        let clearing = vec![row("directing/clearing/tests/ui.test.ts")];
        assert_eq!(nightly_ts_packages(&clearing), vec!["directing/clearing"]);
        assert_eq!(nightly_bats_suites(&rows), vec!["platform/tests/guard.bats"]);
    }

    #[test]
    fn every_lane_speaks_one_fold_vocabulary() {
        // NEGATIVE PROOF (#3734): red and green are distinguishable by the
        // |fail| token in EVERY lane — no per-tier verdict dialects.
        for kind in ["cargo", "npm", "bats"] {
            let red = nightly_lane_line(kind, "u", false, 1, 2, 0);
            let green = nightly_lane_line(kind, "u", true, 3, 0, 0);
            assert!(red.contains(&format!("nightly-unit|{}|u|fail|", kind)), "{red}");
            assert!(!green.contains("|fail|"), "{green}");
        }
    }

    #[test]
    fn bats_tap_becomes_per_case_results() {
        let tap = "1..3\nok 1 guard fires on violation\nnot ok 2 guard passes clean repo\nok 3 fixture assembled\nrandom noise\n";
        let cases = parse_bats_cases(tap);
        assert_eq!(cases.len(), 3);
        assert_eq!(cases[1], ("guard passes clean repo".to_string(), "fail".to_string()));
        assert_eq!(cases[0].1, "pass");
    }
}

#[cfg(test)]
mod shell_fold_3974 {
    use super::*;
    #[test]
    fn shell_suites_are_units_and_summaries_parse() {
        assert_eq!(unit_of_path("platform/scripts/test-alert-checks.sh"),
            Some(TestUnit::BatsSuite("platform/scripts/test-alert-checks.sh".to_string())));
        assert_eq!(unit_of_path("platform/scripts/not-a-test.sh"), None);
        assert_eq!(parse_shell_counts("noise\n=== Results: 21 passed, 0 failed ===\n"), Some((21, 0)));
        assert_eq!(parse_shell_counts("=== Results: 5 passed, 2 failed ==="), Some((5, 2)));
        // NEGATIVE PROOF (#3734): no summary = None, never invented counts.
        assert_eq!(parse_shell_counts("script exploded"), None);
    }
}

/// #3922 — the security lane's selection: rows declared testConcern=security,
/// mapped to their units. Runs on its OWN cadence (not the land path); an
/// empty selection is an explicit absence line, never a vacuous green.
pub fn security_rows(rows: &[TestRow]) -> Vec<TestRow> {
    rows.iter().filter(|r| r.test_concern == "security").cloned().collect()
}

/// #3922 — the UNITS whose registered tests are security-declared. A unit in
/// this set folds under the `security` lane label so the nightly report and
/// the owner routing see one security lane, not reds scattered across tiers.
pub fn security_units(rows: &[TestRow]) -> std::collections::BTreeSet<String> {
    security_rows(rows)
        .iter()
        .filter_map(|r| unit_of_path(&r.file_path))
        .map(|u| match u {
            TestUnit::RustCrate(c) => c,
            TestUnit::TsPackage(p) => p,
            TestUnit::BatsSuite(s) => s,
        })
        .collect()
}

#[cfg(test)]
mod security_lane_3922 {
    use super::*;
    fn row(path: &str, concern: &str) -> TestRow {
        TestRow { file_path: path.to_string(), covers: String::new(),
            pyramid_layer: String::new(), hermeticity: String::new(),
            test_concern: concern.to_string() }
    }
    #[test]
    fn selects_only_declared_security_rows() {
        let rows = vec![
            row("platform/scripts/test-security-manifest-3726.sh", "security"),
            row("platform/api/tests/a.test.ts", "ui"),
            row("platform/tests/x.bats", ""),
        ];
        let sel = security_rows(&rows);
        assert_eq!(sel.len(), 1);
        assert!(sel[0].file_path.contains("security-manifest"));
        // NEGATIVE PROOF (#3734): an undeclared corpus selects NOTHING —
        // the lane can tell "no security tests declared" from "all green".
        assert!(security_rows(&[row("a.rs", ""), row("b.rs", "perf")]).is_empty());
    }

    #[test]
    fn security_units_relabel_whole_suites() {
        let rows = vec![
            row("platform/scripts/test-security-manifest-3726.sh", "security"),
            row("platform/tests/authz-3977.bats", "security"),
            row("platform/tests/other.bats", ""),
        ];
        let u = security_units(&rows);
        assert!(u.contains("platform/scripts/test-security-manifest-3726.sh"));
        assert!(u.contains("platform/tests/authz-3977.bats"));
        assert!(!u.contains("platform/tests/other.bats"));
    }
}

/// #4004 — does this test case's FILE live inside this package? The nightly's
/// per-package row used to count whatever the runner produced while running in
/// that package's directory, and jest's rootDir can reach past the package dir.
/// Kade read "cards 7 fail / clearing 1 fail" and found the failing cases were
/// platform/api/tests/*.integration.test.ts — another package's results wearing
/// this package's name. Pure so the attribution rule is testable without jest.
pub fn package_owns_case(pkg: &str, file_path: &str) -> bool {
    file_path.starts_with(&format!("{}/", pkg))
}

#[cfg(test)]
mod package_attribution_tests {
    use super::package_owns_case;

    #[test]
    fn a_package_owns_the_cases_under_its_own_path() {
        assert!(package_owns_case("platform/api", "platform/api/tests/streams.test.ts"));
        assert!(package_owns_case("platform/pulse", "platform/pulse/src/store.test.ts"));
    }

    /// NEGATIVE PROOF (#3734): the exact misattribution Kade found must be
    /// refused — an api integration case must never count as a cards case.
    #[test]
    fn a_foreign_case_is_not_owned_no_matter_who_ran_it() {
        assert!(!package_owns_case(
            "directing/products/cards",
            "platform/api/tests/releases.integration.test.ts"
        ));
        assert!(!package_owns_case("directing/clearing", "platform/api/tests/x.test.ts"));
    }

    /// A prefix that is not a path boundary is not ownership — otherwise
    /// platform/api would swallow platform/api-extras and the row would be
    /// wrong in the other direction.
    #[test]
    fn ownership_respects_path_boundaries() {
        assert!(!package_owns_case("platform/api", "platform/api-extras/tests/x.test.ts"));
        assert!(!package_owns_case("platform/api", "platform/api"));
    }
}
