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

/// #4185 — test CASE rows: parsers, covers, the share gate, the case plan.
pub mod cases;
pub mod domain;
pub mod pages;

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
    /// #4199 — images, icons, PDFs, app bundles: the repo's media, not its text.
    Asset,
    /// #4199 — SPARQL files (.sparql, .rq): queries the code runs against the graph.
    Query,
    /// #4199 — view templates (.ejs, .hbs): rendered, not executed.
    Template,
    /// #4199 — a tracked file with no type of its own: .done/.consumed/.bak/.pid
    /// markers and extension-less non-scripts. Named so the graph can say which
    /// files are noise rather than pretend they are not there.
    Marker,
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
            Kind::Asset => "asset",
            Kind::Query => "query",
            Kind::Template => "template",
            Kind::Marker => "marker",
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
const LANG: [(&str, &str); 18] = [
    (".json", "json"),
    (".css", "css"),
    (".scss", "css"),
    (".html", "html"),
    (".htm", "html"),
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
const KIND_BY_EXT: [(&str, Kind); 49] = [
    (".heic", Kind::Asset),
    (".owl", Kind::Config),
    (".png", Kind::Asset),
    (".jpg", Kind::Asset),
    (".jpeg", Kind::Asset),
    (".gif", Kind::Asset),
    (".svg", Kind::Asset),
    (".ico", Kind::Asset),
    (".icns", Kind::Asset),
    (".pdf", Kind::Asset),
    (".car", Kind::Asset),
    (".rsrc", Kind::Asset),
    (".scpt", Kind::Asset),
    (".sparql", Kind::Query),
    (".rq", Kind::Query),
    (".nt", Kind::Data),
    (".tsv", Kind::Data),
    (".jsonl", Kind::Data),
    (".b64", Kind::Data),
    (".ejs", Kind::Template),
    (".hbs", Kind::Template),
    (".mmd", Kind::Doc),
    (".txt", Kind::Doc),
    (".feature", Kind::Test),
    (".css", Kind::Code),
    (".scss", Kind::Code),
    (".swift", Kind::Code),
    (".ini", Kind::Config),
    (".xml", Kind::Config),
    (".lock", Kind::Config),
    (".done", Kind::Marker),
    (".consumed", Kind::Marker),
    (".bak", Kind::Marker),
    (".backup", Kind::Marker),
    (".backup-shm", Kind::Marker),
    (".backup-wal", Kind::Marker),
    (".pid", Kind::Marker),
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
        || base.ends_with(".feature")
        || (base.starts_with("test_") && base.ends_with(".py"))
        || (base.starts_with("test-") && base.ends_with(".sh"))
}

/// The whole rule, as one pure function: same path in, same answer out, every
/// time. Order matters — test wins over extension, extension over language,
/// and anything the model has no name for is SKIPPED rather than guessed.
pub fn classify(rel: &str, has_rust_test_attr: bool) -> Verdict {
    classify_with_head(rel, has_rust_test_attr, None)
}

/// #4199 — the language a shebang names, or None.
fn lang_of_shebang(head: &str) -> Option<&'static str> {
    let first = head.lines().next().unwrap_or("");
    if !first.starts_with("#!") {
        return None;
    }
    if first.contains("bash") || first.contains("/sh") || first.contains("zsh") {
        Some("bash")
    } else if first.contains("python") {
        Some("python")
    } else if first.contains("node") {
        Some("javascript")
    } else {
        Some("bash")
    }
}

/// The whole rule with the file's first bytes for extension-less names. Order
/// matters — test wins over extension, extension over language — and a name
/// nobody can type is still SKIPPED, never guessed: an unknown EXTENSION is the
/// model's to add. An extension-LESS file is decided by what it is: a shebang
/// script is code, a dotfile is config, an app bundle's insides are assets, a
/// well-known bare name (Makefile, LICENSE) is what it says, and anything else
/// is a marker — a file the graph names as noise rather than pretends is absent.
pub fn classify_with_head(rel: &str, has_rust_test_attr: bool, head: Option<&str>) -> Verdict {
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
    if ext.is_empty() {
        let base = rel.rsplit('/').next().unwrap_or(rel);
        if let Some(l) = head.and_then(lang_of_shebang) {
            return Verdict::Classified(Kind::Code, Some(l));
        }
        if base.starts_with('.') {
            return Verdict::Classified(Kind::Config, None);
        }
        if rel.contains(".app/") {
            return Verdict::Classified(Kind::Asset, None);
        }
        if matches!(base, "Makefile" | "Dockerfile" | "Justfile" | "Procfile") {
            return Verdict::Classified(Kind::Code, None);
        }
        if matches!(
            base,
            "LICENSE" | "README" | "CHANGELOG" | "NOTICE" | "AUTHORS"
        ) {
            return Verdict::Classified(Kind::Doc, None);
        }
        return Verdict::Classified(Kind::Marker, None);
    }
    Verdict::Skip
}

#[cfg(test)]
mod log_domain_4222 {
    use super::*;

    fn doms() -> Vec<String> {
        ["cicd", "messages", "monitors", "spine", "logs"].iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn a_log_is_placed_by_the_job_that_writes_it_then_its_name() {
        assert_eq!(log_domain("com.chorus.werk-sweep", "/x/werk-sweep.log", &doms(), &[]).as_deref(), Some("cicd"));
        assert_eq!(log_domain("unmanaged", "/x/nudge-delivery.log", &doms(), &[]).as_deref(), Some("messages"));
        assert_eq!(log_domain("unmanaged", "/x/heartbeat-probe.log", &doms(), &[]).as_deref(), Some("monitors"));
    }

    #[test]
    fn negative_proof_a_log_naming_nothing_stays_unplaced() {
        // #4222 — the two files this used to name (watcher.log, chorus.log) are
        // now assigned by hand in LOG_FILE_DOMAIN, so they are no longer the
        // unplaced case. The rule they encoded still holds for everything the
        // table does NOT name: no default, no guess, report it by name.
        assert_eq!(log_domain("unmanaged", "/x/zzz-unknown.log", &doms(), &[]), None);
        assert_eq!(log_domain("unmanaged", "/x/quux.out", &doms(), &[]), None);
    }

    #[test]
    fn a_domain_the_model_lacks_is_never_invented() {
        // "search" is a real domain but not in this caller's list: no tag.
        assert_eq!(log_domain("unmanaged", "/x/embed-worker.log", &doms(), &[]), None);
    }
}

#[cfg(test)]
mod classify_4173 {
    use super::*;

    // #4199 — Jeff: every tracked file has a row. The kinds the model gained,
    // and the extension-less rule. NEGATIVE PROOF: an unknown EXTENSION is still
    // skipped, never guessed — widening the set stays a model edit.
    #[test]
    fn the_4199_kinds_name_what_was_skipped_and_an_unknown_extension_still_skips() {
        assert_eq!(
            classify("designing/x.png", false),
            Verdict::Classified(Kind::Asset, None)
        );
        assert_eq!(
            classify("platform/q/owners.rq", false),
            Verdict::Classified(Kind::Query, None)
        );
        assert_eq!(
            classify("platform/api/views/x.ejs", false),
            Verdict::Classified(Kind::Template, None)
        );
        assert_eq!(
            classify("docs/diagrams/x.mmd", false),
            Verdict::Classified(Kind::Doc, None)
        );
        assert_eq!(
            classify("proving/flows/x.feature", false),
            Verdict::Classified(Kind::Test, None)
        );
        assert_eq!(
            classify("platform/api/package-lock.json", false),
            Verdict::Classified(Kind::Config, Some("json"))
        );
        assert_eq!(
            classify("Cargo.lock", false),
            Verdict::Classified(Kind::Config, None)
        );
        assert_eq!(
            classify("platform/state/x.done", false),
            Verdict::Classified(Kind::Marker, None)
        );
        assert_eq!(
            classify("data/x.nt", false),
            Verdict::Classified(Kind::Data, None)
        );
        assert_eq!(
            classify("weird/file.xyz", false),
            Verdict::Skip,
            "an unknown extension is the model's to add, not ours to guess"
        );
    }

    #[test]
    fn extension_less_files_are_decided_by_what_they_are() {
        assert_eq!(
            classify_with_head(
                "platform/scripts/werk",
                false,
                Some("#!/usr/bin/env bash\nset -u")
            ),
            Verdict::Classified(Kind::Code, Some("bash"))
        );
        assert_eq!(
            classify_with_head("skills/x/run", false, Some("#!/usr/bin/env python3\n")),
            Verdict::Classified(Kind::Code, Some("python"))
        );
        assert_eq!(
            classify_with_head(".gitignore", false, Some("target/")),
            Verdict::Classified(Kind::Config, None)
        );
        assert_eq!(
            classify_with_head("platform/apps/X.app/Contents/PkgInfo", false, Some("APPL")),
            Verdict::Classified(Kind::Asset, None)
        );
        assert_eq!(
            classify_with_head("Makefile", false, Some("all:")),
            Verdict::Classified(Kind::Code, None)
        );
        assert_eq!(
            classify_with_head("designing/claudemd/PROTOCOL_VERSION", false, Some("1.6.0")),
            Verdict::Classified(Kind::Marker, None)
        );
    }

    // #3872 / #4185 — the zero-browser-tests hole: a playwright spec under
    // proving/ is a test file. The crawler walks git, so discovery IS the tree;
    // this is the one rule that decides whether that tree's .spec.cjs is seen.
    #[test]
    fn a_playwright_spec_under_proving_is_a_test_file() {
        assert!(is_test_path(
            "proving/flows/clearing-base-path-3872.spec.cjs"
        ));
        assert_eq!(
            classify("proving/flows/clearing-base-path-3872.spec.cjs", false),
            Verdict::Classified(Kind::Test, Some("javascript"))
        );
    }

