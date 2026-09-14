//! chorus-crawl — the one herald (#4173).
//!
//! Walks the repo once and keeps a row per tracked file in the graph, through
//! the generated door. Ported from crawl-files.py (336 lines) + testfiles.py's
//! parsers, which already got the hard parts right: ask the server for its own
//! collection path, and SKIP a file the model has no kind for rather than
//! inventing one.
//!
//! This module is the pure decision core — classification and planning — with
//! no I/O, so every rule below is unit-testable without a repo or a server.

/// What a file is, from the model's served CodeKind individuals (#4157):
/// code · config · doc · log · test, plus `data` (#4173). Never a free string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Code,
    Config,
    Doc,
    Log,
    Test,
    Data,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Code => "code",
            Kind::Config => "config",
            Kind::Doc => "doc",
            Kind::Log => "log",
            Kind::Test => "test",
            Kind::Data => "data",
        }
    }
}

/// The classifier's answer. `Skip` is a first-class outcome, not an error and
/// not a default: a .jpg is not code. The walker counts skips and names them;
/// widening the set is a MODEL edit, never a walker edit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Classified(Kind, Option<&'static str>),
    Skip,
}

/// extension → language, mirroring code-vocab.ttl's named individuals (#4157).
const LANG: [(&str, &str); 13] = [
    (".rs", "rust"),
    (".ts", "typescript"),
    (".tsx", "typescript"),
    (".js", "javascript"),
    (".cjs", "javascript"),
    (".mjs", "javascript"),
    (".py", "python"),
    (".sh", "bash"),
    (".bash", "bash"),
    (".bats", "bash"),
    (".md", "markdown"),
    (".ttl", "turtle"),
    (".sql", "sql"),
];

/// extension → kind, for files whose extension decides it outright.
const KIND_BY_EXT: [(&str, Kind); 12] = [
    (".md", Kind::Doc),
    (".html", Kind::Doc),
    (".htm", Kind::Doc),
    (".json", Kind::Config),
    (".yml", Kind::Config),
    (".yaml", Kind::Config),
    (".toml", Kind::Config),
    (".ttl", Kind::Config),
    (".conf", Kind::Config),
    (".plist", Kind::Config),
    (".log", Kind::Log),
    (".csv", Kind::Data),
];

fn ext_of(path: &str) -> String {
    let base = path.rsplit('/').next().unwrap_or(path);
    match base.rfind('.') {
        Some(i) if i > 0 => base[i..].to_ascii_lowercase(),
        _ => String::new(),
    }
}

fn lang_of(ext: &str) -> Option<&'static str> {
    LANG.iter().find(|(e, _)| *e == ext).map(|(_, l)| *l)
}

/// Is this path a test, by the repo's own conventions? Ported verbatim from
/// testfiles.py's `is_test_file` so the two walkers cannot disagree while both
/// exist. Path-shaped only — the content check (a .rs carrying `#[test]`) is
/// the caller's, since it needs the file.
pub fn is_test_path(rel: &str) -> bool {
    let base = rel.rsplit('/').next().unwrap_or(rel);
    base.ends_with(".bats")
        || base.ends_with(".test.ts")
        || base.ends_with(".test.tsx")
        || base.ends_with(".test.js")
        || base.ends_with(".test.cjs")
        || base.ends_with(".spec.ts")
        || base.ends_with(".spec.js")
        || base.ends_with(".spec.cjs")
        || base.ends_with(".test.sh")
        || (base.starts_with("test_") && base.ends_with(".py"))
        || (base.starts_with("test-") && base.ends_with(".sh"))
}

/// The whole rule, as one pure function: same path in, same answer out, every
/// time. Order matters — test wins over extension, extension over language,
/// and anything the model has no name for is SKIPPED rather than guessed.
pub fn classify(rel: &str, has_rust_test_attr: bool) -> Verdict {
    let ext = ext_of(rel);
    let lang = lang_of(&ext);
    if is_test_path(rel) || (ext == ".rs" && has_rust_test_attr) {
        return Verdict::Classified(Kind::Test, lang);
    }
    if let Some((_, k)) = KIND_BY_EXT.iter().find(|(e, _)| *e == ext) {
        return Verdict::Classified(*k, lang);
    }
    if lang.is_some() {
        return Verdict::Classified(Kind::Code, lang);
    }
    Verdict::Skip
}

