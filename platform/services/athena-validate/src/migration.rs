// `athena-validate migration` — the V1→V2 Athena migration readout.
//
// #4515. Ported from the retired MCP tool `chorus_migration_readout` (#3594,
// platform/mcp-server/src/server.ts). Jeff kept it: "i dont think our migration
// is done". One fixed, deterministic report: for each V2 chorus:Domain, its
// progress across created / owl-src / loaded / athena-make-run / v1-data-retired /
// v1-code-retired / done.
//
// Cells: ✓ done, · not done, — not yet instrumented. athena-make-run and
// v1-code-retired are not derivable from the graph or the source, so they are
// `—` on every row, never faked as done, and `done` stays · while they are.
//
// The pure part (`compute_rows`, `render`, `src_domains_in`) is separate from
// the I/O (`gather`, `run`) so tests can drive it without a store. This file is
// std-only and has no `crate::` paths, so tests/migration_4515.rs can include it.

use std::process::Command;

/// A stage cell: Some(true)=✓, Some(false)=·, None=— (not yet instrumented).
pub type Cell = Option<bool>;

pub fn cell(v: Cell) -> &'static str {
    match v {
        None => "—",
        Some(true) => "✓",
        Some(false) => "·",
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Row {
    pub domain: String,
    pub created: Cell,
    pub owl_src: Cell,
    pub loaded: Cell,
    pub athena_make: Cell,
    pub v1_retired: Cell,
    pub v1_code: Cell,
    pub done: Cell,
}

/// PURE: raw graph/source facts → one row per domain, sorted by name.
///
/// `domains` are the chorus:Domain local names in the live graph; `src_domains`
/// those declared in a committed MODEL_SET .ttl. athena-make-run and
/// v1-code-retired stay None; done stays false while any stage is unproven.
pub fn compute_rows(domains: &[String], src_domains: &[String]) -> Vec<Row> {
    let mut ds: Vec<String> = domains.to_vec();
    ds.sort();
    ds.into_iter()
        .map(|d| Row {
            owl_src: Some(src_domains.contains(&d)), // declared in committed source
            domain: d,
            created: Some(true),    // it is a chorus:Domain in the graph
            loaded: Some(true),     // read live from the store
            athena_make: None,      // not yet instrumented
            v1_retired: Some(true), // the V1 class is deleted from the model; no V1 twin can exist
            v1_code: None,          // not yet instrumented
            done: Some(false),      // cannot be ✓ while athena-make / v1-code are unproven
        })
        .collect()
}

fn pad(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w { s.to_string() } else { format!("{s}{}", " ".repeat(w - n)) }
}

/// PURE: rows → the fixed matrix text.
pub fn render(rows: &[Row]) -> String {
    let mut lines: Vec<String> = vec![
        "V1→V2 Athena migration — per-domain stage matrix (live graph + committed source):".into(),
        String::new(),
        "  domain            created owl-src loaded athena-make v1-retired v1-code done".into(),
    ];
    for r in rows {
        let l = format!(
            "  {} {} {} {} {} {} {} {}",
            pad(&r.domain, 16),
            pad(cell(r.created), 7),
            pad(cell(r.owl_src), 7),
            pad(cell(r.loaded), 6),
            pad(cell(r.athena_make), 7),
            pad(cell(r.v1_retired), 10),
            pad(cell(r.v1_code), 7),
            cell(r.done)
        );
        lines.push(l.trim_end().to_string());
    }
    let n = rows.len();
    let done = |f: fn(&Row) -> Cell| rows.iter().filter(|r| f(r) == Some(true)).count();
    lines.push(String::new());
    lines.push(format!(
        "{n} domains — created {}/{n} · owl-src {}/{n} · loaded {}/{n} · v1-retired {}/{n}",
        done(|r| r.created),
        done(|r| r.owl_src),
        done(|r| r.loaded),
        done(|r| r.v1_retired)
    ));
    lines.push(
        "legend: ✓ done · · not-done · — not-yet-instrumented (athena-make-run, v1-code-retired). done=✓ only when every stage is real."
            .into(),
    );
    lines.join("\n")
}

/// PURE: the names declared as `chorus:<name> a` in a source text — the same
/// pattern the TS grepped for (`chorus:[A-Za-z0-9_-]+ +a`).
pub fn src_domains_in(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for line in text.lines() {
        let mut rest = line;
        while let Some(i) = rest.find("chorus:") {
            let after = &rest[i + "chorus:".len()..];
            let name_len = after
                .find(|c: char| !(c.is_ascii_alphanumeric() || c == '_' || c == '-'))
                .unwrap_or(after.len());
            let name = &after[..name_len];
            let tail = &after[name_len..];
            let trimmed = tail.trim_start_matches(' ');
            if !name.is_empty() && trimmed.len() < tail.len() && trimmed.starts_with('a') {
                out.push(name.to_string());
            }
            rest = &after[name_len..];
        }
    }
    out
}

/// The committed MODEL_SET sources the TS read, relative to the repo root.
pub const SOURCE_FILES: [&str; 4] = [
    "roles/silas/ontology/chorus.ttl",
    "roles/kade/ontology/werk-domains.ttl",
    "roles/wren/ontology/domains-wren-silas.ttl",
    "roles/kade/ontology/domains-kade-3581.ttl",
];

pub const DOMAINS_QUERY: &str = "PREFIX chorus: <https://jeffbridwell.com/chorus#> \
SELECT ?s WHERE { GRAPH <urn:chorus:domains:domains> { ?s a chorus:Domain } }";

/// PURE: a SPARQL CSV response → domain local names. None when the body has no
/// header, i.e. the query never ran — never an empty list, which would read as
/// "no domains" when the truth is "not measured".
pub fn parse_domains(body: &str) -> Option<Vec<String>> {
    let mut lines = body.lines().filter(|l| !l.trim().is_empty());
    let header = lines.next()?;
    if header.trim() != "s" {
        return None;
    }
    Some(
        lines
            .map(|l| l.trim().trim_matches('"'))
            .map(|iri| iri.rsplit(['#', '/']).next().unwrap_or(iri).to_string())
            .filter(|n| !n.is_empty())
            .collect(),
    )
}

fn repo_root() -> String {
    for k in ["CHORUS_ROOT", "CHORUS_HOME"] {
        if let Ok(v) = std::env::var(k) {
            if !v.is_empty() {
                return v;
            }
        }
    }
    "/Users/jeffbridwell/CascadeProjects/chorus".to_string()
}

fn endpoint() -> String {
    std::env::var("FUSEKI_QUERY")
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "http://localhost:3030/pods/query".into())
}

