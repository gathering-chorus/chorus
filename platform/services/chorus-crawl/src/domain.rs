//! #4201 — which domain does a test file exercise?
//!
//! Jeff, 2026-09-17 13:34: "there are tests that test the services domain or
//! the domains domain, the rest must be tagged to the actual domain it is part
//! of." The folder a test sits in is never the answer. Five rules read the
//! FILE, in order; every rule that fires must agree, or the file is a conflict
//! and gets no tag (Silas and Wren, 13:48). A file no rule places is unplaced
//! and gets no tag. Both are listed by name, never silently placed.
//!
//! Everything here is pure: text in, placement out. The caller supplies the
//! set of real Domain rows (only those can be a tag) and the card lookup.

/// The five rules, in precedence order. Precedence is for reporting only —
/// agreement is required, so no rule "wins".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    Route,
    Binary,
    Class,
    Module,
    Unit,
    Card,
}

impl Rule {
    pub fn label(self) -> &'static str {
        match self {
            Rule::Route => "route",
            Rule::Binary => "binary",
            Rule::Class => "class",
            Rule::Module => "module",
            Rule::Unit => "unit",
            Rule::Card => "card",
        }
    }
}

/// One rule's reading of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signal {
    pub rule: Rule,
    pub domain: String,
    /// The text that fired it — what a reader sees on the conflict line.
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Placement {
    /// Every rule that fired agreed.
    Tagged { domain: String, signals: Vec<Signal> },
    /// Two or more rules named different domains. No tag.
    Conflict { signals: Vec<Signal> },
    /// No rule fired. No tag.
    Unplaced,
}

impl Placement {
    pub fn domain(&self) -> Option<&str> {
        match self {
            Placement::Tagged { domain, .. } => Some(domain),
            _ => None,
        }
    }
}

// ── rule 1: the API route the file calls ─────────────────────────────────────
//
// `/api/chorus/<segment>` and the athena/owl surfaces. Longest prefix first.
const ROUTES: &[(&str, &str)] = &[
    ("/api/chorus/context/priorities", "board"),
    ("/api/chorus/context/board", "board"),
    ("/api/chorus/context/spine", "spine"),
    ("/api/chorus/context/roles", "roles"),
    ("/api/chorus/context/alerts", "alerts"),
    ("/api/chorus/context/health", "monitors"),
    ("/api/chorus/knowledge", "knowledge"),
    ("/api/chorus/decisions", "decisions"),
    ("/api/chorus/principles", "principles"),
    ("/api/chorus/practices", "practices"),
    ("/api/chorus/freshness", "spine"),
    ("/api/chorus/trace", "spine"),
    ("/api/chorus/spine", "spine"),
    ("/api/chorus/search", "search"),
    ("/api/chorus/embed", "search"),
    ("/api/chorus/cards", "cards"),
    ("/api/chorus/card/", "cards"),
    ("/api/chorus/nudge", "messages"),
    ("/api/chorus/messages", "messages"),
    ("/api/chorus/notify", "messages"),
    ("/api/chorus/quality", "tests"),
    ("/api/chorus/tests", "tests"),
    ("/api/chorus/cost", "analytics"),
    ("/api/chorus/patterns", "analytics"),
    ("/api/chorus/pain", "analytics"),
    ("/api/chorus/flow", "analytics"),
    ("/api/chorus/jeff", "analytics"),
    ("/api/chorus/logs", "logs"),
    ("/api/chorus/rca", "rcas"),
    ("/api/chorus/security", "security"),
    ("/api/chorus/principals", "identity"),
    ("/api/chorus/identity", "identity"),
    ("/api/chorus/domain/", "domains"),
    ("/api/chorus/class-atlas", "domains"),
    ("/api/athena/", "domains"),
    ("/api/nudge", "messages"),
    ("/valuestreams", "value-streams"),
    ("/pipelines", "pipelines"),
    ("/products", "products"),
    ("/logs/sources", "logs"),
    ("/code/files", "code"),
    ("/tests/tests", "tests"),
    ("/owl/", "domains"),
];