#[cfg(test)]
mod classify_4173 {
    use super::*;

    // AC: kind and language come from the model's named values, never free strings.
    #[test]
    fn every_kind_the_model_serves_is_reachable_and_named_exactly() {
        let cases = [
            ("platform/services/werk-test/src/main.rs", Kind::Code, Some("rust")),
            ("platform/api/src/server.ts", Kind::Code, Some("typescript")),
            ("platform/tests/4145-runner.bats", Kind::Test, Some("bash")),
            ("platform/api/tests/index-db.test.ts", Kind::Test, Some("typescript")),
            ("designing/docs/crawler.html", Kind::Doc, None),
            ("roles/kade/current-work.md", Kind::Doc, Some("markdown")),
            ("platform/api/package.json", Kind::Config, None),
            ("roles/silas/ontology/chorus.ttl", Kind::Config, Some("turtle")),
            ("platform/logs/chorus.log", Kind::Log, None),
            ("platform/data/drives.csv", Kind::Data, None),
        ];
        for (path, want_kind, want_lang) in cases {
            match classify(path, false) {
                Verdict::Classified(k, l) => {
                    assert_eq!(k, want_kind, "kind for {}", path);
                    assert_eq!(l, want_lang, "language for {}", path);
                }
                Verdict::Skip => panic!("{} should classify, not skip", path),
            }
        }
    }

    // NEGATIVE PROOF (#3734): a file the model has no kind for is SKIPPED and
    // counted — never written as "code" by default. Inventing a kind is the
    // defect this card removes, so the check must show the skip happening.
    #[test]
    fn negative_proof_a_file_the_model_cannot_name_is_skipped_not_called_code() {
        for path in ["roles/silas/ontology/chorus.ttl.png", "designing/diagrams/flow.svg", "platform/api/public/font.woff2", "LICENSE"] {
            assert_eq!(classify(path, false), Verdict::Skip, "{} must skip", path);
        }
    }

    // The two states the classifier exists to separate: a .rs that declares
    // tests is a test, one that does not is code. A check that cannot tell
    // them apart would mark the whole workspace one or the other.
    #[test]
    fn negative_proof_a_rust_file_is_test_or_code_by_its_content_not_its_folder() {
        assert_eq!(classify("platform/services/werk-test/src/lib.rs", true), Verdict::Classified(Kind::Test, Some("rust")));
        assert_eq!(classify("platform/services/werk-test/src/lib.rs", false), Verdict::Classified(Kind::Code, Some("rust")));
    }

    // AC: deterministic. Same path, same answer — no clock, no cwd, no order.
    #[test]
    fn deterministic_same_path_answers_the_same_twice() {
        let p = "platform/scripts/test-nightly.sh";
        assert_eq!(classify(p, false), classify(p, false));
        assert_eq!(classify(p, false), Verdict::Classified(Kind::Test, Some("bash")));
    }

    // A dotfile has no extension to key on and must not be mistaken for one:
    // ".gitignore" is not an extension of a file called "".
    #[test]
    fn negative_proof_a_dotfile_is_not_read_as_an_extension() {
        assert_eq!(classify(".gitignore", false), Verdict::Skip);
        assert_eq!(classify("platform/api/.eslintrc.json", false), Verdict::Classified(Kind::Config, None));
    }
}

// ─────────────────────────── the plan ───────────────────────────

/// What the run intends to do to one row, decided before anything is written.
/// Planning is pure so the whole decision is testable without a repo or a
/// server, and so a run can be explained after the fact from its counts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Not in the graph yet.
    Post { path: String },
    /// In the graph, content changed (sha differs).
    Replace { path: String },
    /// In the graph, content identical — the run must NOT rewrite it.
    Unchanged { path: String },
    /// In the graph, gone from the tree. Deleted one row at a time.
    Delete { path: String },
    /// On disk, but the model has no name for it. Counted and reported.
    Skipped { path: String },
}