    // AC: kind and language come from the model's named values, never free strings.
    #[test]
    fn every_kind_the_model_serves_is_reachable_and_named_exactly() {
        let cases = [
            (
                "platform/services/werk-test/src/main.rs",
                Kind::Code,
                Some("rust"),
            ),
            ("platform/api/src/server.ts", Kind::Code, Some("typescript")),
            ("platform/tests/4145-runner.bats", Kind::Test, Some("bash")),
            (
                "platform/api/tests/index-db.test.ts",
                Kind::Test,
                Some("typescript"),
            ),
            ("designing/docs/crawler.html", Kind::Doc, Some("html")),
            ("roles/kade/current-work.md", Kind::Doc, Some("markdown")),
            ("platform/api/package.json", Kind::Config, Some("json")),
            (
                "roles/silas/ontology/chorus.ttl",
                Kind::Config,
                Some("turtle"),
            ),
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
        // #4199 widened the kinds (png/svg are assets, LICENSE is doc); the proof
        // keeps the property on extensions the model still has no name for.
        for path in [
            "platform/api/public/font.woff2",
            "weird/file.xyz",
            "a/b.woff",
        ] {
            assert_eq!(classify(path, false), Verdict::Skip, "{} must skip", path);
        }
    }

    // The two states the classifier exists to separate: a .rs that declares
    // tests is a test, one that does not is code. A check that cannot tell
    // them apart would mark the whole workspace one or the other.
    #[test]
    fn negative_proof_a_rust_file_is_test_or_code_by_its_content_not_its_folder() {
        assert_eq!(
            classify("platform/services/werk-test/src/lib.rs", true),
            Verdict::Classified(Kind::Test, Some("rust"))
        );
        assert_eq!(
            classify("platform/services/werk-test/src/lib.rs", false),
            Verdict::Classified(Kind::Code, Some("rust"))
        );
    }

    // AC: deterministic. Same path, same answer — no clock, no cwd, no order.
    #[test]
    fn deterministic_same_path_answers_the_same_twice() {
        let p = "platform/scripts/test-nightly.sh";
        assert_eq!(classify(p, false), classify(p, false));
        assert_eq!(
            classify(p, false),
            Verdict::Classified(Kind::Test, Some("bash"))
        );
    }

    // A dotfile has no extension to key on and must not be mistaken for one:
    // ".gitignore" is not an extension of a file called "".
    #[test]
    fn negative_proof_a_dotfile_is_not_read_as_an_extension() {
        // ".gitignore" is not a file called "" with extension ".gitignore": it is an
        // extension-less dotfile, which #4199 names config. Had the dot been read as
        // an extension, the verdict would be Skip (no kind for ".gitignore").
        assert_eq!(
            classify(".gitignore", false),
            Verdict::Classified(Kind::Config, None)
        );
        assert_eq!(
            classify("platform/api/.eslintrc.json", false),
            Verdict::Classified(Kind::Config, Some("json"))
        );
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
    /// Every OTHER field the row carries, verbatim, as the door served it.
    ///
    /// #4178 — the crawler used to read a row and keep two strings. That was
    /// fine while it could only create rows, and wrong the moment it had to
    /// update one: the DAL is single-writer full-replace by design (#3345), so
    /// a writer that restates only its own fields DELETES everyone else's —
    /// `fileInDomain` and `fileHasOwner` among them. Keeping the whole row is
    /// what lets an update put the complete entity back.
    pub other: Vec<(String, String)>,
}

/// The whole decision, as a pure function of (tree, graph, how well we read the
/// tree). No clock, no network, no filesystem.
pub fn plan(disk: &[OnDisk], graph: &[InGraph], read: TreeRead) -> Vec<Action> {
    plan_with(disk, graph, read, &|_, _| false)
}

/// #4201 — a file's CONTENT is not the only thing that can go stale. The domain
/// a file belongs to is computed from its text, and a rule change moves it
/// while the sha stands still: tagging `hasDomain` on a tree whose shas all
/// match produced 30 replaces out of 6,226 rows, because only a changed sha
/// could make a row eligible. `restate` answers, for one row, whether what the
/// crawler would write differs from what the row holds.
pub fn plan_with(
    disk: &[OnDisk],
    graph: &[InGraph],
    read: TreeRead,
    restate: &dyn Fn(&str, &InGraph) -> bool,
) -> Vec<Action> {
    let mut out = Vec::new();
    for f in disk {
        if !f.classified {
            out.push(Action::Skipped {
                path: f.path.clone(),
            });
            continue;
        }
        match graph.iter().find(|g| g.path == f.path) {
            None => out.push(Action::Post {
                path: f.path.clone(),
            }),
            Some(g) if g.sha != f.sha || restate(&f.path, g) => out.push(Action::Replace {
                path: f.path.clone(),
            }),
            Some(_) => out.push(Action::Unchanged {
                path: f.path.clone(),
            }),
        }
    }
    // Orphans: a row whose path the tree no longer has. Only decidable when the
    // tree was read in full.
    if read == TreeRead::Complete {
        for g in graph {
            if !disk.iter().any(|f| f.path == g.path) {
                out.push(Action::Delete {
                    path: g.path.clone(),
                });
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
    actions.iter().any(|a| {
        matches!(
            a,
            Action::Post { .. } | Action::Replace { .. } | Action::Delete { .. }
        )
    })
}

#[cfg(test)]
mod plan_4173 {
    use super::*;

    fn d(path: &str, sha: &str) -> OnDisk {
        OnDisk {
            path: path.into(),
            sha: sha.into(),
            classified: true,
        }
    }
    fn g(path: &str, sha: &str) -> InGraph {
        InGraph {
            path: path.into(),
            sha: sha.into(),
            other: Vec::new(),
        }
    }

    #[test]
    fn a_new_file_posts_a_changed_file_replaces_an_identical_file_is_left_alone() {
        let disk = [d("a.rs", "aaa"), d("b.rs", "NEW"), d("c.rs", "ccc")];
        let graph = [g("a.rs", "aaa"), g("b.rs", "old")];
        let actions = plan(&disk, &graph, TreeRead::Complete);
        assert!(actions.contains(&Action::Unchanged {
            path: "a.rs".into()
        }));
        assert!(actions.contains(&Action::Replace {
            path: "b.rs".into()
        }));
        assert!(actions.contains(&Action::Post {
            path: "c.rs".into()
        }));
    }

    // AC: idempotent — a second run writes NOTHING. Proven by running the plan
    // against its own result, not asserted in a comment.
    #[test]
    fn a_second_run_over_an_unchanged_tree_writes_nothing() {
        let disk = [d("a.rs", "aaa"), d("b.ts", "bbb")];
        let graph = [g("a.rs", "aaa"), g("b.ts", "bbb")];
        let actions = plan(&disk, &graph, TreeRead::Complete);
        assert!(
            !writes_anything(&actions),
            "a no-op walk must write nothing: {:?}",
            actions
        );
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
        assert!(actions.contains(&Action::Delete {
            path: "gone.rs".into()
        }));
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
        assert_eq!(
            counts(&scoped).deleted,
            0,
            "a delta must delete nothing it did not walk: {:?}",
            scoped
        );
        assert_eq!(
            counts(&scoped).replaced,
            1,
            "it still updates what DID change"
        );

        // The control, with identical inputs: a run that claims to have read the
        // whole tree DOES treat those rows as orphans. Without this the check
        // could not tell the two states apart.
        let full = plan(&changed_only, &whole_graph, TreeRead::Complete);
        assert_eq!(
            counts(&full).deleted,
            2,
            "a full walk still reconciles orphans"
        );
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
        assert_eq!(
            counts(&complete).deleted,
            1,
            "control: a complete read DOES delete"
        );
        assert_eq!(
            counts(&partial).deleted,
            0,
            "a partial read must delete nothing"
        );
    }

    // An unclassifiable file is reported, never silently dropped and never
    // posted as "code" — the plan carries it so the run can name it.
    #[test]
    fn an_unnameable_file_is_counted_as_skipped_not_posted() {
        let disk = [OnDisk {
            path: "logo.png".into(),
            sha: "p".into(),
            classified: false,
        }];
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

/// A watermark says "the graph already holds every tracked file as of this
/// commit". An EMPTY graph cannot be in that state, so the two facts together
/// are proof the store was reset underneath us, not proof there is nothing to do.
///
/// 2026-09-14: the werk store was rebuilt and every CodeFile row went with it.
/// The watermark file survived on disk, so the next run planned a three-file
/// delta against a graph holding zero rows and called that up to date. Only a
/// separate failure stopped it from writing three rows into an empty graph and
/// advancing the watermark over the hole. A delta is only meaningful against the
/// graph the watermark describes; when the graph is gone, so is the delta.
pub fn delta_is_trustworthy(graph_rows: usize, tracked_on_disk: usize) -> bool {
    !(graph_rows == 0 && tracked_on_disk > 0)
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
        'R' => it.next().map(|b| Change::Renamed {
            from: a.to_string(),
            to: b.to_string(),
        }),
        _ => None,
    }
}

/// Decide the scope from what we know about the watermark. A watermark that is
/// missing, empty, or names a commit this clone does not have is not an error
/// to swallow: it is a full walk that SAYS why.
pub fn scope_for(watermark: Option<&str>, head: &str, watermark_is_reachable: bool) -> Scope {
    match watermark {
        None => Scope::Full {
            why: "no watermark on the graph — first run",
        },
        Some(w) if w.trim().is_empty() => Scope::Full {
            why: "watermark is empty",
        },
        Some(_) if !watermark_is_reachable => Scope::Full {
            why: "watermark commit is not in this clone (rebase, force-push or shallow)",
        },
        Some(w) if w == head => Scope::Delta {
            from: w.to_string(),
            to: head.to_string(),
        },
        Some(w) => Scope::Delta {
            from: w.to_string(),
            to: head.to_string(),
        },
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
        assert_eq!(
            parse_name_status("A\tplatform/a.rs"),
            Some(Change::Touched("platform/a.rs".into()))
        );
        assert_eq!(
            parse_name_status("M\tplatform/a.rs"),
            Some(Change::Touched("platform/a.rs".into()))
        );
        assert_eq!(
            parse_name_status("D\tplatform/gone.rs"),
            Some(Change::Removed("platform/gone.rs".into()))
        );
    }

    // A rename is a MOVE. Read as delete+add it would drop the row and re-mint
    // it under a new name, losing everything hung off the old one.
    #[test]
    fn a_rename_is_a_move_not_a_delete_plus_an_add() {
        assert_eq!(
            parse_name_status("R096\tplatform/old.rs\tplatform/new.rs"),
            Some(Change::Renamed {
                from: "platform/old.rs".into(),
                to: "platform/new.rs".into()
            })
        );
    }

    // NEGATIVE PROOF (#3734): an unrecognised status is NOT silently dropped.
    // Returning None is what makes the caller fall back to a full walk; a
    // parser that guessed would lose the file without anyone knowing.
    #[test]
    fn negative_proof_an_unknown_status_refuses_rather_than_guessing() {
        assert_eq!(parse_name_status("U\tplatform/conflicted.rs"), None);
        assert_eq!(parse_name_status("garbage"), None);
        assert_eq!(
            parse_name_status("R096\tplatform/old.rs"),
            None,
            "a rename missing its target is not a rename"
        );
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
                Scope::Full { why } => assert!(
                    why.contains(expect),
                    "why should mention {}: {}",
                    expect,
                    why
                ),
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
        assert!(scope_for(Some("abc123def"), "999fff000", true)
            .label()
            .starts_with("delta abc123def"));
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
    /// #4180 — a row that exists but carries the WRONG content hash. The first
    /// reconcile only compared path sets, so three rows with stale shas read as
    /// "clean — the graph matches the tree" while the graph described a file that
    /// no longer existed in that form. A row that is present and wrong is drift.
    pub stale_sha: Vec<String>,
}

impl Drift {
    pub fn is_clean(&self) -> bool {
        self.missing_from_graph.is_empty()
            && self.missing_from_tree.is_empty()
            && self.stale_sha.is_empty()
    }
    /// The morning line. Clean says so in one sentence; dirty names paths.
    pub fn report(&self) -> String {
        if self.is_clean() {
            return "reconcile: clean — the graph matches the tree".to_string();
        }
        let mut parts = Vec::new();
        if !self.missing_from_graph.is_empty() {
            parts.push(format!(
                "{} in the tree with no row: {}",
                self.missing_from_graph.len(),
                self.missing_from_graph.join(", ")
            ));
        }
        if !self.missing_from_tree.is_empty() {
            parts.push(format!(
                "{} rows with no file: {}",
                self.missing_from_tree.len(),
                self.missing_from_tree.join(", ")
            ));
        }
        if !self.stale_sha.is_empty() {
            parts.push(format!(
                "{} rows with a stale sha: {}",
                self.stale_sha.len(),
                self.stale_sha.join(", ")
            ));
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
        match disk.iter().find(|f| f.path == g.path) {
            None => d.missing_from_tree.push(g.path.clone()),
            Some(f) if f.classified && f.sha != g.sha => d.stale_sha.push(g.path.clone()),
            Some(_) => {}
        }
    }
    d.missing_from_graph.sort();
    d.missing_from_tree.sort();
    d.stale_sha.sort();
    d
}

#[cfg(test)]
mod reconcile_4173 {
    use super::*;

    #[test]
    fn a_row_with_the_wrong_sha_is_drift_not_clean() {
        // #4180 — three prod rows carried stale hashes and reconcile said clean.
        let disk = [OnDisk {
            path: "a.rs".into(),
            sha: "new".into(),
            classified: true,
        }];
        let graph = [InGraph {
            path: "a.rs".into(),
            sha: "old".into(),
            other: Vec::new(),
        }];
        let r = reconcile(&disk, &graph);
        assert!(!r.is_clean());
        assert_eq!(r.stale_sha, vec!["a.rs".to_string()]);
        assert!(r.report().contains("1 rows with a stale sha: a.rs"));
    }

    // NEGATIVE PROOF (#3734): the sha check must not fire on the states that are
    // NOT drift — a matching sha, and an unclassified file the crawler never
    // writes — or every reconcile would be red and the word would mean nothing.
    #[test]
    fn negative_proof_a_matching_sha_and_an_unclassified_file_are_not_sha_drift() {
        let disk = [
            OnDisk {
                path: "a.rs".into(),
                sha: "same".into(),
                classified: true,
            },
            OnDisk {
                path: "x.bin".into(),
                sha: "disk".into(),
                classified: false,
            },
        ];
        let graph = [
            InGraph {
                path: "a.rs".into(),
                sha: "same".into(),
                other: Vec::new(),
            },
            InGraph {
                path: "x.bin".into(),
                sha: "graph".into(),
                other: Vec::new(),
            },
        ];
        let r = reconcile(&disk, &graph);
        assert!(r.stale_sha.is_empty(), "{:?}", r.stale_sha);
    }

    fn d(path: &str) -> OnDisk {
        OnDisk {
            path: path.into(),
            sha: "s".into(),
            classified: true,
        }
    }
    fn g(path: &str) -> InGraph {
        InGraph {
            path: path.into(),
            sha: "s".into(),
            other: Vec::new(),
        }
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
        assert!(
            drift.report().contains("ghost.rs"),
            "the report names the path: {}",
            drift.report()
        );
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
        let disk = [OnDisk {
            path: "logo.png".into(),
            sha: "p".into(),
            classified: false,
        }];
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

pub fn watermark_after(
    head: &str,
    read: TreeRead,
    failed_writes: usize,
    scope_was_full: bool,
) -> Watermark {
    if failed_writes > 0 {
        return Watermark::Hold("a write failed — the graph does not match this commit");
    }
    if read == TreeRead::Partial {
        return Watermark::Hold(
            "the tree read was partial — deletes were refused, so orphans may remain",
        );
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
            Watermark::Advance(_) => {
                panic!("a run with a failed write must not advance the watermark")
            }
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

    #[test]
    fn an_empty_graph_under_a_watermark_is_a_reset_not_a_no_op() {
        assert!(!delta_is_trustworthy(0, 6172));
    }

    // NEGATIVE PROOF: the guard must stay quiet in the two states that look
    // similar but are not a reset — a populated graph, and an empty repo — or
    // it would force a full walk on every ordinary run and mean nothing.
    #[test]
    fn negative_proof_the_reset_guard_does_not_fire_on_a_populated_graph_or_an_empty_tree() {
        assert!(delta_is_trustworthy(5533, 6172));
        assert!(delta_is_trustworthy(1, 6172));
        assert!(delta_is_trustworthy(0, 0));
    }
}

/// The JSON objects inside a collection response's `data` array.
///
/// Zero-dependency and deliberately dumb: find `"data"`, then walk braces,
/// tracking string state so a `{` inside a value cannot open a fake object.
/// Returns each object's body including its braces.
pub fn row_objects(page: &str) -> Vec<&str> {
    let Some(start) = page.find("\"data\"") else {
        return Vec::new();
    };
    let rest = &page[start..];
    let Some(open) = rest.find('[') else {
        return Vec::new();
    };
    let mut out = Vec::new();
    let bytes = rest.as_bytes();
    let (mut depth, mut obj_start, mut in_str, mut esc) = (0usize, 0usize, false, false);
    for i in open..bytes.len() {
        let c = bytes[i] as char;
        if in_str {
            if esc {
                esc = false;
            } else if c == '\\' {
                esc = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => {
                if depth == 0 {
                    obj_start = i;
                }
                depth += 1;
            }
            '}' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    out.push(&rest[obj_start..=i]);
                }
            }
            ']' if depth == 0 => break,
            _ => {}
        }
    }
    out
}

/// Flat string key/value pairs of one row object. Non-string values are skipped
/// — the crawler restates what it can see, and a field it cannot read is a field
/// it must not claim to preserve.
pub fn row_fields(obj: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let b = obj.as_bytes();
    let mut i = 0usize;
    while i < b.len() {
        if b[i] != b'"' {
            i += 1;
            continue;
        }
        let Some(key_end) = find_str_end(obj, i + 1) else {
            break;
        };
        let key = &obj[i + 1..key_end];
        let mut j = key_end + 1;
        while j < b.len() && (b[j] as char).is_whitespace() {
            j += 1;
        }
        if j >= b.len() || b[j] != b':' {
            i = key_end + 1;
            continue;
        }
        j += 1;
        while j < b.len() && (b[j] as char).is_whitespace() {
            j += 1;
        }
        if j < b.len() && b[j] == b'"' {
            let Some(val_end) = find_str_end(obj, j + 1) else {
                break;
            };
            out.push((json_unescape(key), json_unescape(&obj[j + 1..val_end])));
            i = val_end + 1;
        } else {
            i = j;
        }
    }
    out
}

/// A JSON string body's VALUE: `\"` `\\` `\/` `\n` `\t` `\r` `\b` `\f` and
/// `\uXXXX` (surrogate pairs joined) decoded; an unknown escape passes through.
///
/// #4185 — row_fields used to hand back the raw spelling. A served testName
/// `has zero =\"// occurrences` compared raw against the parsed name never
/// matched: 125 quoted cases were deleted and re-posted on every full pass, and
/// the reconcile read them as drift in both directions. fields_json re-escapes
/// on the way out, so the value is the only honest thing to hold in between.
pub fn json_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match it.next() {
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('b') => out.push('\u{8}'),
            Some('f') => out.push('\u{c}'),
            Some('u') => {
                let hex: String = it.by_ref().take(4).collect();
                let Ok(code) = u32::from_str_radix(&hex, 16) else {
                    out.push_str("\\u");
                    out.push_str(&hex);
                    continue;
                };
                if (0xD800..0xDC00).contains(&code) {
                    let mut peek = it.clone();
                    if peek.next() == Some('\\') && peek.next() == Some('u') {
                        let low: String = peek.by_ref().take(4).collect();
                        if let Ok(lo) = u32::from_str_radix(&low, 16) {
                            if (0xDC00..0xE000).contains(&lo) {
                                let cp = 0x10000 + ((code - 0xD800) << 10) + (lo - 0xDC00);
                                if let Some(ch) = char::from_u32(cp) {
                                    out.push(ch);
                                    it = peek;
                                    continue;
                                }
                            }
                        }
                    }
                    out.push('\u{FFFD}');
                } else {
                    out.push(char::from_u32(code).unwrap_or('\u{FFFD}'));
                }
            }
            Some(o) => {
                out.push('\\');
                out.push(o);
            }
            None => out.push('\\'),
        }
    }
    out
}

#[cfg(test)]
mod row_values_4185 {
    use super::*;

    // NEGATIVE PROOF (#3734): the served spelling and the parsed name must be
    // ONE value, or every quoted case is re-posted every run (125 on the variant).
    #[test]
    fn negative_proof_a_served_name_with_an_escaped_quote_reads_as_its_value() {
        let obj = r#"{"name": "t-1", "filePath": "a.test.ts", "testName": "has zero =\"// occurrences in index.html"}"#;
        let f = row_fields(obj);
        let case = f
            .iter()
            .find(|(k, _)| k == "testName")
            .map(|(_, v)| v.clone())
            .unwrap();
        assert_eq!(case, "has zero =\"// occurrences in index.html");
        assert_ne!(
            case, r#"has zero =\"// occurrences in index.html"#,
            "the raw JSON spelling is not the value"
        );
    }

    #[test]
    fn json_unescape_decodes_every_escape() {
        assert_eq!(json_unescape(r"a\nb\tc\\d\/eé😀"), "a\nb\tc\\d/eé😀");
        assert_eq!(json_unescape("plain"), "plain");
        assert_eq!(
            json_unescape(r"\x"),
            r"\x",
            "an unknown escape passes through"
        );
    }
}

fn find_str_end(s: &str, from: usize) -> Option<usize> {
    let b = s.as_bytes();
    let mut i = from;
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'"' => return Some(i),
            _ => i += 1,
        }
    }
    None
}

/// The row to PUT back: everything the door served, with the crawler's own
/// fields written over it. Server-managed keys are dropped — restating them is
/// either refused or a lie about who changed the row.
pub fn merge_row(
    existing: &[(String, String)],
    owned: &[(String, String)],
) -> Vec<(String, String)> {
    const SERVER_OWNED: &[&str] = &[
        "name",
        "label",
        "iri",
        "type",
        "id",
        "self",
        "created",
        "modified",
        "creator",
        "version",
        "changedAt",
        "changedIn",
        "ownedBy",
    ];
    let mut out: Vec<(String, String)> = existing
        .iter()
        .filter(|(k, v)| !SERVER_OWNED.contains(&k.as_str()) && !v.is_empty())
        .cloned()
        .collect();
    // #4222 — a key the caller supplies more than once is multi-valued: every
    // existing value of that key is dropped and all the new ones are kept, so a
    // file can carry every domain it serves. A key supplied once keeps the old
    // single-slot behaviour.
    let mut multi: Vec<&str> = Vec::new();
    for (i, (k, _)) in owned.iter().enumerate() {
        if owned.iter().skip(i + 1).any(|(ok, _)| ok == k) && !multi.contains(&k.as_str()) {
            multi.push(k.as_str());
        }
    }
    out.retain(|(k, _)| !multi.contains(&k.as_str()));
    for (k, v) in owned {
        if multi.contains(&k.as_str()) {
            out.push((k.clone(), v.clone()));
            continue;
        }
        match out.iter_mut().find(|(ek, _)| ek == k) {
            Some(slot) => slot.1 = v.clone(),
            None => out.push((k.clone(), v.clone())),
        }
    }
    out
}

/// The door refuses a value that already carries the prefix its mint adds
/// (`double-prefix: 'code-kind-doc' already starts with 'code-kind-'`). The
/// refusal names the prefix, so the retry is derived from the server's own
/// words rather than from a table of prefixes kept in step by hand.
pub fn strip_named_prefix(err: &str, fields: &mut [(String, String)]) -> bool {
    let Some(i) = err.find("already starts with '") else {
        return false;
    };
    let tail = &err[i + "already starts with '".len()..];
    let Some(j) = tail.find('\'') else {
        return false;
    };
    let prefix = &tail[..j];
    if prefix.is_empty() {
        return false;
    }
    let mut changed = false;
    for (_, v) in fields.iter_mut() {
        if let Some(bare) = v.strip_prefix(prefix) {
            if !bare.is_empty() {
                *v = bare.to_string();
                changed = true;
            }
        }
    }
    changed
}

/// #4185 — the door caps a write body at 65,536 bytes. A CodeFile row is
/// small enough that 200 of them fit; a Test row carries a path, a case name
/// and a minted name, so 200 of them do not: the first full pass on the
/// variant sent 40 batches of 68,814 bytes and every one came back 422. A
/// batch is bounded by BYTES as well as rows, and the bound is measured
/// against what the door said, with headroom for the brackets and commas.
pub const DOOR_BODY_CAP: usize = 65_536;
pub const BATCH_BODY_BUDGET: usize = 60_000;

/// Would adding a row of `next_len` bytes to a batch currently `body_len`
/// bytes (joined) still fit under the budget?
pub fn batch_accepts(body_len: usize, next_len: usize, budget: usize) -> bool {
    // "+ 1" for the comma this row adds; "+ 2" for the enclosing brackets
    body_len + next_len + 1 + 2 <= budget
}

#[cfg(test)]
mod batch_budget_4185 {
    use super::*;

    #[test]
    fn a_batch_under_the_budget_accepts_the_next_row() {
        assert!(batch_accepts(1_000, 340, BATCH_BODY_BUDGET));
    }

    // NEGATIVE PROOF (#3734): the state that went 422 on the variant — a
    // batch that WOULD cross the door's cap — is refused before it is sent.
    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn negative_proof_a_row_that_would_cross_the_cap_does_not_join_the_batch() {
        assert!(!batch_accepts(59_800, 340, BATCH_BODY_BUDGET));
        // and the budget itself sits under the door's cap
        assert!(BATCH_BODY_BUDGET < DOOR_BODY_CAP);
    }
}

/// #4185 — the mass-delete guard.
///
/// A full walk over a tree that is NOT the tree the graph describes reads every
/// row as an orphan. Measured 2026-09-16 on the variant: a three-file fixture
/// repo pointed at a store holding the whole repo's rows planned 5,338 case
/// deletes and 5,561 file deletes, and spent ten minutes issuing them one at a
/// time. The same shape in production is one wrong `CHORUS_ROOT` — a launchd
/// unit started in the wrong directory, a hand run from a scratch checkout —
/// and it empties the registry the runner selects from.
///
/// A real full pass never deletes half the graph: the tree and the graph differ
/// by a land's worth of files. So a plan whose deletes exceed the share below,
/// over a graph big enough for the share to mean anything, is refused whole and
/// said out loud. Under the floor (a fixture-sized graph) the guard stands down:
/// a three-row graph losing two rows is a test, not a wipe.
pub const MASS_DELETE_SHARE: f64 = 0.50;
pub const MASS_DELETE_FLOOR: usize = 100;

/// Should this run's deletes be refused as a mass delete?
pub fn mass_delete_refused(planned_deletes: usize, graph_rows: usize) -> bool {
    graph_rows >= MASS_DELETE_FLOOR
        && (planned_deletes as f64) > (graph_rows as f64) * MASS_DELETE_SHARE
}

#[cfg(test)]
mod mass_delete_4185 {
    use super::*;

    // NEGATIVE PROOF (#3734): the state measured on the variant — a fixture tree
    // against a whole-repo graph — is refused.
    #[test]
    fn negative_proof_a_fixture_tree_against_a_whole_repo_graph_is_refused() {
        assert!(mass_delete_refused(5_338, 8_187));
        assert!(mass_delete_refused(5_561, 5_564));
    }

    // The controls: a land's worth of deletes is not a wipe, and a tiny graph
    // (the fixture suites) is below the floor so its own deletes still happen.
    #[test]
    fn a_lands_worth_of_deletes_passes_and_a_fixture_sized_graph_is_below_the_floor() {
        assert!(
            !mass_delete_refused(33, 8_187),
            "a land that removed 33 cases is normal"
        );
        assert!(
            !mass_delete_refused(2, 3),
            "a three-row fixture graph losing two rows is a test, not a wipe"
        );
        assert!(!mass_delete_refused(0, 8_187));
    }
}

// ─────────────────────────── identity that outlives ten minutes (#4192) ───────────────────────────

/// The `exp` claim of a JWT, read from its payload without a library: the
/// middle segment, base64url, then the first `"exp":<digits>`. None when the
/// token is not a JWT or carries no exp — the caller then mints fresh rather
/// than guessing a lifetime.
pub fn token_exp(token: &str) -> Option<u64> {
    let payload = token.split('.').nth(1)?;
    let bytes = base64url_decode(payload)?;
    let text = String::from_utf8_lossy(&bytes);
    let at = text.find("\"exp\"")?;
    let rest = text[at + 5..].trim_start().strip_prefix(':')?.trim_start();
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

fn base64url_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let (mut buf, mut bits) = (0u32, 0u32);
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'-' | b'+' => 62,
            b'_' | b'/' => 63,
            b'=' => break,
            _ => return None,
        } as u32;
        buf = (buf << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
            buf &= (1 << bits) - 1;
        }
    }
    Some(out)
}

/// Must the run mint a new identity before its next write? Yes when the token
/// has no readable expiry (nothing to trust) or expires within `margin`
/// seconds. #4192: the crawler identity lives 600 s and the run minted it once,
/// so a pass over 7,858 rows was refused from minute ten on (6,807 failures on
/// 2026-09-16 17:41).
pub fn token_needs_mint(exp: Option<u64>, now: u64, margin: u64) -> bool {
    match exp {
        None => true,
        Some(e) => now + margin >= e,
    }
}

/// The kind prefixes the door has named in its refusals this run. A served
/// edge value carries its target kind's prefix (`code-file-file-…`) and the
/// door refuses that spelling on write with `double-prefix: '…' already
/// starts with '<prefix>'`. Learned once, the prefix is stripped from every
/// later row BEFORE the first PUT — one round trip per update instead of two
/// (measured 2.9 s each at load 9.6, 2026-09-16).
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PrefixMemory {
    known: Vec<String>,
}

impl PrefixMemory {
    /// Remember the prefix a refusal names. True if it named one we did not know.
    pub fn learn(&mut self, err: &str) -> bool {
        let Some(p) = named_prefix(err) else {
            return false;
        };
        if self.known.iter().any(|k| k == p) {
            return false;
        }
        self.known.push(p.to_string());
        true
    }
    /// Strip every known prefix from every value that carries it. True if any changed.
    pub fn apply(&self, fields: &mut [(String, String)]) -> bool {
        let mut changed = false;
        for p in &self.known {
            for (_, v) in fields.iter_mut() {
                if let Some(bare) = v.strip_prefix(p.as_str()) {
                    if !bare.is_empty() {
                        *v = bare.to_string();
                        changed = true;
                    }
                }
            }
        }
        changed
    }
    pub fn is_empty(&self) -> bool {
        self.known.is_empty()
    }
}

/// The prefix a door refusal names, if any.
pub fn named_prefix(err: &str) -> Option<&str> {
    let i = err.find("already starts with '")?;
    let tail = &err[i + "already starts with '".len()..];
    let j = tail.find('\'')?;
    let p = &tail[..j];
    if p.is_empty() {
        None
    } else {
        Some(p)
    }
}

#[cfg(test)]
mod identity_4192 {
    use super::*;

    fn jwt_with_exp(exp: u64) -> String {
        // header.payload.sig — only the payload matters here
        let payload = format!("{{\"sub\":\"crawler\",\"iat\":1,\"exp\":{exp}}}");
        format!("eyJhbGciOiJub25lIn0.{}.sig", b64url(payload.as_bytes()))
    }
    fn b64url(b: &[u8]) -> String {
        const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut s = String::new();
        for chunk in b.chunks(3) {
            let n = chunk.len();
            let v = (chunk[0] as u32) << 16
                | (*chunk.get(1).unwrap_or(&0) as u32) << 8
                | *chunk.get(2).unwrap_or(&0) as u32;
            s.push(T[(v >> 18) as usize & 63] as char);
            s.push(T[(v >> 12) as usize & 63] as char);
            if n > 1 {
                s.push(T[(v >> 6) as usize & 63] as char);
            }
            if n > 2 {
                s.push(T[v as usize & 63] as char);
            }
        }
        s
    }

    #[test]
    fn the_exp_claim_is_read_from_the_payload() {
        assert_eq!(token_exp(&jwt_with_exp(1_789_591_876)), Some(1_789_591_876));
        assert_eq!(token_exp("not-a-jwt"), None);
    }

    // NEGATIVE PROOF (#3734): an expired or expiring token demands a mint; a
    // fresh one does not; a token with no readable exp is never trusted.
    #[test]
    fn negative_proof_an_expiring_token_demands_a_mint_and_a_fresh_one_does_not() {
        let exp = Some(1_000_600);
        assert!(token_needs_mint(exp, 1_000_600, 60), "already expired");
        assert!(token_needs_mint(exp, 1_000_550, 60), "inside the margin");
        assert!(
            !token_needs_mint(exp, 1_000_000, 60),
            "control: nine minutes of life left"
        );
        assert!(token_needs_mint(None, 0, 60), "no exp → mint, never guess");
    }

    #[test]
    fn a_prefix_learned_on_row_one_is_stripped_from_row_two_before_any_round_trip() {
        let mut m = PrefixMemory::default();
        assert!(m.learn("PUT /code/files/x -> HTTP 422 double-prefix: 'code-kind-doc' already starts with 'code-kind-'"));
        assert!(
            !m.learn("PUT … already starts with 'code-kind-'"),
            "known once"
        );
        let mut row2 = vec![
            ("hasKind".to_string(), "code-kind-test".to_string()),
            ("filePath".to_string(), "a.rs".to_string()),
        ];
        assert!(m.apply(&mut row2));
        assert_eq!(row2[0].1, "test");
        assert_eq!(row2[1].1, "a.rs");
    }

    // NEGATIVE PROOF: a value that does not carry the prefix is left alone, and
    // a refusal that names no prefix teaches nothing.
    #[test]
    fn negative_proof_values_without_the_prefix_are_untouched_and_nameless_refusals_teach_nothing()
    {
        let mut m = PrefixMemory::default();
        assert!(!m.learn("PUT … -> HTTP 403 authz"));
        assert!(m.is_empty());
        m.learn("x already starts with 'code-file-'");
        let mut row = vec![("inFile".to_string(), "file-platform-x-abc".to_string())];
        assert!(!m.apply(&mut row));
        assert_eq!(row[0].1, "file-platform-x-abc");
    }
}

// ─────────────────────────── graph vs project (#4199) ───────────────────────────
//
// Jeff, 2026-09-17: "a high level of consistency and completeness between the
// graph and project as we make changes at a high rate — these can't be lossy."
// One line a morning answers it for code, tests and logs together. Everything
// here is pure so each verdict has a fixture that turns it red.

/// The files the model has no kind for, summarised by extension: the count and
/// the top few, so "642 skipped" becomes "png 233, none 180, nt 37 …" and the
/// model can be asked for each one.
pub fn no_kind_summary(paths: &[String]) -> (usize, String) {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for p in paths {
        let base = p.rsplit('/').next().unwrap_or(p);
        let ext = match base.rfind('.') {
            Some(i) if i > 0 => base[i + 1..].to_ascii_lowercase(),
            _ => "none".to_string(),
        };
        match counts.iter_mut().find(|(e, _)| *e == ext) {
            Some(c) => c.1 += 1,
            None => counts.push((ext, 1)),
        }
    }
    counts.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let top: Vec<String> = counts
        .iter()
        .take(6)
        .map(|(e, n)| format!("{e} {n}"))
        .collect();
    (paths.len(), top.join(", "))
}

/// Log files the box writes, read from a launchd plist's StandardOutPath and
/// StandardErrorPath. No plist library: the two keys are followed by one
/// `<string>` each.
pub fn log_paths_in_plist(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for key in ["StandardOutPath", "StandardErrorPath"] {
        let needle = format!("<key>{key}</key>");
        let mut from = 0;
        while let Some(i) = text[from..].find(&needle) {
            let at = from + i + needle.len();
            let rest = &text[at..];
            if let Some(s) = rest.find("<string>") {
                if let Some(e) = rest[s + 8..].find("</string>") {
                    let p = rest[s + 8..s + 8 + e].trim().to_string();
                    if !p.is_empty() && !out.contains(&p) {
                        out.push(p);
                    }
                }
            }
            from = at;
        }
    }
    out
}

/// Log files vs LogSource rows, both ways.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LogDrift {
    /// A log file the box writes with no LogSource row.
    pub files_without_rows: Vec<String>,
    /// A LogSource row whose file is not on the box.
    pub rows_without_files: Vec<String>,
}

impl LogDrift {
    pub fn is_clean(&self) -> bool {
        self.files_without_rows.is_empty() && self.rows_without_files.is_empty()
    }
    pub fn report(&self) -> String {
        if self.is_clean() {
            return "reconcile logs: clean — every log file the box writes has its row and every row has its file".to_string();
        }
        let mut parts = Vec::new();
        if !self.files_without_rows.is_empty() {
            parts.push(format!(
                "{} log file(s) with no row: {}",
                self.files_without_rows.len(),
                self.files_without_rows.join(", ")
            ));
        }
        if !self.rows_without_files.is_empty() {
            parts.push(format!(
                "{} row(s) with no file: {}",
                self.rows_without_files.len(),
                self.rows_without_files.join(", ")
            ));
        }
        format!("reconcile logs: DRIFT — {}", parts.join(" · "))
    }
}

/// `exists` answers whether a row's path is a file on this box: a row may name a
/// log outside the directories we sweep (a worker's own log under ~/.chorus) and
/// still be true. Only a row whose file is gone is drift.
pub fn reconcile_logs(
    files_on_box: &[String],
    row_paths: &[String],
    exists: &dyn Fn(&str) -> bool,
) -> LogDrift {
    let mut d = LogDrift::default();
    for f in files_on_box {
        if !row_paths.contains(f) {
            d.files_without_rows.push(f.clone());
        }
    }
    for r in row_paths {
        if !exists(r) {
            d.rows_without_files.push(r.clone());
        }
    }
    d.files_without_rows.sort();
    d.rows_without_files.sort();
    d
}

/// Every `watermark -> <sha>` a crawl log recorded, in order.
pub fn passes_watermarks(log: &str) -> Vec<String> {
    log.lines()
        .filter_map(|l| l.split("watermark -> ").nth(1))
        .map(|s| s.split_whitespace().next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

/// Lands (first-parent commits on main) that no crawl pass ever covered. A land
/// is covered when some later pass's watermark has it as an ancestor — a pass
/// that walked HEAD after the land saw its files. The relation is injected so
/// the rule is testable without a repo.
pub fn uncovered_lands(
    lands: &[String],
    watermarks: &[String],
    is_ancestor: &dyn Fn(&str, &str) -> bool,
) -> Vec<String> {
    lands
        .iter()
        .filter(|land| {
            !watermarks
                .iter()
                .any(|w| land.as_str() == w.as_str() || is_ancestor(land, w))
        })
        .cloned()
        .collect()
}

/// The morning line, one field per verdict.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectLine {
    pub file_rows: usize,
    pub tracked: usize,
    pub no_kind: usize,
    pub no_kind_top: String,
    pub case_rows: usize,
    pub no_case: usize,
    pub no_case_detail: String,
    pub log_rows: usize,
    pub log_files: usize,
    pub lag_commits: usize,
    pub files_drift: usize,
    pub cases_drift: usize,
    pub logs_drift: usize,
    pub lands: usize,
    pub uncovered: Vec<String>,
    /// #4201 — test files the five rules placed, left in conflict, left unplaced
    pub tag_placed: usize,
    pub tag_conflicts: usize,
    pub tag_unplaced: usize,
}

impl ProjectLine {
    pub fn render(&self) -> String {
        let verdict = |n: usize| {
            if n == 0 {
                "clean".to_string()
            } else {
                format!("DRIFT {n}")
            }
        };
        let lossless = if self.uncovered.is_empty() {
            format!("lossless lands={} all covered", self.lands)
        } else {
            format!(
                "LOSSY lands={} uncovered={} ({})",
                self.lands,
                self.uncovered.len(),
                self.uncovered.join(",")
            )
        };
        format!(
            "graph vs project · complete files={}/{} no-kind={} ({}) cases={} no-case={} ({}) logs={} rows/{} files · tagged placed={} conflicts={} unplaced={} · current lag={} · consistent files={} cases={} logs={} · {}",
            self.file_rows, self.tracked, self.no_kind, self.no_kind_top, self.case_rows, self.no_case, self.no_case_detail, self.log_rows, self.log_files,
            self.tag_placed, self.tag_conflicts, self.tag_unplaced,
            self.lag_commits, verdict(self.files_drift), verdict(self.cases_drift), verdict(self.logs_drift), lossless
        )
    }
    pub fn is_clean(&self) -> bool {
        self.lag_commits == 0
            && self.files_drift == 0
            && self.cases_drift == 0
            && self.logs_drift == 0
            && self.uncovered.is_empty()
            && self.tag_conflicts == 0
            && self.tag_unplaced == 0
    }
}

#[cfg(test)]
mod project_4199 {
    use super::*;
    fn s(v: &[&str]) -> Vec<String> {
        v.iter().map(|x| x.to_string()).collect()
    }

    #[test]
    fn no_kind_files_are_named_by_extension_most_first() {
        let (n, top) = no_kind_summary(&s(&["a.png", "b.png", "c.lock", "Makefile", "d.png"]));
        assert_eq!(n, 5);
        assert_eq!(top, "png 3, lock 1, none 1");
    }

    #[test]
    fn plist_log_paths_are_read_from_both_keys_once() {
        let p = "<dict><key>StandardOutPath</key><string>/x/a.log</string><key>StandardErrorPath</key><string>/x/a.log</string><key>Label</key><string>com.x</string></dict>";
        assert_eq!(log_paths_in_plist(p), vec!["/x/a.log".to_string()]);
        assert!(log_paths_in_plist("<dict></dict>").is_empty());
    }

    #[test]
    fn logs_that_match_reconcile_clean() {
        let on_disk = |p: &str| p == "/l/a.log" || p == "/l/b.log" || p == "/w/worker.log";
        let d = reconcile_logs(
            &s(&["/l/a.log", "/l/b.log"]),
            &s(&["/l/b.log", "/l/a.log", "/w/worker.log"]),
            &on_disk,
        );
        assert!(
            d.is_clean(),
            "a row outside the swept directories whose file exists is not drift: {}",
            d.report()
        );
    }
    // NEGATIVE PROOF (#3734): a log file with no row is red and named; so is a row with no file.
    #[test]
    fn negative_proof_a_log_file_without_a_row_is_red_and_named() {
        let on_disk = |p: &str| p == "/l/a.log" || p == "/l/new.log";
        let d = reconcile_logs(
            &s(&["/l/a.log", "/l/new.log"]),
            &s(&["/l/a.log", "/l/gone.log"]),
            &on_disk,
        );
        assert!(!d.is_clean());
        assert!(d.report().contains("/l/new.log"), "{}", d.report());
        assert!(d.report().contains("/l/gone.log"), "{}", d.report());
    }

    #[test]
    fn watermarks_are_read_from_a_crawl_log_in_order() {
        let log = "chorus-crawl: full\nchorus-crawl: watermark -> aaa111\nchorus-crawl: delta\nchorus-crawl: watermark HELD — x\nchorus-crawl: watermark -> bbb222\n";
        assert_eq!(passes_watermarks(log), s(&["aaa111", "bbb222"]));
    }

    // NEGATIVE PROOF (#3734): a land after the last pass is uncovered and named;
    // a land an earlier or equal watermark descends from is covered.
    #[test]
    fn negative_proof_a_land_no_pass_walked_is_uncovered() {
        // linear history: L1 -> W1 -> L2 -> L3 ; passes recorded W1 and L2
        let order = ["L1", "W1", "L2", "L3"];
        let anc = |a: &str, b: &str| {
            let ia = order.iter().position(|x| *x == a);
            let ib = order.iter().position(|x| *x == b);
            matches!((ia, ib), (Some(x), Some(y)) if x < y)
        };
        let missing = uncovered_lands(&s(&["L1", "L2", "L3"]), &s(&["W1", "L2"]), &anc);
        assert_eq!(missing, s(&["L3"]));
        assert!(
            uncovered_lands(&s(&["L1", "L2"]), &s(&["W1", "L2"]), &anc).is_empty(),
            "control: both covered"
        );
    }

    #[test]
    fn the_morning_line_says_clean_or_names_the_red() {
        let mut p = ProjectLine {
            file_rows: 5575,
            tracked: 6214,
            no_kind: 642,
            no_kind_top: "png 233, none 180".into(),
            case_rows: 8236,
            no_case: 2,
            no_case_detail: "unextracted 2 · no-lane 3".into(),
            log_rows: 90,
            log_files: 40,
            lag_commits: 0,
            files_drift: 0,
            cases_drift: 0,
            logs_drift: 0,
            lands: 3,
            uncovered: vec![],
            tag_placed: 1000,
            tag_conflicts: 0,
            tag_unplaced: 0,
        };
        assert!(p.is_clean());
        // #4201 — an unplaced test file is not clean, and the line says so
        let mut q = p.clone();
        q.tag_unplaced = 1;
        assert!(!q.is_clean());
        assert!(q.render().contains("tagged placed=1000 conflicts=0 unplaced=1"), "{}", q.render());
        let line = p.render();
        assert!(
            line.contains("complete files=5575/6214 no-kind=642 (png 233, none 180)"),
            "{line}"
        );
        assert!(line.contains("lossless lands=3 all covered"), "{line}");
        p.lag_commits = 2;
        p.uncovered = s(&["abc1234"]);
        assert!(!p.is_clean());
        assert!(p.render().contains("current lag=2"));
        assert!(p.render().contains("LOSSY lands=3 uncovered=1 (abc1234)"));
    }
}

/// #4178 — the identity a scheduled run must present.
///
/// The door stamps `ownedBy` from the caller, and only the owner may update or
/// delete a row. So whoever runs the crawler TAKES every row it creates: a
/// hand-run by a person silently reassigns ownership away from the automation,
/// and the next scheduled pass is then refused on those rows. Measured
/// 2026-09-15 on the variant: one pass run as `wren` left {crawler 5547,
/// wren 1}.
///
/// The scheduled identity is therefore not a detail of the plist — it is the
/// thing that decides whether tomorrow's run can write at all.
pub const SCHEDULED_ROLE: &str = "crawler";

/// Is this run allowed to be the scheduled one? A scheduled pass must present
/// the automation identity; anything else is a hand-run, which is fine to do
/// and must never be wired to a timer.
pub fn scheduled_identity_ok(role: &str) -> bool {
    role == SCHEDULED_ROLE
}

#[cfg(test)]
mod merge_4178 {
    use super::*;

    // ─────────────── #4178: an update must not delete other writers' fields ───────────────

    const PAGE: &str = r#"{ "kind": "CodeFile", "data": [
  { "name": "file-a", "filePath": "a/b.rs", "fileSha": "aaa", "hasKind": "code-kind-code",
    "hasLanguage": "language-rust", "fileInDomain": "code-domain", "fileHasOwner": "role-kade",
    "stale": "", "label": "file-a" },
  { "name": "file-c", "filePath": "c/d{e}.md", "fileSha": "ccc", "hasKind": "code-kind-doc" }
], "links": { "next": "" }, "count": 2 }"#;

    #[test]
    fn a_row_is_read_whole_not_two_strings() {
        let objs = row_objects(PAGE);
        assert_eq!(
            objs.len(),
            2,
            "a brace inside a value must not open a fake object"
        );
        let f = row_fields(objs[0]);
        let get = |k: &str| f.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
        assert_eq!(get("filePath"), Some("a/b.rs"));
        assert_eq!(get("fileInDomain"), Some("code-domain"));
        assert_eq!(get("fileHasOwner"), Some("role-kade"));
        // the second row's path contains braces — the scanner must still split it out
        assert_eq!(
            row_fields(objs[1])
                .iter()
                .find(|(k, _)| k == "filePath")
                .map(|(_, v)| v.as_str()),
            Some("c/d{e}.md")
        );
    }

    #[test]
    fn an_update_carries_the_other_writers_fields_through() {
        let existing = row_fields(row_objects(PAGE)[0]);
        let owned = vec![
            ("filePath".to_string(), "a/b.rs".to_string()),
            ("fileSha".to_string(), "bbb".to_string()), // the content changed
            ("hasKind".to_string(), "code".to_string()),
        ];
        let merged = merge_row(&existing, &owned);
        let get = |k: &str| merged.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str());
        assert_eq!(get("fileSha"), Some("bbb"), "the crawler's own field wins");
        assert_eq!(get("hasKind"), Some("code"));
        assert_eq!(
            get("fileInDomain"),
            Some("code-domain"),
            "the domain tag survives"
        );
        assert_eq!(get("fileHasOwner"), Some("role-kade"));
        assert_eq!(
            get("hasLanguage"),
            Some("language-rust"),
            "a field nobody restated survives"
        );
        // server-managed keys are never restated
        assert_eq!(get("name"), None);
        assert_eq!(get("label"), None);
        // and an empty field is not sent back as an empty string
        assert_eq!(get("stale"), None);
    }

    // NEGATIVE PROOF (#3734): the whole card is that a tag SURVIVES an update.
    // A merge that quietly dropped the existing row would pass every assertion
    // above about the crawler's own fields, so the check that matters is the
    // one where the graph's fields are absent from the payload — it must fail.
    #[test]
    fn negative_proof_restating_only_the_crawlers_own_fields_loses_the_domain_tag() {
        let existing = row_fields(row_objects(PAGE)[0]);
        let owned = vec![
            ("filePath".to_string(), "a/b.rs".to_string()),
            ("fileSha".to_string(), "bbb".to_string()),
            ("hasKind".to_string(), "code".to_string()),
        ];
        // what the crawler did BEFORE this card: own fields only, no merge.
        let unmerged = owned.clone();
        assert!(
            !unmerged.iter().any(|(k, _)| k == "fileInDomain"),
            "the pre-#4178 payload carries no domain tag — this is the state the merge exists to prevent"
        );
        // and the merge is what separates the two states
        let merged = merge_row(&existing, &owned);
        assert!(merged
            .iter()
            .any(|(k, v)| k == "fileInDomain" && v == "code-domain"));
        assert_ne!(merged.len(), unmerged.len());
    }

    #[test]
    fn a_refusal_that_names_its_prefix_is_retried_bare() {
        let err = "422 { \"error\": \"validation\", \"message\": \"athena-model: double-prefix: 'code-kind-doc' already starts with 'code-kind-' — pass the bare name\" }";
        let mut fields = vec![
            ("hasKind".to_string(), "code-kind-doc".to_string()),
            ("filePath".to_string(), "x.md".to_string()),
        ];
        assert!(strip_named_prefix(err, &mut fields));
        assert_eq!(fields[0].1, "doc");
        assert_eq!(fields[1].1, "x.md", "an unrelated field is untouched");
    }

    // NEGATIVE PROOF: the retry must be driven by the server's words, not by a
    // guess. An error that names no prefix changes nothing, and a value that IS
    // the prefix is not stripped to empty.
    #[test]
    fn negative_proof_an_unnamed_prefix_changes_nothing_and_a_bare_value_is_not_emptied() {
        let mut fields = vec![("hasKind".to_string(), "code-kind-doc".to_string())];
        assert!(!strip_named_prefix("502 dal unavailable", &mut fields));
        assert_eq!(fields[0].1, "code-kind-doc");

        let mut exact = vec![("hasKind".to_string(), "code-kind-".to_string())];
        assert!(!strip_named_prefix(
            "already starts with 'code-kind-'",
            &mut exact
        ));
        assert_eq!(exact[0].1, "code-kind-");
    }

    #[test]
    fn the_scheduled_run_presents_the_automation_identity() {
        assert!(scheduled_identity_ok("crawler"));
        assert_eq!(SCHEDULED_ROLE, "crawler");
    }

    // NEGATIVE PROOF (#3734): the check exists to separate a scheduled pass from
    // a hand-run. If it admitted a person's identity it would pass on the very
    // state it is meant to catch — the one that reassigns ownership and locks
    // tomorrow's run out of the rows it just took.
    #[test]
    fn negative_proof_a_person_is_not_a_scheduled_identity() {
        for who in ["kade", "wren", "silas", "jeff", ""] {
            assert!(
                !scheduled_identity_ok(who),
                "{who} must not be wired to a timer"
            );
        }
    }
}

// ── #4199 — the log leg: every log file the box writes is a LogSource row ────
//
// Jeff, 2026-09-17 11:51: "the core crawler does the log writes". The crawler
// already walks the plists and the log directories to judge the drift; the same
// walk now writes the rows. Identity is the file's path. Row names are door
// names (`log-<slug>-<fnv8>`); the old harvester's `urn:` ids cannot be
// addressed through the door at all, so the crawler never touches them — they
// show as drift until a DBA retires them, and that is the honest reading.

/// The `Label` of a launchd plist, if it has one.
pub fn plist_label(text: &str) -> Option<String> {
    let needle = "<key>Label</key>";
    let at = text.find(needle)? + needle.len();
    let rest = &text[at..];
    let s = rest.find("<string>")? + 8;
    let e = rest[s..].find("</string>")?;
    let label = rest[s..s + e].trim();
    (!label.is_empty()).then(|| label.to_string())
}

/// One log file on this box, as the walk found it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogFile {
    pub path: String,
    /// The launchd job whose plist names this file, when one does.
    pub launchd_label: Option<String>,
    pub size: u64,
    /// mtime, seconds since the epoch
    pub written_secs: u64,
}

/// #4222 — the domain a log belongs to, read from the job that writes it and
/// the name of the file itself. In that order: the launchd label is a fact the
/// box asserts, the filename is what the author called it, and neither is the
/// folder.
///
/// The word list is the API's own vocabulary for a domain — `werk` is the cicd
/// surface, `nudge` is messages, `fuseki` is infrastructure. Measured against
/// the live 133 rows on 2026-09-19: label and name together place 49. The rest
/// stay unplaced and are reported; `UnitDomainMapping` is the authored answer
/// for those, and it covers 6 today.
pub fn log_domain(
    label: &str,
    path: &str,
    domains: &[String],
    authored: &[(String, String)],
) -> Option<String> {
    const WORD: &[(&str, &str)] = &[
        ("werk", "cicd"),
        ("crawl", "code"),
        ("embed", "search"),
        ("reindex", "search"),
        ("index", "search"),
        ("eventloop", "monitors"),
        ("heartbeat", "monitors"),
        ("watchdog", "monitors"),
        ("health", "monitors"),
        ("clearing", "messages"),
        ("nudge", "messages"),
        ("bridge", "messages"),
        ("fuseki", "infrastructure"),
        ("backup", "infrastructure"),
        ("athena", "knowledge"),
        ("hooks", "spine"),
        ("pulse", "spine"),
        ("bdd", "tests"),
        ("test", "tests"),
        ("deploy", "deploys"),
        ("oidc", "identity"),
        ("harvest", "services"),
        ("alert", "alerts"),
    ];
    let file = path.rsplit('/').next().unwrap_or(path);
    // #4222 — the AUTHORED rows in roles/kade/ontology/surface-domain-4222.ttl:
    // the 72 log files whose name carries no word any rule knows (`caddy.log`,
    // `tm-thin.log`, `css.log`). Checked FIRST, because an authored assignment
    // beats an incidental word match — `log-rotate.log` is logs, not a rotate
    // rule. No row, no domain: the source reports unplaced, never a default.
    if let Some((_, d)) = authored.iter().find(|(k, _)| k == file) {
        if let Some(hit) = domains.iter().find(|x| *x == d) {
            return Some(hit.clone());
        }
    }
    let words: Vec<String> = format!("{label} {file}")
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_ascii_lowercase())
        .collect();
    for w in &words {
        if let Some(d) = domains.iter().find(|d| d.as_str() == w) {
            return Some(d.clone());
        }
        if let Some((_, mapped)) = WORD.iter().find(|(k, _)| k == w) {
            if let Some(d) = domains.iter().find(|d| d.as_str() == *mapped) {
                return Some(d.clone());
            }
        }
    }
    None
}

