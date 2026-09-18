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
    Neighbor,
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
            Rule::Neighbor => "neighbor",
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
    // #4201 — athena-make is the API every domain is read and written through,
    // and athena-model is the layer under it. A test that MENTIONS either is
    // usually just reaching the surface it tests: `/api/athena/` and the
    // athena-make binary tagged the security-envelope test, a spine e2e gate
    // and a test-dispatch gate as `domains`. Only a test that IS one of those
    // crates belongs to domains, which is what the card says: athena-make,
    // athena-model and model tests.
    ("athena-", "domains"),
    ("properties-resolver", "Properties"),
    // #4201 — the cards CLI is the board's client: 48 of its test files named
    // no route, no class and no import the tables know, because they drive the
    // binary they ship with. The crate IS the cards domain.
    ("cards", "cards"),
    ("chorus-mcp", "spine"),
    ("chorus-messaging", "messages"),
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
        // The value is what follows the KEY. Splitting the whole line on the
        // quote returns `name` for `"name": "clearing"` — every package.json in
        // the tree read as the unit "name" until 2026-09-18.
        .and_then(|l| l[key.len()..].split(&['"', '\''][..]).nth(1))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
}

const MODULES: &[(&str, &str)] = &[
    ("logs-query", "logs"),
    ("log-reader", "logs"),
    ("class-atlas", "domains"),
    ("chorus-domain", "domains"),
    ("domain-renderer", "domains"),
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

/// Every hit a table makes, uncollapsed. `fire` folds several domains into one
/// empty-domain signal, which hides WHICH rules agree — the plurality below
/// needs to count them.
/// True when `content` DEFINES `name` as its own type. #4201 — the class rule
/// reads a graph class name anywhere in a file, and chorus-hooks declares its
/// own `pub struct Commitment` (an autonomy commitment, nothing to do with the
/// services registry's Commitment row). 31 of the 74 cases left covering
/// `services` were that one file. A file that defines a name is that name's
/// home, not a test of the graph class that happens to share it.
fn defines_locally(content: &str, name: &str) -> bool {
    content.lines().map(str::trim_start).any(|l| {
        ["struct", "class", "interface", "enum", "type"].iter().any(|kw| {
            let head = l.strip_prefix("pub ").unwrap_or(l);
            let head = head.strip_prefix("export ").unwrap_or(head);
            head.strip_prefix(kw)
                .and_then(|r| r.strip_prefix(' '))
                .map(|r| r.trim_start().starts_with(name))
                .unwrap_or(false)
        })
    })
}

fn fire_all(rule: Rule, table: &[(&str, &str)], hay: &str, valid: &[String]) -> Vec<Signal> {
    let mut hits: Vec<Signal> = Vec::new();
    for (needle, dom) in table.iter().copied() {
        if rule == Rule::Class && defines_locally(hay, needle) {
            continue;
        }
        if hay.contains(needle)
            && valid.iter().any(|v| v == dom)
            && !hits.iter().any(|h| h.domain == dom)
        {
            hits.push(Signal { rule, domain: dom.to_string(), evidence: needle.to_string() });
        }
    }
    hits
}

/// The domain named by strictly more rules than any other, if there is one.
/// #4201 — rules disagreeing is not the same as rules being evenly split: a
/// binary name mentioned once in a fixture does not outweigh a route, a class
/// and an import all naming the same domain.
fn plurality(signals: &[Signal]) -> Option<String> {
    let mut tally: Vec<(String, Vec<Rule>)> = Vec::new();
    for s in signals.iter().filter(|s| !s.domain.is_empty()) {
        match tally.iter_mut().find(|(d, _)| *d == s.domain) {
            Some((_, rules)) => {
                if !rules.contains(&s.rule) {
                    rules.push(s.rule);
                }
            }
            None => tally.push((s.domain.clone(), vec![s.rule])),
        }
    }
    tally.sort_by_key(|(_, rules)| std::cmp::Reverse(rules.len()));
    match tally.as_slice() {
        [(d, top), rest @ ..] if rest.first().is_some_and(|(_, n)| n.len() < top.len()) => {
            Some(d.clone())
        }
        _ => None,
    }
}

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
        // #4201 — a Rust crate imports as `use class_atlas::…` while the module
        // table (and every path, package and binary name in this repo) spells
        // the same unit `class-atlas`. Underscores are the crate-name spelling
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
/// The quoted specs of a file's RELATIVE imports — `../src/coherence-check`,
/// `./helpers`. #4201: a test that imports a source file is testing THAT file,
/// and the source file names routes and classes the test itself never spells.
pub fn relative_imports(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for line in content.lines() {
        let l = line.trim_start();
        if l.starts_with("//") || l.starts_with("*") {
            continue;
        }
        if !(l.contains("from ") || l.contains("require(") || l.contains("import(")) {
            continue;
        }
        for q in ['\'', '"'] {
            let mut it = l.split(q);
            it.next();
            while let Some(spec) = it.next() {
                if (spec.starts_with("./") || spec.starts_with("../"))
                    && !out.contains(&spec.to_string())
                {
                    out.push(spec.to_string());
                }
                if it.next().is_none() {
                    break;
                }
            }
        }
    }
    out
}

/// `spec` resolved against `from`'s directory, with the extensions a JS/TS
/// import may omit. Returns candidates in probe order, not a proven file.
pub fn resolve_relative(from: &str, spec: &str) -> Vec<String> {
    let mut dir: Vec<&str> = from.split('/').collect();
    dir.pop();
    let mut parts: Vec<String> = dir.iter().map(|s| s.to_string()).collect();
    for seg in spec.split('/') {
        match seg {
            "." | "" => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other.to_string()),
        }
    }
    let base = parts.join("/");
    let stem = base.strip_suffix(".js").unwrap_or(&base).to_string();
    let mut out = vec![base.clone()];
    for ext in [".ts", ".tsx", ".js", ".cjs", ".mjs", ".rs"] {
        out.push(format!("{stem}{ext}"));
    }
    for idx in ["/index.ts", "/index.js"] {
        out.push(format!("{stem}{idx}"));
    }
    out.dedup();
    out
}