/// A delete is only ever decided from a POSITIVE reading of the WHOLE tree.
///
/// Two different things can make that reading incomplete, and both must refuse:
/// a walk that failed part-way, and a walk that only ever looked at part of the
/// tree on purpose. The second one nearly cost the graph: a delta run sees only
/// the changed files, so every unchanged file is "absent from disk" and every
/// row for one looks like an orphan. The first delta run after a full one
/// planned 5,346 deletes — the entire collection minus the diff — and only the
/// door's authz refusal stopped it.
///
/// "I cannot see it" must never become "it is gone" (#4022), and neither must
/// "I did not look at it".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeRead {
    /// The full file list was read successfully; absence from it is real.
    Complete,
    /// The walk failed part-way. Deletes are refused for this run.
    Partial,
    /// The walk deliberately looked at a subset (a delta). Absence from this
    /// view says nothing at all — deletes come from git's own D/R entries.
    Scoped,
}

/// One file as the tree reports it: repo-relative path, content hash, and
/// whether the model could name it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnDisk {
    pub path: String,
    pub sha: String,
    pub classified: bool,
}

/// One row as the door reports it: path and the sha the graph believes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InGraph {
    pub path: String,
    pub sha: String,
}

/// The whole decision, as a pure function of (tree, graph, how well we read the
/// tree). No clock, no network, no filesystem.
pub fn plan(disk: &[OnDisk], graph: &[InGraph], read: TreeRead) -> Vec<Action> {
    let mut out = Vec::new();
    for f in disk {
        if !f.classified {
            out.push(Action::Skipped { path: f.path.clone() });
            continue;
        }
        match graph.iter().find(|g| g.path == f.path) {
            None => out.push(Action::Post { path: f.path.clone() }),
            Some(g) if g.sha != f.sha => out.push(Action::Replace { path: f.path.clone() }),
            Some(_) => out.push(Action::Unchanged { path: f.path.clone() }),
        }
    }
    // Orphans: a row whose path the tree no longer has. Only decidable when the
    // tree was read in full.
    if read == TreeRead::Complete {
        for g in graph {
            if !disk.iter().any(|f| f.path == g.path) {
                out.push(Action::Delete { path: g.path.clone() });
            }
        }
    }
    out
}

/// The counts a run reports. A run that cannot say what it did is the silence
/// this card exists to end (33 hydrator failures in 24h reached nobody).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Counts {
    pub posted: usize,
    pub replaced: usize,
    pub unchanged: usize,
    pub deleted: usize,
    pub skipped: usize,
}

pub fn counts(actions: &[Action]) -> Counts {
    let mut c = Counts::default();
    for a in actions {
        match a {
            Action::Post { .. } => c.posted += 1,
            Action::Replace { .. } => c.replaced += 1,
            Action::Unchanged { .. } => c.unchanged += 1,
            Action::Delete { .. } => c.deleted += 1,
            Action::Skipped { .. } => c.skipped += 1,
        }
    }
    c
}

/// Does this plan write anything? A second run over an unchanged tree must
/// answer false — that is idempotence, proven rather than asserted.
pub fn writes_anything(actions: &[Action]) -> bool {
    actions
        .iter()
        .any(|a| matches!(a, Action::Post { .. } | Action::Replace { .. } | Action::Delete { .. }))
}

#[cfg(test)]
mod plan_4173 {
    use super::*;

    fn d(path: &str, sha: &str) -> OnDisk {
        OnDisk { path: path.into(), sha: sha.into(), classified: true }
    }
    fn g(path: &str, sha: &str) -> InGraph {
        InGraph { path: path.into(), sha: sha.into() }
    }

    #[test]
    fn a_new_file_posts_a_changed_file_replaces_an_identical_file_is_left_alone() {
        let disk = [d("a.rs", "aaa"), d("b.rs", "NEW"), d("c.rs", "ccc")];
        let graph = [g("a.rs", "aaa"), g("b.rs", "old")];
        let actions = plan(&disk, &graph, TreeRead::Complete);
        assert!(actions.contains(&Action::Unchanged { path: "a.rs".into() }));
        assert!(actions.contains(&Action::Replace { path: "b.rs".into() }));
        assert!(actions.contains(&Action::Post { path: "c.rs".into() }));
    }