/// A log file with no launchd job behind it: a script's or a service's own.
pub const UNMANAGED: &str = "unmanaged";
/// A log nobody wrote to for this long is silent, not active.
pub const SILENT_AFTER_SECS: u64 = 7 * 86_400;

/// The row the crawler owns for one log file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogRow {
    pub path: String,
    pub launchd_label: String,
    pub size: u64,
    pub written_secs: u64,
}

impl LogRow {
    pub fn from_file(f: &LogFile) -> LogRow {
        LogRow {
            path: f.path.clone(),
            launchd_label: f.launchd_label.clone().unwrap_or_else(|| UNMANAGED.to_string()),
            size: f.size,
            written_secs: f.written_secs,
        }
    }
    pub fn status_at(&self, observed_secs: u64) -> &'static str {
        if observed_secs.saturating_sub(self.written_secs) > SILENT_AFTER_SECS {
            "silent"
        } else {
            "active"
        }
    }
    /// The fields the crawler asserts. `machine` is the bare onMachine value;
    /// the door's own prefix is learned on the first refusal (PrefixMemory).
    pub fn owned_fields(&self, machine: &str, observed_secs: u64) -> Vec<(String, String)> {
        let base = self.path.rsplit('/').next().unwrap_or(&self.path);
        vec![
            ("label".to_string(), format!("{base} ({machine})")),
            ("logPath".to_string(), self.path.clone()),
            ("launchdLabel".to_string(), self.launchd_label.clone()),
            ("logStatus".to_string(), self.status_at(observed_secs).to_string()),
            ("lastObserved".to_string(), iso_from_secs(observed_secs)),
            ("lastWrittenAt".to_string(), iso_from_secs(self.written_secs)),
            ("sizeBytes".to_string(), self.size.to_string()),
            ("onMachine".to_string(), machine.to_string()),
        ]
    }
}