/// The four external rules read against one file's content — no unit, no card,
/// no recursion. This is what the neighbour rule asks of an imported file.
/// A route path whose own segment names a live domain. Only ever consulted for
/// an IMPORTED file (the neighbour hop): a source file that registers exactly
/// one route family is about that family, and the 43-row ROUTES table cannot
/// keep up with an API that adds routes weekly. Tried at the top level on
/// 2026-09-18 and reverted — there it placed 1 file and created 8 conflicts.
fn route_segment_signal(content: &str, valid: &[String]) -> Option<Signal> {
    let mut hits: Vec<Signal> = Vec::new();
    for (i, _) in content.match_indices("/api/") {
        let rest = &content[i + 1..];
        let end = rest
            .find(|c: char| !(c.is_ascii_alphanumeric() || "/-_".contains(c)))
            .unwrap_or(rest.len());
        for seg in rest[..end].split('/').skip(1) {
            if let Some(d) = valid.iter().find(|v| *v == seg) {
                if !hits.iter().any(|h: &Signal| h.domain == *d) {
                    hits.push(Signal {
                        rule: Rule::Route,
                        domain: d.clone(),
                        evidence: format!("/{}", &rest[..end]),
                    });
                }
                break;
            }
        }
    }
    (hits.len() == 1).then(|| hits.remove(0))
}

fn external_signal(content: &str, valid: &[String]) -> Option<Signal> {
    let imports = import_lines(content);
    let mut hits: Vec<Signal> = Vec::new();
    hits.extend(fire_all(Rule::Route, ROUTES, content, valid));
    hits.extend(fire_all(Rule::Binary, BINARIES, content, valid));
    hits.extend(fire_all(Rule::Class, CLASSES, content, valid));
    hits.extend(fire_all(Rule::Module, MODULES, &imports, valid));
    // #4201 — the imported file has to be ABOUT one domain to speak for the
    // test. Taking its first hit let clearing's server.ts, which serves many
    // routes, tag 67 clearing tests `domains` off one incidental
    // `/api/chorus/domain/` — while the authored row says clearing is
    // messages. An ambiguous neighbour says nothing and the unit rule decides.
    if hits.is_empty() {
        return route_segment_signal(content, valid);
    }
    let domain = one_domain(&hits).or_else(|| plurality(&hits))?;
    hits.into_iter().find(|s| s.domain == domain)
}

