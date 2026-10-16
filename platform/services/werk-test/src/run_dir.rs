// @domain: tests
//! #4540 — the run folder: what one werk-test step hands the next.
//!
//! A card's test is dagu steps (scope, prepare, one run per test type,
//! verdict, report), each its own process, so the state that used to live in
//! one function's locals is written here as plain tab-separated lines. std
//! only. The folder is keyed on the run's trace, never the card: two runs of
//! one card never share a folder. A step's output file is rewritten whole, so
//! a retried step replaces what it wrote before (Silas 10-16: one result file
//! per test type, never appended — a retry must not count twice).

use crate::{CaseResult, CheckKind, JestPlan, JestSelection, Quarantined, StackState, TestUnit};
use std::path::{Path, PathBuf};

/// The step names `werk-test <step> <card> <role>` takes, in run order.
pub const STEPS: [&str; 5] = ["scope", "prepare", "run", "report", "verdict"];

/// Jeff's run order for test types (2026-09-12 note, #4540 AC7).
pub const TEST_TYPES: [&str; 7] = ["coverage", "lint", "security", "unit", "integration", "ui", "perf"];

/// Jeff 10-16 (option B): one queue runs every type but perf, in his order;
/// perf runs after it as its own step, so its timing sees a quiet box.
pub const QUEUE_TYPES: [&str; 6] = ["coverage", "lint", "security", "unit", "integration", "ui"];

/// The test type a check proves. A lint ratchet, tsc and the doc floor are
/// lint; a crate, package or suite run is unit (its integration cases run in
/// the same binary today); the browser lane is ui.
pub fn check_type(kind: CheckKind) -> &'static str {
    match kind {
        CheckKind::Tsc | CheckKind::ClippyRatchet | CheckKind::LintRatchet | CheckKind::DocCoherence => "lint",
        CheckKind::CargoTest | CheckKind::Jest | CheckKind::Bats => "unit",
        CheckKind::UiFlows => "ui",
    }
}

/// The folder for a run: `<base>/<trace>`. An empty trace is refused — a
/// folder named for nothing would be shared by every run without one.
pub fn folder_for(base: &Path, trace: &str) -> Result<PathBuf, String> {
    let t = trace.trim();
    if t.is_empty() || t.contains('/') || t.starts_with('.') {
        return Err(format!("run folder needs a trace id, got {:?}", trace));
    }
    Ok(base.join(t))
}

pub struct RunDir {
    pub path: PathBuf,
}

impl RunDir {
    pub fn create(path: PathBuf) -> Result<RunDir, String> {
        std::fs::create_dir_all(&path).map_err(|e| format!("run folder {}: {}", path.display(), e))?;
        Ok(RunDir { path })
    }

    /// Write `name` whole (a temp name, then rename).
    pub fn put(&self, name: &str, lines: &[String]) -> Result<(), String> {
        let tmp = self.path.join(format!(".{}.tmp", name));
        let mut body = lines.join("\n");
        if !body.is_empty() {
            body.push('\n');
        }
        std::fs::write(&tmp, body).map_err(|e| format!("write {}: {}", name, e))?;
        std::fs::rename(&tmp, self.path.join(name)).map_err(|e| format!("write {}: {}", name, e))
    }

    /// The lines of `name`; `None` when no step wrote it.
    pub fn get(&self, name: &str) -> Option<Vec<String>> {
        std::fs::read_to_string(self.path.join(name))
            .ok()
            .map(|t| t.lines().map(|l| l.to_string()).collect())
    }

    /// `name`, or a refusal naming the step that writes it.
    pub fn need(&self, name: &str, by: &str) -> Result<Vec<String>, String> {
        self.get(name)
            .ok_or_else(|| format!("{} is missing from {}: run `werk-test {}` first", name, self.path.display(), by))
    }
}

/// #4540 (Silas 10-16) — one run, one binary. The 05:11 land swapped
/// werk-test-bin under a running nightly; under dagu every step starts the
/// binary fresh, so a run could mix two builds. Scope copies the binary it is
/// into the run folder; every later step runs that copy.
pub const PINNED_BIN: &str = "werk-test-bin";
pub const PINNED_ENV: &str = "WERK_TEST_PINNED";