    // AC: idempotent — a second run writes NOTHING. Proven by running the plan
    // against its own result, not asserted in a comment.
    #[test]
    fn a_second_run_over_an_unchanged_tree_writes_nothing() {
        let disk = [d("a.rs", "aaa"), d("b.ts", "bbb")];
        let graph = [g("a.rs", "aaa"), g("b.ts", "bbb")];
        let actions = plan(&disk, &graph, TreeRead::Complete);
        assert!(!writes_anything(&actions), "a no-op walk must write nothing: {:?}", actions);
        assert_eq!(counts(&actions).unchanged, 2);
    }

    // NEGATIVE PROOF (#3734): the same check must still fire when there IS work,
    // or "writes nothing" would be true of a crawler that never writes at all.
    #[test]
    fn negative_proof_a_run_with_real_work_does_report_that_it_writes() {
        let disk = [d("a.rs", "CHANGED")];
        let graph = [g("a.rs", "aaa")];
        assert!(writes_anything(&plan(&disk, &graph, TreeRead::Complete)));
    }

    #[test]
    fn a_row_whose_file_left_the_tree_is_deleted_one_at_a_time() {
        let disk = [d("a.rs", "aaa")];
        let graph = [g("a.rs", "aaa"), g("gone.rs", "xxx")];
        let actions = plan(&disk, &graph, TreeRead::Complete);
        assert_eq!(counts(&actions).deleted, 1);
        assert!(actions.contains(&Action::Delete { path: "gone.rs".into() }));
    }

    // NEGATIVE PROOF (#3734): a DELTA run must not delete the rest of the graph.
    //
    // This is not hypothetical. On 2026-09-14 the first delta run after a full
    // one planned 5,346 deletes — the whole collection minus the diff — because
    // the disk view held only the changed files while the graph view held
    // everything. The door's authz refusal is the only thing that stopped it.
    // The two states the check must separate: "absent because it is gone" and
    // "absent because I did not look".
    #[test]
    fn negative_proof_a_scoped_delta_run_never_deletes_what_it_did_not_look_at() {
        let changed_only = [d("changed.rs", "new")];
        let whole_graph = [
            g("changed.rs", "old"),
            g("untouched-a.rs", "aaa"),
            g("untouched-b.ts", "bbb"),
        ];
        let scoped = plan(&changed_only, &whole_graph, TreeRead::Scoped);
        assert_eq!(counts(&scoped).deleted, 0, "a delta must delete nothing it did not walk: {:?}", scoped);
        assert_eq!(counts(&scoped).replaced, 1, "it still updates what DID change");

        // The control, with identical inputs: a run that claims to have read the
        // whole tree DOES treat those rows as orphans. Without this the check
        // could not tell the two states apart.
        let full = plan(&changed_only, &whole_graph, TreeRead::Complete);
        assert_eq!(counts(&full).deleted, 2, "a full walk still reconciles orphans");
    }

    // NEGATIVE PROOF (#3734): the #4022 lesson. When the tree could not be read
    // in full, "I cannot see it" must NOT become "it is gone" — and the check
    // must show the refusal, with the same inputs that would otherwise delete.
    #[test]
    fn negative_proof_a_partial_tree_read_refuses_to_delete_anything() {
        let disk = [d("a.rs", "aaa")];
        let graph = [g("a.rs", "aaa"), g("gone.rs", "xxx")];
        let complete = plan(&disk, &graph, TreeRead::Complete);
        let partial = plan(&disk, &graph, TreeRead::Partial);
        assert_eq!(counts(&complete).deleted, 1, "control: a complete read DOES delete");
        assert_eq!(counts(&partial).deleted, 0, "a partial read must delete nothing");
    }

    // An unclassifiable file is reported, never silently dropped and never
    // posted as "code" — the plan carries it so the run can name it.
    #[test]
    fn an_unnameable_file_is_counted_as_skipped_not_posted() {
        let disk = [OnDisk { path: "logo.png".into(), sha: "p".into(), classified: false }];
        let actions = plan(&disk, &[], TreeRead::Complete);
        assert_eq!(counts(&actions).skipped, 1);
        assert_eq!(counts(&actions).posted, 0);
    }
}