/// #4201 — the file a test imports is the file it exercises. Reads each
/// relative import target and takes the first single domain the external rules
/// find there. One hop only: a neighbour's neighbours are not consulted.
pub fn place_by_neighbor(
    content: &str,
    path: &str,
    valid: &[String],
    read: &dyn Fn(&str) -> Option<String>,
) -> Option<Signal> {
    let mut found: Vec<Signal> = Vec::new();
    for spec in relative_imports(content) {
        for cand in resolve_relative(path, &spec) {
            let Some(text) = read(&cand) else { continue };
            if let Some(s) = external_signal(&text, valid) {
                if !found.iter().any(|f: &Signal| f.domain == s.domain) {
                    found.push(Signal {
                        rule: Rule::Neighbor,
                        domain: s.domain,
                        evidence: format!("{spec} ({})", s.evidence),
                    });
                }
            }
            break;
        }
    }
    match found.len() {
        0 => None,
        1 => found.pop(),
        _ => {
            let ev = found
                .iter()
                .map(|h| format!("{}={}", h.evidence, h.domain))
                .collect::<Vec<_>>()
                .join(", ");
            Some(Signal {
                rule: Rule::Neighbor,
                domain: String::new(),
                evidence: ev,
            })
        }
    }
}

/// The authored unit → domain rows (`roles/silas/ontology/unit-domain-4084.ttl`,
/// #4084). Keyed on `launchdLabel`, so a crate matches only when the label is
/// its plain name or `com.chorus.<name>` / `com.gathering.<name>`.
pub fn unit_domain_rows(ttl: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut label: Option<String> = None;
    for line in ttl.lines() {
        let l = line.trim();
        if l.starts_with('#') {
            continue;
        }
        if let Some(rest) = l.split("chorus:launchdLabel").nth(1) {
            label = rest.split('"').nth(1).map(str::to_string);
        }
        if let Some(rest) = l.split("chorus:hasDomain").nth(1) {
            if let Some(lb) = label.take() {
                let dom = rest
                    .trim()
                    .trim_end_matches(&[' ', '.', ';'][..])
                    .trim_start_matches("chorus:")
                    .to_string();
                if !dom.is_empty() {
                    out.push((lb, dom));
                }
            }
        }
    }
    out
}

/// `unit` looked up in the authored rows, allowing for the launchd prefix.
fn unit_row_domain(unit: &str, rows: &[(String, String)], valid: &[String]) -> Option<Signal> {
    for (label, dom) in rows {
        let plain = label
            .strip_prefix("com.chorus.")
            .or_else(|| label.strip_prefix("com.gathering."))
            .unwrap_or(label);
        if (plain == unit || label == unit) && valid.iter().any(|v| v == dom) {
            return Some(Signal {
                rule: Rule::Unit,
                domain: dom.clone(),
                evidence: format!("{label} (authored)"),
            });
        }
    }
    None
}

pub fn place_in_unit(
    content: &str,
    unit: Option<&str>,
    valid: &[String],
    card_domain: &dyn Fn(u32) -> Option<String>,
) -> Placement {
    place_in_file(content, "", unit, &[], valid, card_domain, &|_| None)
}