pub fn pin_binary(dir: &RunDir, exe: &Path) -> Result<(), String> {
    let dst = dir.path.join(PINNED_BIN);
    let tmp = dir.path.join(format!(".{}.tmp", PINNED_BIN));
    std::fs::copy(exe, &tmp).map_err(|e| format!("pin {}: {}", exe.display(), e))?;
    std::fs::rename(&tmp, &dst).map_err(|e| format!("pin {}: {}", dst.display(), e))
}

/// The pinned copy a later step must run instead of itself, if any: the run
/// folder holds one, this process is not it, and it has not already handed off.
pub fn pinned_to_exec(dir: &RunDir, current: &Path, already: bool) -> Option<PathBuf> {
    let pinned = dir.path.join(PINNED_BIN);
    (!already && pinned.is_file() && current != pinned).then_some(pinned)
}

/// One field of a line: tabs and newlines become spaces.
pub fn field(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

pub fn units_lines(units: &[TestUnit]) -> Vec<String> {
    units
        .iter()
        .map(|u| match u {
            TestUnit::RustCrate(n) => format!("crate\t{}", field(n)),
            TestUnit::TsPackage(n) => format!("ts\t{}", field(n)),
            TestUnit::BatsSuite(n) => format!("bats\t{}", field(n)),
        })
        .collect()
}

pub fn units_of(lines: &[String]) -> Result<Vec<TestUnit>, String> {
    lines
        .iter()
        .map(|l| match l.split_once('\t') {
            Some(("crate", n)) => Ok(TestUnit::RustCrate(n.to_string())),
            Some(("ts", n)) => Ok(TestUnit::TsPackage(n.to_string())),
            Some(("bats", n)) => Ok(TestUnit::BatsSuite(n.to_string())),
            _ => Err(format!("units: bad line {:?}", l)),
        })
        .collect()
}

/// `selected` then `pkg\tfile` per file (a package with no files is `pkg\t`),
/// or `full\t<reason>`.
pub fn jest_lines(plan: &JestPlan) -> Vec<String> {
    match plan {
        JestPlan::FullFallback { reason } => vec![format!("full\t{}", field(reason))],
        JestPlan::Selected(sels) => {
            let mut v = vec!["selected".to_string()];
            for s in sels {
                if s.test_files.is_empty() {
                    v.push(format!("{}\t", field(&s.package)));
                }
                v.extend(s.test_files.iter().map(|f| format!("{}\t{}", field(&s.package), field(f))));
            }
            v
        }
    }
}

pub fn jest_of(lines: &[String]) -> Result<JestPlan, String> {
    match lines.first().map(String::as_str) {
        Some(l) if l.starts_with("full\t") => Ok(JestPlan::FullFallback { reason: l["full\t".len()..].to_string() }),
        Some("selected") => {
            let mut sels: Vec<JestSelection> = Vec::new();
            for l in &lines[1..] {
                let (p, f) = l.split_once('\t').ok_or_else(|| format!("jest: bad line {:?}", l))?;
                let i = match sels.iter().position(|s| s.package == p) {
                    Some(i) => i,
                    None => {
                        sels.push(JestSelection { package: p.to_string(), test_files: Vec::new() });
                        sels.len() - 1
                    }
                };
                if !f.is_empty() {
                    sels[i].test_files.push(f.to_string());
                }
            }
            Ok(JestPlan::Selected(sels))
        }
        _ => Err("jest: no plan line".to_string()),
    }
}

pub fn stack_lines(s: &StackState) -> Vec<String> {
    vec![match s {
        StackState::Up => "up".to_string(),
        StackState::Down(w) => format!("down\t{}", field(w)),
        StackState::Unmeasurable(w) => format!("unmeasurable\t{}", field(w)),
    }]
}

pub fn stack_of(lines: &[String]) -> Result<StackState, String> {
    let l = lines.first().map(String::as_str).unwrap_or("");
    match l.split_once('\t') {
        Some(("down", w)) => Ok(StackState::Down(w.to_string())),
        Some(("unmeasurable", w)) => Ok(StackState::Unmeasurable(w.to_string())),
        _ if l == "up" => Ok(StackState::Up),
        _ => Err(format!("stack: bad line {:?}", l)),
    }
}

/// `a\tb` per line.
pub fn pair_lines<'a>(pairs: impl IntoIterator<Item = (&'a String, &'a String)>) -> Vec<String> {
    pairs.into_iter().map(|(a, b)| format!("{}\t{}", field(a), field(b))).collect()
}