// ─────────────────────────── the delta ───────────────────────────

/// How this run decided what to look at. The run REPORTS which it did — a full
/// walk that pretends to be a delta (or the reverse) is the reporting class
/// this card exists to end.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Scope {
    /// git gave us the changes between the watermark and HEAD.
    Delta { from: String, to: String },
    /// No usable watermark: the whole tree, and the reason it had to be.
    Full { why: &'static str },
}

/// One line of `git diff --name-status`, decided. Renames arrive as
/// `R<score>\told\tnew` and are a move, never a delete-plus-add: collapsing
/// them loses the row's history and churns the graph for nothing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Touched(String),
    Removed(String),
    Renamed { from: String, to: String },
}

/// Parse `git diff --name-status -z`-style lines given as plain tab-separated
/// text. Unknown statuses are NOT guessed — an unrecognised line makes the
/// caller fall back to a full walk rather than silently skip a file.
pub fn parse_name_status(line: &str) -> Option<Change> {
    let mut it = line.split('\t');
    let status = it.next()?;
    let a = it.next()?;
    let code = status.chars().next()?;
    match code {
        'A' | 'M' | 'C' | 'T' => Some(Change::Touched(a.to_string())),
        'D' => Some(Change::Removed(a.to_string())),
        'R' => it.next().map(|b| Change::Renamed { from: a.to_string(), to: b.to_string() }),
        _ => None,
    }
}

/// Decide the scope from what we know about the watermark. A watermark that is
/// missing, empty, or names a commit this clone does not have is not an error
/// to swallow: it is a full walk that SAYS why.
pub fn scope_for(watermark: Option<&str>, head: &str, watermark_is_reachable: bool) -> Scope {
    match watermark {
        None => Scope::Full { why: "no watermark on the graph — first run" },
        Some(w) if w.trim().is_empty() => Scope::Full { why: "watermark is empty" },
        Some(_) if !watermark_is_reachable => {
            Scope::Full { why: "watermark commit is not in this clone (rebase, force-push or shallow)" }
        }
        Some(w) if w == head => Scope::Delta { from: w.to_string(), to: head.to_string() },
        Some(w) => Scope::Delta { from: w.to_string(), to: head.to_string() },
    }
}

impl Scope {
    pub fn label(&self) -> String {
        match self {
            Scope::Delta { from, to } => {
                let short = |s: &String| s.chars().take(9).collect::<String>();
                format!("delta {}..{}", short(from), short(to))
            }
            Scope::Full { why } => format!("full ({})", why),
        }
    }
}

#[cfg(test)]
mod delta_4173 {
    use super::*;

    #[test]
    fn name_status_reads_adds_edits_and_removals() {
        assert_eq!(parse_name_status("A\tplatform/a.rs"), Some(Change::Touched("platform/a.rs".into())));
        assert_eq!(parse_name_status("M\tplatform/a.rs"), Some(Change::Touched("platform/a.rs".into())));
        assert_eq!(parse_name_status("D\tplatform/gone.rs"), Some(Change::Removed("platform/gone.rs".into())));
    }

    // A rename is a MOVE. Read as delete+add it would drop the row and re-mint
    // it under a new name, losing everything hung off the old one.
    #[test]
    fn a_rename_is_a_move_not_a_delete_plus_an_add() {
        assert_eq!(
            parse_name_status("R096\tplatform/old.rs\tplatform/new.rs"),
            Some(Change::Renamed { from: "platform/old.rs".into(), to: "platform/new.rs".into() })
        );
    }

    // NEGATIVE PROOF (#3734): an unrecognised status is NOT silently dropped.
    // Returning None is what makes the caller fall back to a full walk; a
    // parser that guessed would lose the file without anyone knowing.
    #[test]
    fn negative_proof_an_unknown_status_refuses_rather_than_guessing() {
        assert_eq!(parse_name_status("U\tplatform/conflicted.rs"), None);
        assert_eq!(parse_name_status("garbage"), None);
        assert_eq!(parse_name_status("R096\tplatform/old.rs"), None, "a rename missing its target is not a rename");
    }