/// The full rule set, including the neighbour rule, which needs the file's own
/// path and a reader to follow a relative import.
pub fn place_in_file(
    content: &str,
    path: &str,
    unit: Option<&str>,
    unit_rows: &[(String, String)],
    valid: &[String],
    card_domain: &dyn Fn(u32) -> Option<String>,
    read: &dyn Fn(&str) -> Option<String>,
) -> Placement {
    let mut signals: Vec<Signal> = Vec::new();
    signals.extend(fire_all(Rule::Route, ROUTES, content, valid));
    signals.extend(fire_all(Rule::Binary, BINARIES, content, valid));
    signals.extend(fire_all(Rule::Class, CLASSES, content, valid));
    let imports = import_lines(content);
    signals.extend(fire_all(Rule::Module, MODULES, &imports, valid));
    // Only when nothing external was named: a file that imports a unit is
    // testing THAT unit, and must not be overridden by the crate it lives in.
    // #4201 — a test inside a unit that HAS a declared domain is part of that
    // unit's domain unless the file itself names it. A clearing tunnel-auth
    // test asserting once on /api/chorus/domain/ is not a test of the domains
    // model; clearing is messages. The unit only wins over mentions, never
    // over the file naming its own unit's domain.
    let unit_domain = unit.and_then(|u| {
        fire(Rule::Unit, UNITS, u, valid).or_else(|| unit_row_domain(u, unit_rows, valid))
    });
    // Only against MENTIONS — a route called, a binary named, a class asserted.
    // An import is different in kind: a file that imports another unit is
    // testing that unit, and keeps its answer.
    if let Some(ud) = &unit_domain {
        let all_mentions = signals
            .iter()
            .all(|s| matches!(s.rule, Rule::Route | Rule::Binary | Rule::Class));
        if !signals.is_empty() && all_mentions && !signals.iter().any(|s| s.domain == ud.domain) {
            signals.clear();
            signals.push(ud.clone());
        }
    }
    // The file's OWN route, derived, when the tables found nothing in it. Same
    // rule as the neighbour hop: exactly one live domain named, or silence.
    // Fired BEFORE the import hop because a file that calls a route is more
    // directly about it than the file it happens to import.
    if signals.is_empty() {
        if let Some(s) = route_segment_signal(content, valid) {
            signals.push(s);
        }
    }
    // Only when nothing in this file named a domain: follow what it imports.
    if signals.is_empty() {
        if let Some(s) = place_by_neighbor(content, path, valid, read) {
            signals.push(s);
        }
    }
    if signals.is_empty() {
        if let Some(u) = unit {
            // The code table carries Jeff's explicit rulings; the authored
            // rows (#4084) carry everything else.
            if let Some(s) = fire(Rule::Unit, UNITS, u, valid)
                .or_else(|| unit_row_domain(u, unit_rows, valid))
            {
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
    // #4201 — last resort on a tie: the crate the file lives in. athena-make's
    // own coverage test names seven domains across two rules and ties at two
    // apiece; the one thing it is beyond argument is a test of athena-make.
    // Only breaks a tie, and only when the unit's domain is already one of the
    // candidates — it can never introduce a domain the file never named.
    let by_unit = || {
        let u = unit?;
        let s = fire(Rule::Unit, UNITS, u, valid)
            .or_else(|| unit_row_domain(u, unit_rows, valid))?;
        signals
            .iter()
            .any(|c| c.domain == s.domain)
            .then_some(s.domain)
    };
    match one_domain(&signals)
        .or_else(|| plurality(&signals))
        .or_else(by_unit)
    {
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
    /// separator was normalised this file fired NO rule — `use class_atlas::`
    /// could not match the table's `class-atlas`, so every service test in the
    /// repo landed on the `tests` fallback.
    #[test]
    fn a_rust_crate_import_tags_the_file() {
        let c = "//! #3373 cors\nuse class_atlas::http_response;\n#[test]\nfn t() {}";
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
        let c = "use class_atlas::http_response;";
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
        // #4201 — each hit is its own signal now, so the line names the rule
        // once per domain rather than folding them into `route [a, b]`.
        assert_eq!(line.matches("route→").count(), 2, "{line}");
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
    // ---- #4201 the neighbour rule ----

    /// NEGATIVE PROOF. The 99 `platform/api/tests` files look like this: no
    /// route string, no class name, no external unit — every signal lives in
    /// the source file they import. Without the neighbour rule the file fires
    /// nothing and lands on the `tests` fallback.
    #[test]
    fn a_test_takes_the_domain_of_the_source_file_it_imports() {
        let test = "import { checkCoherence } from '../src/coherence-check';\n\
                    describe('checkCoherence', () => { it('drifts', () => {}); });";
        let src = "app.get('/api/chorus/context/roles', (req, res) => res.json(roles));";
        let read = |p: &str| (p == "platform/api/src/coherence-check.ts").then(|| src.to_string());

        // the guarded condition VIOLATED: no reader, the neighbour is unreadable
        assert_eq!(
            place_in_file(test, "platform/api/tests/coherence.test.ts", None, &[], &valid(), &no_card, &|_| None),
            Placement::Unplaced,
            "with no neighbour to read, this file must stay unplaced"
        );
        // and with the neighbour readable
        let p = place_in_file(test, "platform/api/tests/coherence.test.ts", None, &[], &valid(), &no_card, &read);
        assert_eq!(p.domain(), Some("roles"));
    }

    /// CONTROL: the file's own signal still wins. A neighbour must never
    /// override a route the test itself spells.
    #[test]
    fn the_files_own_signal_beats_its_neighbor() {
        let test = "import { x } from '../src/thing';\nrequest(app).get('/api/chorus/cards');";
        let src = "app.get('/api/chorus/context/roles', h);";
        let read = |_: &str| Some(src.to_string());
        let p = place_in_file(test, "platform/api/tests/a.test.ts", None, &[], &valid(), &no_card, &read);
        assert_eq!(p.domain(), Some("cards"));
    }

    /// CONTROL: two neighbours naming different domains is a conflict to read,
    /// not a coin flip.
    #[test]
    fn two_neighbors_disagreeing_is_a_conflict() {
        let test = "import a from '../src/a';\nimport b from '../src/b';";
        let read = |p: &str| match p {
            "x/src/a.ts" => Some("app.get('/api/chorus/cards', h);".to_string()),
            "x/src/b.ts" => Some("app.get('/api/chorus/context/roles', h);".to_string()),
            _ => None,
        };
        let p = place_in_file(test, "x/tests/t.test.ts", None, &[], &valid(), &no_card, &read);
        assert!(matches!(p, Placement::Conflict { .. }), "got {p:?}");
    }

    /// CONTROL: one hop only. The neighbour's OWN imports are not followed —
    /// otherwise a shared helper drags every test into one domain.
    #[test]
    fn the_hop_does_not_recurse() {
        let test = "import { h } from './helper';";
        let read = |p: &str| match p {
            "x/tests/helper.ts" => Some("import { q } from '../src/deep';".to_string()),
            "x/src/deep.ts" => Some("app.get('/api/chorus/cards', h);".to_string()),
            _ => None,
        };
        assert_eq!(
            place_in_file(test, "x/tests/t.test.ts", None, &[], &valid(), &no_card, &read),
            Placement::Unplaced
        );
    }

    #[test]
    fn resolve_relative_walks_up_and_tries_the_extensions() {
        let c = resolve_relative("platform/api/tests/a.test.ts", "../src/coherence-check");
        assert!(c.contains(&"platform/api/src/coherence-check.ts".to_string()), "{c:?}");
        let js = resolve_relative("a/b/t.ts", "./x.js");
        assert!(js.contains(&"a/b/x.ts".to_string()), "{js:?}");
    }

    #[test]
    fn relative_imports_ignores_packages_and_comments() {
        let c = "// import { a } from '../src/commented';\n\
                 import express from 'express';\n\
                 const { b } = require('./real');\n";
        assert_eq!(relative_imports(c), vec!["./real".to_string()]);
    }
    /// NEGATIVE PROOF for the authored rows. `clearing` is a crate, not a
    /// LaunchAgent name, so the code table has never held it — 56 of its test
    /// files sat unplaced. #4084 already authored `clearing → messages`; the
    /// unit rule was simply not reading that file.
    #[test]
    fn the_unit_rule_reads_the_authored_rows() {
        let ttl = "chorus:unitdomain-clearing a chorus:UnitDomainMapping ;\n    \
                   chorus:launchdLabel \"com.chorus.clearing\" ;\n    \
                   chorus:hasDomain chorus:messages .\n";
        let rows = unit_domain_rows(ttl);
        assert_eq!(rows, vec![("com.chorus.clearing".to_string(), "messages".to_string())]);
        let mut v = valid();
        v.push("messages".to_string());
        let c = "describe('bridge', () => {});";

        // the guarded condition VIOLATED: no authored rows to read
        assert_eq!(
            place_in_file(c, "directing/clearing/tests/a.test.ts", Some("clearing"), &[], &v, &no_card, &|_| None),
            Placement::Unplaced,
            "with no authored rows this file must stay unplaced"
        );
        let p = place_in_file(c, "directing/clearing/tests/a.test.ts", Some("clearing"), &rows, &v, &no_card, &|_| None);
        assert_eq!(p.domain(), Some("messages"));
    }

    /// CONTROL: a unit with no authored row stays unplaced — the rule must not
    /// guess from the nearest label.
    #[test]
    fn a_unit_with_no_authored_row_stays_unplaced() {
        let rows = vec![("com.chorus.clearing".to_string(), "messages".to_string())];
        assert_eq!(
            place_in_file("x", "a/b.test.ts", Some("clearing-ui"), &rows, &valid(), &no_card, &|_| None),
            Placement::Unplaced
        );
    }

    /// CONTROL: Jeff's ruling in the code table wins over an authored row.
    #[test]
    fn the_code_table_beats_an_authored_row() {
        let rows = vec![("com.chorus.hooks".to_string(), "logs".to_string()),
                        ("chorus-hooks".to_string(), "logs".to_string())];
        let p = place_in_file("x", "a/b.rs", Some("chorus-hooks"), &rows, &valid(), &no_card, &|_| None);
        assert_eq!(p.domain(), Some("spine"));
    }
    /// NEGATIVE PROOF: a package.json name. `"name": "clearing"` split on the
    /// quote yields `name`, so EVERY npm package in the tree declared the unit
    /// "clearing"… no: the unit "name". Both the unit rule and the authored
    /// rows then looked up a word that is not a unit.
    #[test]
    fn a_package_json_declares_its_name_not_the_word_name() {
        let read = |p: &str| {
            (p == "directing/clearing/package.json")
                .then(|| "{\n  \"name\": \"clearing\",\n  \"version\": \"1.0.0\"\n}".to_string())
        };
        assert_eq!(
            declared_unit("directing/clearing/tests/a.test.ts", &read),
            Some("clearing".to_string())
        );
    }

    /// CONTROL: the Cargo spelling still reads.
    #[test]
    fn a_cargo_toml_still_declares_its_name() {
        let read = |p: &str| {
            (p == "x/Cargo.toml").then(|| "[package]\nname = \"chorus-hooks\"\n".to_string())
        };
        assert_eq!(declared_unit("x/src/a.rs", &read), Some("chorus-hooks".to_string()));
    }
    /// NEGATIVE PROOF for the plurality: three rules name `domains`, one names
    /// `value-streams` off a single mention. Even split → conflict, as before;
    /// strict plurality → tagged. Same file, one extra agreeing rule.
    #[test]
    fn a_plurality_of_rules_breaks_a_tie_an_even_split_does_not() {
        let mut v = valid();
        v.push("value-streams".to_string());
        // even split: one rule each — must stay a conflict
        let even = "app.get('/api/chorus/logs'); const s: ValueStreamStep = q;";
        assert!(
            matches!(place(even, &v, &no_card), Placement::Conflict { .. }),
            "an even split must not be broken"
        );
        // plurality: route, binary and module all name logs
        let many = "app.get('/api/chorus/logs');\nimport { q } from 'logs-query';\n                    const s: ValueStreamStep = q;\nconst bin = 'log-harvest';";
        assert_eq!(place(many, &v, &no_card).domain(), Some("logs"));
    }
    /// NEGATIVE PROOF: chorus-hooks declares `pub struct Commitment` — its own
    /// autonomy commitment, not the services registry's row. The class rule
    /// read the word and tagged the file `services`, 31 cases of it.
    #[test]
    fn a_locally_defined_type_does_not_fire_the_class_rule() {
        let defines = "pub struct Commitment {\n    pub id: String,\n}\n                       fn load(p: &Path) -> Vec<Commitment> { vec![] }";
        assert_eq!(place(defines, &valid(), &no_card), Placement::Unplaced);
        // and with the unit known, it falls to the unit rule as it should
        let p = place_in_file(defines, "platform/services/chorus-hooks/src/x.rs",
            Some("chorus-hooks"), &[], &valid(), &no_card, &|_| None);
        assert_eq!(p.domain(), Some("spine"));
    }

    /// CONTROL: a file that USES the class without defining it still fires.
    #[test]
    fn using_a_class_it_does_not_define_still_fires() {
        let uses = "const c: Commitment = await get('/x');\nexpect(c.id).toBe('a');";
        assert_eq!(place(uses, &valid(), &no_card).domain(), Some("services"));
    }
    /// NEGATIVE PROOF: an ambiguous neighbour must not speak. clearing's
    /// server.ts serves many routes; taking its FIRST hit tagged 67 clearing
    /// tests `domains` off one incidental `/api/chorus/domain/` line, against
    /// an authored row that says clearing is messages.
    #[test]
    fn a_neighbor_that_serves_many_domains_says_nothing() {
        let mut v = valid();
        v.push("messages".to_string());
        let test = "import { start } from '../src/server';\nit('boots', () => {});";
        let many = "app.get('/api/chorus/domain/x', h);\napp.get('/api/chorus/cards', h);";
        let one = "app.get('/api/chorus/domain/x', h);";
        let rows = vec![("com.chorus.clearing".to_string(), "messages".to_string())];
        let path = "directing/clearing/tests/server-unit.test.ts";

        // ambiguous neighbour: the unit rule decides, and gets it right
        let p = place_in_file(test, path, Some("clearing"), &rows, &v, &no_card,
            &|q: &str| (q == "directing/clearing/src/server.ts").then(|| many.to_string()));
        assert_eq!(p.domain(), Some("messages"), "{p:?}");

        // control: a neighbour that IS about one domain still speaks, and wins
        let p = place_in_file(test, path, Some("clearing"), &rows, &v, &no_card,
            &|q: &str| (q == "directing/clearing/src/server.ts").then(|| one.to_string()));
        assert_eq!(p.domain(), Some("domains"), "{p:?}");
    }
    /// NEGATIVE PROOF: athena-make is the API every domain is read and written
    /// through, so a MENTION of it says how a test reaches its subject, not
    /// what its subject is. It tagged the security-envelope test, a spine e2e
    /// gate and a test-dispatch gate `domains`. Only a test that IS one of the
    /// athena crates belongs there.
    #[test]
    fn mentioning_athena_make_is_not_a_domain_signal_being_it_is() {
        let mut v = valid();
        v.push("value-streams".to_string());
        let reaches = "const r = await fetch('/api/athena/domains');\n                       execSync('athena-make deploy');\n                       import { verify } from '../src/es256';";
        // the file reaches athena to test something else: athena says nothing,
        // and the one real signal it carries decides
        assert_eq!(place(reaches, &v, &no_card).domain(), Some("identity"));

        // control: a file that IS an athena crate tags domains, off the unit
        let p = place_in_file("fn main() {}", "platform/services/athena-make/src/lib.rs",
            Some("athena-make"), &[], &v, &no_card, &|_| None);
        assert_eq!(p.domain(), Some("domains"));
    }
    /// NEGATIVE PROOF: the unit breaks a tie but can never invent one.
    /// athena-make's own coverage test names seven domains across two rules
    /// and ties two-all; the one thing beyond argument is that it tests
    /// athena-make. The same tie in a crate with no domain stays a conflict,
    /// and a unit whose domain is NOT among the candidates is ignored.
    #[test]
    fn the_unit_breaks_a_tie_and_never_introduces_a_new_domain() {
        let tied = "execSync('werk-test');\nconst c: TestResult = r;\n                    execSync('chorus-model');\nconst d: DomainShape = s;";

        // no unit: the tie stands
        assert!(
            matches!(place(tied, &valid(), &no_card), Placement::Conflict { .. }),
            "with no unit this must stay a conflict"
        );
        // the unit is one of the candidates: it decides
        let p = place_in_file(tied, "platform/services/athena-make/tests/c.rs",
            Some("athena-make"), &[], &valid(), &no_card, &|_| None);
        assert_eq!(p.domain(), Some("domains"));
        // the unit is NOT among the candidates: the mentions are someone
        // else's names in chorus-hooks' own test, so the unit replaces them.
        // (Before the mention-vs-unit precedence landed this was a conflict;
        // the conflict was the file being read as a test of what it names.)
        let p = place_in_file(tied, "platform/services/chorus-hooks/tests/c.rs",
            Some("chorus-hooks"), &[], &valid(), &no_card, &|_| None);
        assert_eq!(p.domain(), Some("spine"), "{p:?}");
    }
    /// The three crates added 2026-09-18, each proved by the state that made
    /// it necessary: the file names nothing the tables know, so without the
    /// unit row it is unplaced.
    #[test]
    fn the_cards_cli_tests_are_the_cards_domain() {
        let c = "const out = execSync(`${CLI} add --title x`);\nexpect(out).toContain('ok');";
        assert_eq!(place(c, &valid(), &no_card), Placement::Unplaced, "control: no signal in the file");
        let p = place_in_file(c, "directing/products/cards/tests/a.test.ts",
            Some("cards"), &[], &valid(), &no_card, &|_| None);
        assert_eq!(p.domain(), Some("cards"));
    }
    /// NEGATIVE PROOF: one incidental mention outranked the crate the test
    /// lives in. clearing's tunnel-auth test asserts once on
    /// `/api/chorus/domain/chorus` and was tagged `domains`; clearing is
    /// messages. Same for a cards test that reads the domain API to compute a
    /// blast radius — the subject is cards.
    #[test]
    fn a_mention_does_not_outrank_the_unit_that_declares_a_domain() {
        let mut v = valid();
        v.push("messages".to_string());
        let rows = vec![("com.chorus.clearing".to_string(), "messages".to_string())];
        let c = "expect(stubHits).toEqual(['/api/chorus/domain/chorus']);";

        // with no unit domain to weigh it against, the mention still decides
        let p = place_in_file(c, "x/t.test.ts", None, &[], &v, &no_card, &|_| None);
        assert_eq!(p.domain(), Some("domains"), "control: the rule still fires alone");

        // inside a unit that declares one, the unit is the subject
        let p = place_in_file(c, "directing/clearing/tests/t.test.ts",
            Some("clearing"), &rows, &v, &no_card, &|_| None);
        assert_eq!(p.domain(), Some("messages"));
    }

    /// CONTROL: the unit never overrides the file naming the unit's OWN
    /// domain — a clearing test about messages keeps every signal it earned.
    #[test]
    fn the_unit_does_not_override_a_file_that_names_its_own_domain() {
        let mut v = valid();
        v.push("messages".to_string());
        let rows = vec![("com.chorus.clearing".to_string(), "messages".to_string())];
        let c = "await request(app).post('/api/chorus/nudge');";
        let p = place_in_file(c, "directing/clearing/tests/t.test.ts",
            Some("clearing"), &rows, &v, &no_card, &|_| None);
        assert_eq!(p.domain(), Some("messages"));
        match p {
            Placement::Tagged { signals, .. } => {
                assert_eq!(signals[0].rule, Rule::Route, "the route signal survives, not a unit stand-in");
            }
            other => panic!("{other:?}"),
        }
    }
    /// NEGATIVE PROOF: a source file the ROUTES table has never heard of. The
    /// table is 43 hand-kept rows against an API that adds routes weekly, so
    /// an imported handler registering `/api/chorus/<domain>` said nothing and
    /// its test stayed unplaced. Derived from the live domain list instead —
    /// and only for an imported file, only when the four rules found nothing,
    /// only when exactly one domain is named.
    #[test]
    fn an_imported_handler_naming_one_unlisted_route_family_places() {
        // `deploys` is a live domain with NO row in the 43-line ROUTES table —
        // exactly the gap this derivation covers.
        let test = "import { handler } from '../src/handlers/deploys';";
        let v = valid();
        let src = "app.get('/api/chorus/deploys/:id', handler);";
        let read = |p: &str| (p == "a/src/handlers/deploys.ts").then(|| src.to_string());

        // the guarded condition: without the derivation, nothing fires
        assert_eq!(
            place_in_file(test, "a/tests/t.test.ts", None, &[], &v, &no_card, &|_| None),
            Placement::Unplaced
        );
        let p = place_in_file(test, "a/tests/t.test.ts", None, &[], &v, &no_card, &read);
        assert_eq!(p.domain(), Some("deploys"));
    }

    /// CONTROL: a neighbour naming TWO domains this way still says nothing —
    /// the derivation must not turn an ambiguous file into a coin flip.
    #[test]
    fn a_derived_route_that_names_two_domains_stays_silent() {
        let test = "import { h } from '../src/handlers/both';";
        let v = valid();
        // both families are absent from ROUTES, so only the derivation can see
        // them — and seeing two, it must say nothing.
        let src = "app.get('/api/chorus/deploys/x', h);\napp.get('/api/chorus/cicd/y', h);";
        let read = |_: &str| Some(src.to_string());
        assert_eq!(
            place_in_file(test, "a/tests/t.test.ts", None, &[], &v, &no_card, &read),
            Placement::Unplaced
        );
    }
    /// NEGATIVE PROOF: an integration test that calls a route the table has
    /// never heard of. 172 of the unplaced files import nothing at all — they
    /// reach the API over HTTP — so the neighbour hop cannot help them and the
    /// hand-kept table is the only thing that could have.
    #[test]
    fn a_file_calling_one_unlisted_route_places_on_its_own() {
        let v = valid();
        // `deploys` is live and has no ROUTES row
        let c = "const r = await fetch('http://localhost:3340/api/chorus/deploys/x');";
        assert_eq!(
            fire(Rule::Route, ROUTES, c, &v),
            None,
            "the table must genuinely not cover this — otherwise the proof is hollow"
        );
        assert_eq!(place(c, &v, &no_card).domain(), Some("deploys"));
    }

    /// CONTROL: it only speaks when nothing else did. A file that also calls a
    /// LISTED route keeps the table's answer instead of gaining a second one.
    #[test]
    fn the_derived_route_never_competes_with_a_rule_that_fired() {
        let v = valid();
        let c = "await fetch('/api/chorus/cards');\nawait fetch('/api/chorus/deploys/x');";
        let p = place(c, &v, &no_card);
        assert_eq!(p.domain(), Some("cards"), "{p:?}");
    }
}