pub fn pairs_of(lines: &[String]) -> Vec<(String, String)> {
    lines.iter().filter_map(|l| l.split_once('\t')).map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

pub fn holds_lines(q: &[Quarantined]) -> Vec<String> {
    q.iter()
        .map(|h| format!("{}\t{}\t{}\t{}", field(&h.case), field(&h.reason), field(&h.until), field(&h.file)))
        .collect()
}

pub fn holds_of(lines: &[String]) -> Vec<Quarantined> {
    crate::parse_quarantine_rows(&lines.join("\n"))
}

/// What one `run --type <t>` step measured, written whole as `ran-<type>`.
#[derive(Debug, Default, PartialEq)]
pub struct TypeResult {
    pub cases: Vec<CaseResult>,
    pub unmeasured: Vec<String>,
    pub failed: Vec<String>,
    pub unit_secs: Vec<(String, f64)>,
    pub unmatched_cargo: usize,
}

impl TypeResult {
    pub fn lines(&self) -> Vec<String> {
        let mut v = vec![format!("unmatched\t{}", self.unmatched_cargo)];
        v.extend(self.cases.iter().map(|c| {
            format!("case\t{}\t{}\t{}", field(&c.file_path), field(&c.test_name), field(&c.result))
        }));
        v.extend(self.unmeasured.iter().map(|u| format!("unmeasured\t{}", field(u))));
        v.extend(self.failed.iter().map(|f| format!("failed\t{}", field(f))));
        v.extend(self.unit_secs.iter().map(|(u, s)| format!("secs\t{}\t{:.3}", field(u), s)));
        v
    }

    pub fn of(lines: &[String]) -> TypeResult {
        let mut r = TypeResult::default();
        for l in lines {
            let Some((k, rest)) = l.split_once('\t') else { continue };
            match k {
                "unmatched" => r.unmatched_cargo = rest.parse().unwrap_or(0),
                "case" => {
                    let mut it = rest.splitn(3, '\t');
                    if let (Some(f), Some(n), Some(res)) = (it.next(), it.next(), it.next()) {
                        r.cases.push(CaseResult { file_path: f.into(), test_name: n.into(), result: res.into() });
                    }
                }
                "unmeasured" => r.unmeasured.push(rest.to_string()),
                "failed" => r.failed.push(rest.to_string()),
                "secs" => {
                    if let Some((u, s)) = rest.rsplit_once('\t') {
                        r.unit_secs.push((u.to_string(), s.parse().unwrap_or(0.0)));
                    }
                }
                _ => {}
            }
        }
        r
    }

    /// Every type's result, in Jeff's type order, as one.
    pub fn all(dir: &RunDir) -> TypeResult {
        let mut all = TypeResult::default();
        for t in TEST_TYPES {
            if let Some(lines) = dir.get(&format!("ran-{}", t)) {
                let r = TypeResult::of(&lines);
                all.cases.extend(r.cases);
                all.unmeasured.extend(r.unmeasured);
                all.failed.extend(r.failed);
                all.unit_secs.extend(r.unit_secs);
                all.unmatched_cargo += r.unmatched_cargo;
            }
        }
        all
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> RunDir {
        let p = std::env::temp_dir().join(format!("run-dir-4540-{}-{}", tag, std::process::id()));
        let _ = std::fs::remove_dir_all(&p);
        RunDir::create(p).unwrap()
    }

    #[test]
    fn every_step_file_reads_back_as_written() {
        let units = vec![TestUnit::RustCrate("werk-test".into()), TestUnit::TsPackage("platform/api".into()),
                         TestUnit::BatsSuite("platform/tests/a b.bats".into())];
        assert_eq!(units_of(&units_lines(&units)).unwrap(), units);
        let plan = JestPlan::Selected(vec![
            JestSelection { package: "platform/api".into(), test_files: vec!["platform/api/t/a.test.ts".into()] },
            JestSelection { package: "directing/clearing".into(), test_files: vec![] }]);
        assert_eq!(jest_of(&jest_lines(&plan)).unwrap(), plan);
        let full = JestPlan::FullFallback { reason: "registry down".into() };
        assert_eq!(jest_of(&jest_lines(&full)).unwrap(), full);
        for s in [StackState::Up, StackState::Down("fuseki refused".into()), StackState::Unmeasurable("slow".into())] {
            assert_eq!(stack_of(&stack_lines(&s)).unwrap(), s);
        }
        let q = vec![Quarantined { case: "c".into(), reason: "flaky".into(), until: "2026-11-01".into(), file: "x.bats".into() }];
        assert_eq!(holds_of(&holds_lines(&q)), q);
        let r = TypeResult {
            cases: vec![CaseResult { file_path: "a.bats".into(), test_name: "one\ttwo".into(), result: "pass".into() }],
            unmeasured: vec!["x — ran nothing".into()], failed: vec!["cargo-test:fx".into()],
            unit_secs: vec![("bats:a".into(), 1.5)], unmatched_cargo: 2,
        };
        let back = TypeResult::of(&r.lines());
        assert_eq!(back.cases[0].test_name, "one two", "a tab in a name cannot split the line");
        assert_eq!((back.unmeasured, back.failed, back.unit_secs, back.unmatched_cargo), (r.unmeasured, r.failed, r.unit_secs, 2));
    }

    /// A retried type rewrites its file: the cases are counted once.
    #[test]
    fn a_retried_type_step_counts_its_cases_once() {
        let d = tmp("retry");
        let r = TypeResult { cases: vec![CaseResult { file_path: "a".into(), test_name: "t".into(), result: "fail".into() }], ..Default::default() };
        d.put("ran-unit", &r.lines()).unwrap();
        d.put("ran-unit", &r.lines()).unwrap();
        d.put("ran-lint", &TypeResult::default().lines()).unwrap();
        assert_eq!(TypeResult::all(&d).cases.len(), 1);
        let _ = std::fs::remove_dir_all(&d.path);
    }

    /// NEGATIVE PROOF: a missing step file is a refusal that names the step,
    /// never an empty answer read as "nothing ran".
    #[test]
    fn negative_proof_a_step_that_never_ran_is_named() {
        let d = tmp("missing");
        let e = d.need("units", "scope").unwrap_err();
        assert!(e.contains("units is missing") && e.contains("werk-test scope"), "{e}");
        assert!(folder_for(Path::new("/x"), "").is_err(), "no trace, no folder");
        assert!(folder_for(Path::new("/x"), "../up").is_err());
        assert_eq!(folder_for(Path::new("/x"), "034gp").unwrap(), PathBuf::from("/x/034gp"));
        let _ = std::fs::remove_dir_all(&d.path);
    }

    /// One run, one binary: a later step runs the copy scope pinned.
    #[test]
    fn a_later_step_runs_the_binary_scope_pinned() {
        let d = tmp("pin");
        let exe = d.path.join("built");
        std::fs::write(&exe, "v1").unwrap();
        assert_eq!(pinned_to_exec(&d, &exe, false), None, "nothing pinned yet: run yourself");
        pin_binary(&d, &exe).unwrap();
        std::fs::write(&exe, "v2 — swapped by a land").unwrap();
        let p = pinned_to_exec(&d, &exe, false).expect("a pinned copy to run");
        assert_eq!(std::fs::read_to_string(&p).unwrap(), "v1");
        // NEGATIVE PROOF: the copy itself, or a step that already handed off, runs on
        assert_eq!(pinned_to_exec(&d, &p, false), None);
        assert_eq!(pinned_to_exec(&d, &exe, true), None);
        let _ = std::fs::remove_dir_all(&d.path);
    }

    #[test]
    fn every_check_has_a_type_in_jeffs_order() {
        for k in [CheckKind::CargoTest, CheckKind::Tsc, CheckKind::Jest, CheckKind::ClippyRatchet,
                  CheckKind::LintRatchet, CheckKind::DocCoherence, CheckKind::Bats, CheckKind::UiFlows] {
            assert!(TEST_TYPES.contains(&check_type(k)), "{:?}", k);
        }
    }
}