    // AC: a full walk happens only when the watermark is unusable — and the run
    // says WHICH it did and why. Four ways to be unusable, all named.
    #[test]
    fn an_unusable_watermark_forces_a_full_walk_that_says_why() {
        let head = "abc123def";
        for (mark, reachable, expect) in [
            (None, true, "first run"),
            (Some(""), true, "empty"),
            (Some("deadbeef"), false, "not in this clone"),
        ] {
            match scope_for(mark, head, reachable) {
                Scope::Full { why } => assert!(why.contains(expect), "why should mention {}: {}", expect, why),
                Scope::Delta { .. } => panic!("watermark {:?} should force a full walk", mark),
            }
        }
    }

    // NEGATIVE PROOF: the control. A usable watermark must NOT force a full
    // walk — otherwise "always full" would pass the test above forever.
    #[test]
    fn negative_proof_a_usable_watermark_takes_the_delta_path() {
        match scope_for(Some("abc123def"), "999fff000", true) {
            Scope::Delta { from, to } => {
                assert_eq!(from, "abc123def");
                assert_eq!(to, "999fff000");
            }
            Scope::Full { why } => panic!("a reachable watermark must not go full: {}", why),
        }
        assert!(scope_for(Some("abc123def"), "999fff000", true).label().starts_with("delta abc123def"));
    }
}

// ─────────────────────────── the reconcile ───────────────────────────

/// What the nightly full pass found. Drift is not a number to log and move on
/// from: it NAMES the paths, in both directions, because "graph has 6,140,
/// tree has 6,175" tells nobody which thirty-five.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Drift {
    /// In the tree, classified, and the graph has no row for it.
    pub missing_from_graph: Vec<String>,
    /// A row whose path the tree does not have.
    pub missing_from_tree: Vec<String>,
}

impl Drift {
    pub fn is_clean(&self) -> bool {
        self.missing_from_graph.is_empty() && self.missing_from_tree.is_empty()
    }
    /// The morning line. Clean says so in one sentence; dirty names paths.
    pub fn report(&self) -> String {
        if self.is_clean() {
            return "reconcile: clean — the graph matches the tree".to_string();
        }
        let mut parts = Vec::new();
        if !self.missing_from_graph.is_empty() {
            parts.push(format!("{} in the tree with no row: {}", self.missing_from_graph.len(), self.missing_from_graph.join(", ")));
        }
        if !self.missing_from_tree.is_empty() {
            parts.push(format!("{} rows with no file: {}", self.missing_from_tree.len(), self.missing_from_tree.join(", ")));
        }
        format!("reconcile: DRIFT — {}", parts.join(" · "))
    }
}

/// Compare the tree to the graph, both directions. Unclassified files are not
/// drift — the model has no name for them, so the graph correctly has no row.
pub fn reconcile(disk: &[OnDisk], graph: &[InGraph]) -> Drift {
    let mut d = Drift::default();
    for f in disk.iter().filter(|f| f.classified) {
        if !graph.iter().any(|g| g.path == f.path) {
            d.missing_from_graph.push(f.path.clone());
        }
    }
    for g in graph {
        if !disk.iter().any(|f| f.path == g.path) {
            d.missing_from_tree.push(g.path.clone());
        }
    }
    d.missing_from_graph.sort();
    d.missing_from_tree.sort();
    d
}

#[cfg(test)]
mod reconcile_4173 {
    use super::*;

    fn d(path: &str) -> OnDisk {
        OnDisk { path: path.into(), sha: "s".into(), classified: true }
    }
    fn g(path: &str) -> InGraph {
        InGraph { path: path.into(), sha: "s".into() }
    }

    #[test]
    fn a_graph_that_matches_the_tree_reconciles_clean() {
        let drift = reconcile(&[d("a.rs"), d("b.ts")], &[g("a.rs"), g("b.ts")]);
        assert!(drift.is_clean());
        assert!(drift.report().contains("clean"));
    }