// ── rule 2: the binary or script the file runs ───────────────────────────────
const BINARIES: &[(&str, &str)] = &[
    ("chorus-provision", "identity"),
    ("chorus-identity-token", "identity"),
    ("chorus-oidc", "identity"),
    ("werk-test", "tests"),
    ("nightly-suites", "tests"),
    ("chorus-crawl", "code"),
    ("crawl-nightly", "code"),
    ("werk-build", "builds"),
    ("build-signed", "builds"),
    ("werk-deploy", "deploys"),
    ("athena-deploy", "deploys"),
    ("chorus-bin-install", "deploys"),
    ("chorus-sdk-deploy", "deploys"),
    ("deploy-canonical", "deploys"),
    ("agent-state.sh", "deploys"),
    ("werk-merge", "cicd"),
    ("werk-accept", "cicd"),
    ("werk-pull", "cicd"),
    ("werk-commit", "cicd"),
    ("werk-push", "cicd"),
    ("werk-review", "cicd"),
    ("werk.yml", "cicd"),
    ("werk-resume", "cicd"),
    ("chorus_werk", "cicd"),
    ("deep-health", "monitors"),
    ("chorus-health", "monitors"),
    ("fuseki-backup", "infrastructure"),
    ("restore-drill", "infrastructure"),
    ("backup-dump", "infrastructure"),
    ("chorus-lance-maintain", "search"),
    ("chorus-rerank", "knowledge"),
    ("athena-validate", "knowledge"),
    ("gen-role-mcp", "toolchain"),
    ("chorus-awake", "roles"),
    ("pulse-gather", "roles"),
    ("loom-gemba", "roles"),
    ("git-queue", "version-control"),
    ("chorus-inject", "messages"),
    ("chorus-model", "domains"),
    ("athena-model", "domains"),
    ("athena-make", "domains"),
    ("service-harvest", "services"),
    ("service-drift", "services"),
    ("log-harvest", "logs"),
    ("authn-coverage", "security"),
    ("authz-coverage", "security"),
    ("security-distance", "security"),
    ("gitleaks", "security"),
];

// ── rule 3: the class or shape the file asserts ──────────────────────────────
const CLASSES: &[(&str, &str)] = &[
    ("RoleShape", "roles"),
    ("PrincipleShape", "principles"),
    ("PracticeShape", "practices"),
    ("ProductShape", "products"),
    ("ServiceInstance", "services"),
    ("ServiceShape", "services"),
    ("Commitment", "services"),
    ("PipelineRun", "pipelines"),
    ("PipelineStep", "pipelines"),
    ("PipelineShape", "pipelines"),
    ("ValueStreamStep", "value-streams"),
    ("ValueStreamShape", "value-streams"),
    ("acl:Authorization", "security"),
    ("acl:Write", "security"),
    ("chorus:Permission", "security"),
    ("PermissionShape", "security"),
    ("PrincipalShape", "identity"),
    ("chorus:Principal", "identity"),
    ("WebID", "identity"),
    ("CodeFileShape", "code"),
    ("chorus:CodeFile", "code"),
    ("TestResult", "tests"),
    ("TestSuiteRun", "tests"),
    ("chorus:Test ", "tests"),
    ("LogSource", "logs"),
    ("CardShape", "cards"),
    ("chorus:Card", "cards"),
    ("AlertShape", "alerts"),
    ("chorus:Alert", "alerts"),
    ("MonitorShape", "monitors"),
    ("chorus:Monitor", "monitors"),
    ("DecisionShape", "decisions"),
    ("DomainShape", "domains"),
    ("SubDomain", "domains"),
    ("chorus:Domain", "domains"),
];