/// A LogSource row as the door serves it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogInGraph {
    pub name: String,
    pub path: String,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogAction {
    Post(LogRow),
    Replace { name: String, row: LogRow },
    Unchanged { path: String },
    Delete { name: String, path: String },
}

/// A name the door will address: its own minted shape, not a `urn:` id.
pub fn door_addressable(name: &str) -> bool {
    !name.is_empty() && !name.contains(':') && !name.contains('/')
}

/// Deterministic door name for a log file: slug of the path + fnv8 of the exact path.
pub fn log_row_name(path: &str) -> String {
    let mut slug = String::new();
    let mut last_dash = false;
    for c in path.chars() {
        if c.is_ascii_alphanumeric() {
            slug.push(c.to_ascii_lowercase());
            last_dash = false;
        } else if !last_dash {
            slug.push('-');
            last_dash = true;
        }
    }
    let slug = slug.trim_matches('-');
    let mut end = slug.len().min(100);
    while !slug.is_char_boundary(end) {
        end -= 1;
    }
    let slug = slug[..end].trim_end_matches('-');
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in path.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("log-{slug}-{:08x}", (h & 0xffff_ffff) as u32)
}

/// What one pass does to the logs domain. A full pass refreshes every row it
/// owns (size, last write, status); a delta only adds and retires. Rows the
/// door cannot address are never planned against.
pub fn plan_logs(
    files: &[LogFile],
    rows: &[LogInGraph],
    full: bool,
    exists: &dyn Fn(&str) -> bool,
) -> Vec<LogAction> {
    let mut out = Vec::new();
    for f in files {
        let row = LogRow::from_file(f);
        match rows.iter().find(|r| r.path == f.path && door_addressable(&r.name)) {
            None => out.push(LogAction::Post(row)),
            Some(r) if full => out.push(LogAction::Replace { name: r.name.clone(), row }),
            Some(_) => out.push(LogAction::Unchanged { path: f.path.clone() }),
        }
    }
    for r in rows {
        if !r.path.is_empty() && door_addressable(&r.name) && !exists(&r.path) {
            out.push(LogAction::Delete { name: r.name.clone(), path: r.path.clone() });
        }
    }
    out
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct LogCounts {
    pub posted: usize,
    pub replaced: usize,
    pub unchanged: usize,
    pub deleted: usize,
}

pub fn log_counts(actions: &[LogAction]) -> LogCounts {
    let mut c = LogCounts::default();
    for a in actions {
        match a {
            LogAction::Post(_) => c.posted += 1,
            LogAction::Replace { .. } => c.replaced += 1,
            LogAction::Unchanged { .. } => c.unchanged += 1,
            LogAction::Delete { .. } => c.deleted += 1,
        }
    }
    c
}

/// Seconds since the epoch as `YYYY-MM-DDTHH:MM:SSZ`, no clock library.
pub fn iso_from_secs(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

#[cfg(test)]
mod logs_4199 {
    use super::*;

    fn file(path: &str, label: Option<&str>, written: u64) -> LogFile {
        LogFile {
            path: path.to_string(),
            launchd_label: label.map(|s| s.to_string()),
            size: 10,
            written_secs: written,
        }
    }
    fn row(name: &str, path: &str) -> LogInGraph {
        LogInGraph {
            name: name.to_string(),
            path: path.to_string(),
            fields: vec![],
        }
    }

    #[test]
    fn plist_label_reads_the_label_string() {
        let t = "<dict><key>Label</key>\n<string>com.chorus.x</string><key>StandardOutPath</key><string>/l.log</string></dict>";
        assert_eq!(plist_label(t).as_deref(), Some("com.chorus.x"));
        assert_eq!(plist_label("<dict></dict>"), None);
    }

    #[test]
    fn iso_from_secs_is_civil_utc() {
        assert_eq!(iso_from_secs(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_from_secs(1_700_000_000), "2023-11-14T22:13:20Z");
        assert_eq!(iso_from_secs(951_782_400), "2000-02-29T00:00:00Z");
    }

    #[test]
    fn row_name_is_a_door_name_and_distinguishes_case() {
        let a = log_row_name("/Users/j/Library/Logs/Chorus/a.log");
        let b = log_row_name("/Users/j/Library/Logs/Chorus/A.log");
        assert!(a.starts_with("log-users-j-library-logs-chorus-a-log-"));
        assert_ne!(a, b);
        assert!(door_addressable(&a));
        assert!(!door_addressable("urn:chorus:logsource-library-x"));
    }

    #[test]
    fn a_file_with_no_row_is_posted_even_when_a_urn_row_names_it() {
        let files = [file("/l/a.log", Some("com.a"), 100)];
        let rows = [row("urn:chorus:logsource-library-com.a", "/l/a.log")];
        let plan = plan_logs(&files, &rows, true, &|_| true);
        assert_eq!(plan.len(), 1);
        assert!(matches!(&plan[0], LogAction::Post(r) if r.launchd_label == "com.a"));
    }

    #[test]
    fn a_present_row_is_refreshed_on_full_and_left_on_delta() {
        let files = [file("/l/a.log", None, 100)];
        let rows = [row("log-l-a-log-00000001", "/l/a.log")];
        let full = plan_logs(&files, &rows, true, &|_| true);
        assert!(matches!(&full[0], LogAction::Replace { name, row } if name == "log-l-a-log-00000001" && row.launchd_label == UNMANAGED));
        let delta = plan_logs(&files, &rows, false, &|_| true);
        assert!(matches!(&delta[0], LogAction::Unchanged { .. }));
    }

    #[test]
    fn a_door_row_whose_file_is_gone_is_deleted_and_a_urn_row_never_is() {
        let rows = [
            row("log-l-gone-log-00000002", "/l/gone.log"),
            row("urn:chorus:logsource-library-gone", "/l/gone2.log"),
        ];
        let plan = plan_logs(&[], &rows, true, &|_| false);
        assert_eq!(plan.len(), 1);
        assert!(matches!(&plan[0], LogAction::Delete { name, .. } if name == "log-l-gone-log-00000002"));
    }

    #[test]
    fn negative_proof_a_present_file_is_not_deleted() {
        let files = [file("/l/a.log", None, 100)];
        let rows = [row("log-l-a-log-00000001", "/l/a.log")];
        let plan = plan_logs(&files, &rows, false, &|p| p == "/l/a.log");
        assert_eq!(log_counts(&plan).deleted, 0);
    }

    #[test]
    fn status_is_silent_after_a_week() {
        let r = LogRow::from_file(&file("/l/a.log", None, 1_000_000));
        assert_eq!(r.status_at(1_000_000 + SILENT_AFTER_SECS), "active");
        assert_eq!(r.status_at(1_000_000 + SILENT_AFTER_SECS + 1), "silent");
        let f = r.owned_fields("library", 1_000_100);
        assert!(f.contains(&("label".to_string(), "a.log (library)".to_string())));
        assert!(f.contains(&("onMachine".to_string(), "library".to_string())));
        assert!(f.contains(&("launchdLabel".to_string(), UNMANAGED.to_string())));
    }
}

#[cfg(test)]
mod plan_domain_4201 {
    use super::{plan, plan_with, Action, InGraph, OnDisk, TreeRead};

    fn disk(path: &str, sha: &str) -> OnDisk {
        OnDisk { path: path.to_string(), sha: sha.to_string(), classified: true }
    }
    fn row(path: &str, sha: &str, domain: &str) -> InGraph {
        InGraph {
            path: path.to_string(),
            sha: sha.to_string(),
            other: vec![("hasDomain".to_string(), domain.to_string())],
        }
    }

    /// NEGATIVE PROOF: a row whose sha has not moved but whose domain is wrong.
    /// plan() can only see content, so tagging hasDomain across a settled tree
    /// produced 30 replaces out of 6,226 rows — the other 1,563 were eligible
    /// for nothing and would have stayed blank forever.
    #[test]
    fn a_stale_domain_makes_a_row_eligible_even_when_the_sha_stands_still() {
        let d = vec![disk("a.ts", "SAME")];
        let g = vec![row("a.ts", "SAME", "")];

        // the guarded condition: sha-only planning calls it Unchanged
        assert!(matches!(
            plan(&d, &g, TreeRead::Complete).as_slice(),
            [Action::Unchanged { .. }]
        ));

        // and with the domain question asked, the same row is a Replace
        let acts = plan_with(&d, &g, TreeRead::Complete, &|_, g| {
            g.other.iter().any(|(k, v)| k == "hasDomain" && v.is_empty())
        });
        assert!(matches!(acts.as_slice(), [Action::Replace { .. }]), "{acts:?}");
    }

    /// CONTROL: a row that already holds the right domain stays Unchanged —
    /// otherwise every pass would rewrite the whole tree.
    #[test]
    fn a_row_whose_domain_is_already_right_is_not_restated() {
        let d = vec![disk("a.ts", "SAME")];
        let g = vec![row("a.ts", "SAME", "cards")];
        let acts = plan_with(&d, &g, TreeRead::Complete, &|_, g| {
            g.other.iter().any(|(k, v)| k == "hasDomain" && v.is_empty())
        });
        assert!(matches!(acts.as_slice(), [Action::Unchanged { .. }]), "{acts:?}");
    }
    /// #4222 — every log file the hand table claims, placed. The fixture is the
    /// 72 sources that carried no domain on 2026-09-19; rename one and it drops
    /// out of the table, so this goes red rather than keeping a stale answer.
    #[test]
    fn every_hand_assigned_log_file_places() {
        let domains = log_domains_fixture();
        let authored = authored_logs();
        for (file, want) in &authored {
            assert_eq!(
                super::log_domain("", &format!("/var/log/{file}"), &domains, &authored).as_deref(),
                Some(want.as_str()),
                "{file} must place in {want}"
            );
        }
    }

    /// NEGATIVE PROOF. A log file no rule reaches stays unplaced and is named.
    #[test]
    fn a_log_file_no_rule_reaches_stays_unplaced() {
        let domains = log_domains_fixture();
        assert_eq!(super::log_domain("", "/var/log/zzz-nothing.log", &log_domains_fixture(), &[]), None);
        assert_eq!(super::log_domain("", "/var/log/quux.out", &log_domains_fixture(), &[]), None);
    }

    fn authored_logs() -> Vec<(String, String)> {
        let ttl = std::fs::read_to_string(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../roles/kade/ontology/surface-domain-4222.ttl"
        ))
        .expect("the authored surface rows must exist");
        let rows = crate::domain::surface_domain_rows(&ttl, "chorus:logFileName");
        assert!(!rows.is_empty(), "the authored file must carry log rows");
        rows
    }

    fn log_domains_fixture() -> Vec<String> {
        [
            "alerts", "builds", "cards", "cicd", "code", "domains", "heralds", "identity",
            "infrastructure", "integrations", "knowledge", "logs", "memory", "messages",
            "metrics", "monitors", "practices", "products", "roles", "search", "security",
            "services", "spine", "tests", "toolchain",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

}