    // NEGATIVE PROOF (#3734): the check must go red on real drift and NAME the
    // paths — a reconcile that can only say "clean" is a check that cannot
    // distinguish the two states it exists to separate.
    #[test]
    fn negative_proof_one_unreachable_row_makes_the_reconcile_red_and_names_it() {
        let drift = reconcile(&[d("a.rs")], &[g("a.rs"), g("ghost.rs")]);
        assert!(!drift.is_clean(), "a row with no file is drift");
        assert_eq!(drift.missing_from_tree, vec!["ghost.rs".to_string()]);
        assert!(drift.report().contains("ghost.rs"), "the report names the path: {}", drift.report());
    }

    // The other direction: 6,175 files and 0 rows is the state as of today, and
    // the reconcile must call that drift rather than clean.
    #[test]
    fn negative_proof_files_with_no_rows_are_drift_in_the_other_direction() {
        let drift = reconcile(&[d("a.rs"), d("b.ts")], &[]);
        assert!(!drift.is_clean());
        assert_eq!(drift.missing_from_graph.len(), 2);
        assert!(drift.report().contains("no row"));
    }

    // An unclassified file has no row BY DESIGN. Counting it as drift would
    // make every run red forever over .png files and train everyone to ignore it.
    #[test]
    fn a_file_the_model_cannot_name_is_not_drift() {
        let disk = [OnDisk { path: "logo.png".into(), sha: "p".into(), classified: false }];
        assert!(reconcile(&disk, &[]).is_clean());
    }
}

// ─────────────────────────── the watermark ───────────────────────────

/// The watermark is only advanced by a run that has the right to advance it.
///
/// A run that refused deletes, skipped files it could not read, or failed a
/// write has NOT established that the graph matches this commit. Advancing
/// anyway is how a delta walk silently inherits a hole: the next run asks
/// "what changed since HEAD" and is told "nothing", forever.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Watermark {
    /// Safe to record: this commit is now the graph's known-good point.
    Advance(String),
    /// Left where it was, and why.
    Hold(&'static str),
}

pub fn watermark_after(head: &str, read: TreeRead, failed_writes: usize, scope_was_full: bool) -> Watermark {
    if failed_writes > 0 {
        return Watermark::Hold("a write failed — the graph does not match this commit");
    }
    if read == TreeRead::Partial {
        return Watermark::Hold("the tree read was partial — deletes were refused, so orphans may remain");
    }
    // A Scoped (delta) read is not a failure: it proved the files it walked,
    // and the nightly full pass proves the rest.
    if !scope_was_full {
        // A delta run only proves the changed files. That is enough to move
        // forward — the nightly full pass is what proves the whole.
        return Watermark::Advance(head.to_string());
    }
    Watermark::Advance(head.to_string())
}

#[cfg(test)]
mod watermark_4173 {
    use super::*;

    #[test]
    fn a_clean_run_advances_the_watermark_to_head() {
        assert_eq!(
            watermark_after("abc123", TreeRead::Complete, 0, true),
            Watermark::Advance("abc123".to_string())
        );
    }

    // NEGATIVE PROOF (#3734): the two states that must NOT advance. Without
    // these the next delta run asks "what changed since HEAD", hears
    // "nothing", and the hole becomes permanent.
    #[test]
    fn negative_proof_a_failed_write_holds_the_watermark_where_it_was() {
        match watermark_after("abc123", TreeRead::Complete, 1, true) {
            Watermark::Hold(why) => assert!(why.contains("write failed"), "{why}"),
            Watermark::Advance(_) => panic!("a run with a failed write must not advance the watermark"),
        }
    }

    #[test]
    fn negative_proof_a_partial_tree_read_holds_the_watermark() {
        match watermark_after("abc123", TreeRead::Partial, 0, true) {
            Watermark::Hold(why) => assert!(why.contains("partial"), "{why}"),
            Watermark::Advance(_) => panic!("a partial read must not advance the watermark"),
        }
    }

    // A delta run is allowed to advance: it proved the files that changed, and
    // the nightly full pass proves the rest. Without this the watermark would
    // only ever move on full walks and deltas would never compound.
    #[test]
    fn a_clean_delta_run_still_advances() {
        assert_eq!(
            watermark_after("def456", TreeRead::Complete, 0, false),
            Watermark::Advance("def456".to_string())
        );
    }
}