// ── rule 4: the module the file imports ──────────────────────────────────────
//
// Matched against import / require / use lines only, so a word in a comment
// never fires it.
/// #4201 — the unit a source file DECLARES it belongs to, and that unit's
/// domain. This rule exists for the file shape the other four cannot read: a
/// crate source file carrying its own `#[cfg(test)] mod tests`. Such a file
/// imports nothing external because it IS the unit, so route/binary/class/
/// module all stay silent — 89 chorus-hooks files and 14 more sat on the
/// `tests` fallback for exactly this reason (measured 2026-09-17).
///
/// The name is read from the unit's own manifest (`Cargo.toml` / `package.json`
/// `name`), never from the directory it happens to sit in: a renamed folder
/// keeps the unit's identity, a renamed package changes it, which is the
/// difference the card's title asks for.
///
/// Placements here are Jeff's rulings, not inferences:
///   hooks and injection are the spine's runtime  (2026-09-03: "hooks + mcp =
///   spine/events; the hook is the grain"), and the werk verbs are the build
///   lane's own records (the builds domain: verb runs, werk slots, demo
///   verdicts, gate results).
const UNITS: &[(&str, &str)] = &[
    ("chorus-hooks", "spine"),
    ("chorus-inject", "spine"),
    ("chorus-awake", "spine"),
    ("werk-", "builds"),
    ("properties-resolver", "Properties"),
];

/// The unit name declared by the nearest manifest above `path`, if any.
pub fn declared_unit(path: &str, read: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let mut dir: Vec<&str> = path.split('/').collect();
    dir.pop();
    while !dir.is_empty() {
        let base = dir.join("/");
        if let Some(t) = read(&format!("{base}/Cargo.toml")) {
            if let Some(n) = manifest_name(&t, "name = ") {
                return Some(n);
            }
        }
        if let Some(t) = read(&format!("{base}/package.json")) {
            if let Some(n) = manifest_name(&t, "\"name\": ") {
                return Some(n);
            }
        }
        dir.pop();
    }
    None
}

fn manifest_name(text: &str, key: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|l| l.starts_with(key))
        .and_then(|l| l.split(&['"', '\''][..]).nth(1))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
}

const MODULES: &[(&str, &str)] = &[
    ("logs-query", "logs"),
    ("log-reader", "logs"),
    ("class-atlas", "domains"),
    ("chorus-domain", "domains"),
    ("domain-renderer", "domains"),
    ("athena-", "domains"),
    ("embed", "search"),
    ("lance-store", "search"),
    ("security-", "security"),
    ("es256", "identity"),
    ("token", "identity"),
    ("time-utils", "time"),
    ("board-cache", "board"),
    ("discover-", "code"),
    ("ui-pages", "code"),
    ("code-inventory", "code"),
    ("-summary", "analytics"),
    ("cost", "analytics"),
    ("patterns", "analytics"),
    ("pain", "analytics"),
    ("flow-report", "analytics"),
    ("spine", "spine"),
    ("freshness", "spine"),
    ("nudge", "messages"),
    ("word-cap", "messages"),
    ("rca", "rcas"),
    ("icd", "integrations"),
    ("cards", "cards"),
    ("search", "search"),
];

fn one_domain(signals: &[Signal]) -> Option<String> {
    let first = signals.first()?.domain.clone();
    signals
        .iter()
        .all(|s| s.domain == first)
        .then_some(first)
}

fn fire(rule: Rule, table: &[(&str, &str)], hay: &str, valid: &[String]) -> Option<Signal> {
    let mut hits: Vec<Signal> = Vec::new();
    for (needle, dom) in table.iter().copied() {
        if hay.contains(needle)
            && valid.iter().any(|v| v == dom)
            && !hits.iter().any(|h| h.domain == dom)
        {
            hits.push(Signal {
                rule,
                domain: dom.to_string(),
                evidence: needle.to_string(),
            });
        }
    }
    match hits.len() {
        0 => None,
        1 => hits.pop(),
        _ => {
            // one rule, several domains: the rule itself is in conflict — keep
            // every hit so the line can name them
            let ev = hits
                .iter()
                .map(|h| format!("{}→{}", h.evidence, h.domain))
                .collect::<Vec<_>>()
                .join(", ");
            Some(Signal {
                rule,
                domain: String::new(),
                evidence: ev,
            })
        }
    }
}