/// I/O: read the domains from the store. Err names why it could not be read.
pub fn read_domains() -> Result<Vec<String>, String> {
    let ep = endpoint();
    let out = Command::new("curl")
        .args(["-sS", "--fail", "--max-time", "30", "-H", "Accept: text/csv", "--data-urlencode"])
        .arg(format!("query={DOMAINS_QUERY}"))
        .arg(&ep)
        .output()
        .map_err(|e| format!("curl failed to start: {e}"))?;
    if !out.status.success() {
        let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(format!("{ep}: {}", if why.is_empty() { "store unreachable".into() } else { why }));
    }
    parse_domains(&String::from_utf8_lossy(&out.stdout))
        .ok_or_else(|| format!("{ep}: response is not a SPARQL result"))
}

/// I/O: the domains declared in the committed sources, plus the files that
/// could not be read (named in the report, so a · is never silently caused by
/// a missing file).
pub fn read_src_domains(root: &str) -> (Vec<String>, Vec<String>) {
    let mut names = Vec::new();
    let mut unreadable = Vec::new();
    for f in SOURCE_FILES {
        match std::fs::read_to_string(format!("{root}/{f}")) {
            Ok(t) => names.extend(src_domains_in(&t)),
            Err(_) => unreadable.push(f.to_string()),
        }
    }
    (names, unreadable)
}

/// The subcommand. 0 with the report; 2 (UNMEASURED, named) if the store
/// cannot be read — never a matrix built on no data.
pub fn run() -> i32 {
    let domains = match read_domains() {
        Ok(d) => d,
        Err(why) => {
            println!("migration|UNMEASURED|store could not be read — {why}");
            return 2;
        }
    };
    let root = repo_root();
    let (src, unreadable) = read_src_domains(&root);
    let src: Vec<String> = domains.iter().filter(|d| src.contains(d)).cloned().collect();
    println!("{}", render(&compute_rows(&domains, &src)));
    for f in unreadable {
        println!("note: source unreadable, owl-src reads · for its domains: {root}/{f}");
    }
    0
}