/// Lines that import something: TS/JS `import`/`require`, Rust `use`, shell
/// `source`/`.`.
fn import_lines(content: &str) -> String {
    content
        .lines()
        .map(str::trim_start)
        .filter(|l| {
            l.starts_with("import ")
                || l.contains("require(")
                || l.starts_with("use ")
                || l.starts_with("source ")
                || l.starts_with(". ")
        })
        .collect::<Vec<_>>()
        .join("\n")
        // #4201 — a Rust crate imports as `use athena_make::…` while the module
        // table (and every path, package and binary name in this repo) spells
        // the same unit `athena-make`. Underscores are the crate-name spelling
        // of the hyphen, so the table never matched a single `use` line: 109 of
        // the 503 unplaced files on 2026-09-17 were service tests whose only
        // signal was the crate they exercise. Normalise the separator so one
        // table serves both spellings.
        .replace('_', "-")
}

/// Card numbers named in the file's first 20 lines (`#4201`).
pub fn header_cards(content: &str) -> Vec<u32> {
    let mut out = Vec::new();
    for line in content.lines().take(20) {
        let mut rest = line;
        while let Some(i) = rest.find('#') {
            let digits: String = rest[i + 1..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if digits.len() >= 3 && digits.len() <= 5 {
                if let Ok(n) = digits.parse::<u32>() {
                    if !out.contains(&n) {
                        out.push(n);
                    }
                }
            }
            rest = &rest[i + 1..];
        }
    }
    out
}

/// Place one test file. `valid` is the set of real Domain names; a rule hit
/// outside it is dropped. `card_domain` answers rule 5 for a card number, or
/// None when the card names no real domain.
pub fn place(
    content: &str,
    valid: &[String],
    card_domain: &dyn Fn(u32) -> Option<String>,
) -> Placement {
    place_in_unit(content, None, valid, card_domain)
}

/// `place`, plus the unit the file declares itself part of (rule 4b, #4201).
pub fn place_in_unit(
    content: &str,
    unit: Option<&str>,
    valid: &[String],
    card_domain: &dyn Fn(u32) -> Option<String>,
) -> Placement {
    let mut signals: Vec<Signal> = Vec::new();
    if let Some(s) = fire(Rule::Route, ROUTES, content, valid) {
        signals.push(s);
    }
    if let Some(s) = fire(Rule::Binary, BINARIES, content, valid) {
        signals.push(s);
    }
    if let Some(s) = fire(Rule::Class, CLASSES, content, valid) {
        signals.push(s);
    }
    let imports = import_lines(content);
    if let Some(s) = fire(Rule::Module, MODULES, &imports, valid) {
        signals.push(s);
    }
    // Only when nothing external was named: a file that imports a unit is
    // testing THAT unit, and must not be overridden by the crate it lives in.
    if signals.is_empty() {
        if let Some(u) = unit {
            if let Some(s) = fire(Rule::Unit, UNITS, u, valid) {
                signals.push(s);
            }
        }
    }
    for card in header_cards(content) {
        if let Some(d) = card_domain(card) {
            if valid.contains(&d) {
                signals.push(Signal {
                    rule: Rule::Card,
                    domain: d,
                    evidence: format!("#{card}"),
                });
                break;
            }
        }
    }
    if signals.is_empty() {
        return Placement::Unplaced;
    }
    if signals.iter().any(|s| s.domain.is_empty()) {
        return Placement::Conflict { signals };
    }
    match one_domain(&signals) {
        Some(domain) => Placement::Tagged { domain, signals },
        None => Placement::Conflict { signals },
    }
}

/// The counts Jeff reads: read · registry · placed · conflicts · unplaced.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TagCounts {
    pub read: usize,
    pub placed: usize,
    pub conflicts: usize,
    pub unplaced: usize,
}

impl TagCounts {
    pub fn render(&self) -> String {
        format!(
            "test files read={} placed={} conflicts={} unplaced={}",
            self.read, self.placed, self.conflicts, self.unplaced
        )
    }
}

/// One line per conflicted or unplaced file, so nothing is silently placed.
pub fn listing(path: &str, p: &Placement) -> Option<String> {
    match p {
        Placement::Tagged { .. } => None,
        Placement::Conflict { signals } => Some(format!(
            "conflict {path}: {}",
            signals
                .iter()
                .map(|s| {
                    if s.domain.is_empty() {
                        format!("{} [{}]", s.rule.label(), s.evidence)
                    } else {
                        format!("{}→{} ({})", s.rule.label(), s.domain, s.evidence)
                    }
                })
                .collect::<Vec<_>>()
                .join(" vs ")
        )),
        Placement::Unplaced => Some(format!("unplaced {path}: no rule fired")),
    }
}

#[cfg(test)]
mod tests_4201 {
    use super::*;

    fn valid() -> Vec<String> {
        [
            "cards", "search", "spine", "identity", "security", "tests", "code", "domains",
            "services", "deploys", "cicd", "logs", "board", "roles", "analytics",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }
    fn no_card(_: u32) -> Option<String> {
        None
    }

    #[test]
    fn route_alone_tags_the_file() {
        let c = "import request from 'supertest';\nit('lists', async () => { await request(app).get('/api/chorus/cards'); });";
        let p = place(c, &valid(), &no_card);
        assert_eq!(p.domain(), Some("cards"));
    }

    /// #4201 negative proof: the Rust spelling of a unit name. Before the
    /// separator was normalised this file fired NO rule — `use athena_make::`
    /// could not match the table's `athena-`, so every service test in the
    /// repo landed on the `tests` fallback.
    #[test]
    fn a_rust_crate_import_tags_the_file() {
        let c = "//! #3373 cors\nuse athena_make::http_response;\n#[test]\nfn t() {}";
        assert_eq!(place(c, &valid(), &no_card).domain(), Some("domains"));
    }

    /// Control: the hyphen spelling still tags, so the fix widened nothing.
    #[test]
    fn the_hyphen_spelling_still_tags() {
        let c = "import { q } from '../src/logs-query';";
        assert_eq!(place(c, &valid(), &no_card).domain(), Some("logs"));
    }

    /// Control: normalising the separator must not invent a signal where the
    /// file names no unit at all.
    #[test]
    fn an_unrelated_import_is_still_unplaced() {
        let c = "use std::collections::HashMap;\nimport fs from 'node:fs';";
        assert_eq!(place(c, &valid(), &no_card), Placement::Unplaced);
    }

    /// #4201 negative proof: a crate source file with inline tests. It imports
    /// only its own crate, so route/binary/class/module all stay silent — the
    /// shape that left 89 chorus-hooks files on the `tests` fallback. With the
    /// unit it declares, it places; without it, it is still unplaced.
    #[test]
    fn a_crate_source_file_places_by_its_declared_unit() {
        let c = "use crate::types::Payload;\n#[cfg(test)]\nmod tests {\n#[test] fn t() {}\n}";
        assert_eq!(place(c, &valid(), &no_card), Placement::Unplaced);
        assert_eq!(
            place_in_unit(c, Some("chorus-hooks"), &valid(), &no_card).domain(),
            Some("spine")
        );
    }

    /// Control: an imported unit still wins — a file that names what it tests
    /// is never overridden by the crate it happens to live in.
    #[test]
    fn an_imported_unit_beats_the_crate_it_lives_in() {
        let c = "use athena_make::http_response;";
        assert_eq!(
            place_in_unit(c, Some("chorus-hooks"), &valid(), &no_card).domain(),
            Some("domains")
        );
    }

    /// Control: an unknown unit invents nothing.
    #[test]
    fn an_unknown_unit_stays_unplaced() {
        let c = "use crate::types::Payload;";
        assert_eq!(
            place_in_unit(c, Some("some-new-crate"), &valid(), &no_card),
            Placement::Unplaced
        );
    }

    #[test]
    fn declared_unit_reads_the_manifest_not_the_folder() {
        let read = |p: &str| match p {
            "platform/services/renamed-dir/Cargo.toml" => {
                Some("[package]\nname = \"chorus-hooks\"\n".to_string())
            }
            _ => None,
        };
        assert_eq!(
            declared_unit("platform/services/renamed-dir/src/x.rs", &read).as_deref(),
            Some("chorus-hooks")
        );
        assert_eq!(declared_unit("platform/tests/x.bats", &|_| None), None);
    }

    #[test]
    fn binary_alone_tags_the_file() {
        let c = "#!/usr/bin/env bats\n# #3830 — provision a user\n@test 'x' { run chorus-provision create x; }";
        assert_eq!(place(c, &valid(), &no_card).domain(), Some("identity"));
    }

    #[test]
    fn agreeing_rules_tag_and_keep_every_signal() {
        let c = "import { readLogLines } from '../src/logs-query';\nrequest(app).get('/api/chorus/logs')";
        match place(c, &valid(), &no_card) {
            Placement::Tagged { domain, signals } => {
                assert_eq!(domain, "logs");
                assert_eq!(signals.len(), 2);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn negative_proof_route_says_cards_header_card_says_search_is_a_conflict_with_no_tag() {
        let c = "// #4321 — search ranking\nrequest(app).get('/api/chorus/cards')";
        let card = |n: u32| (n == 4321).then(|| "search".to_string());
        let p = place(c, &valid(), &card);
        assert_eq!(p.domain(), None);
        let line = listing("platform/api/tests/x.test.ts", &p).unwrap();
        assert!(line.starts_with("conflict "));
        assert!(line.contains("route→cards"));
        assert!(line.contains("card→search"));
    }

    #[test]
    fn negative_proof_card_numbered_bats_with_no_signal_is_unplaced_never_services() {
        let c = "#!/usr/bin/env bats\n# #4099 — something\n@test 'a' { [ 1 -eq 1 ]; }";
        let p = place(c, &valid(), &no_card);
        assert_eq!(p, Placement::Unplaced);
        assert_eq!(
            listing("platform/tests/4099-x.bats", &p).as_deref(),
            Some("unplaced platform/tests/4099-x.bats: no rule fired")
        );
    }

    #[test]
    fn one_rule_two_domains_is_a_conflict_named_on_the_line() {
        let c = "request(app).get('/api/chorus/cards'); request(app).get('/api/chorus/search');";
        let p = place(c, &valid(), &no_card);
        assert_eq!(p.domain(), None);
        let line = listing("f.ts", &p).unwrap();
        assert!(line.contains("route ["));
        assert!(line.contains("cards"));
        assert!(line.contains("search"));
    }

    #[test]
    fn a_domain_not_in_the_store_never_fires() {
        let c = "request(app).get('/api/chorus/cards')";
        let p = place(c, &["spine".to_string()], &no_card);
        assert_eq!(p, Placement::Unplaced);
    }

    #[test]
    fn module_rule_reads_import_lines_only() {
        let c = "// this test is about spine events, says the comment\nimport { x } from '../src/cards';";
        assert_eq!(place(c, &valid(), &no_card).domain(), Some("cards"));
    }

    #[test]
    fn header_cards_reads_the_first_lines() {
        assert_eq!(header_cards("# #4201 and #3996\nx\n"), vec![4201, 3996]);
        assert_eq!(header_cards("#!/bin/bash\n# no card"), Vec::<u32>::new());
    }

    #[test]
    fn counts_render_the_five_numbers() {
        let c = TagCounts { read: 10, placed: 7, conflicts: 2, unplaced: 1 };
        assert_eq!(c.render(), "test files read=10 placed=7 conflicts=2 unplaced=1");
    }
}
