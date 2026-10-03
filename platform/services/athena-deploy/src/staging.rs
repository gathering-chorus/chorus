//! #4423 — data staging. Write-Audit-Publish for the model graphs.
//!
//! Jeff, 2026-10-03: "a place ... where we can tear into our model aggressively and
//! get cleanup done without breaking things in prod all the time."
//!
//! `copy` makes the `staging` dataset a copy of prod's `urn:chorus:*` graphs and
//! writes a manifest: per graph, its triple count and a content fingerprint, taken
//! from the SAME bytes that were loaded. `publish` (next step) refuses any graph
//! whose prod fingerprint moved since the copy — a harvester or API write after the
//! copy would otherwise be erased by the swap (Silas review of #4423, hole 1).
//!
//! Same graph names in both datasets, so no code needs to know it is staging: point
//! FUSEKI_QUERY / athena-make at /staging and everything reads the copy.

use std::process::Command;

/// Graphs never copied: too large to be worth copying, and never published.
/// Test results are 5.5M of the 6.1M urn:chorus triples (measured 2026-10-03 10:42).
pub const COPY_SKIP: &[&str] = &["urn:chorus:domains:tests"];

/// Live graphs: written all day by people and services, so a swap from a copy
/// taken earlier would erase real writes. Copied (staging reads need them, e.g. a
/// login needs identity) but NEVER published.
pub const PUBLISH_NEVER: &[&str] = &[
    "urn:chorus:domains:tests",
    "urn:chorus:domains:cards",
    "urn:chorus:domains:board",
    "urn:chorus:domains:messages",
    "urn:chorus:domains:identity",
    "urn:chorus:domains:provenance",
];

/// The graphs a copy takes: every `urn:chorus:` graph except COPY_SKIP, sorted.
pub fn staging_graphs(all: &[String]) -> Vec<String> {
    let mut out: Vec<String> = all
        .iter()
        .filter(|g| g.starts_with("urn:chorus:") && !COPY_SKIP.contains(&g.as_str()))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// A graph's fingerprint from its N-Triples: (triple count, FNV-1a 64 over the
/// sorted lines). Blank-node labels differ on every export, so each `_:label` is
/// written as `_:b` before hashing: two exports of an unchanged graph agree, and
/// any change to a named triple changes the hash. A bnode-only edit that keeps
/// the line set is the one blind spot (count still catches adds and drops).
pub fn fingerprint(nt: &str) -> (usize, u64) {
    let mut lines: Vec<String> = nt
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(normalize_bnodes)
        .collect();
    lines.sort();
    let mut h: u64 = 0xcbf29ce484222325;
    for l in &lines {
        for b in l.bytes().chain(std::iter::once(b'\n')) {
            h ^= b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
    }
    (lines.len(), h)
}

fn normalize_bnodes(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut in_lit = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '"' && !out.ends_with('\\') {
            in_lit = !in_lit;
        }
        if !in_lit && c == '_' && chars.peek() == Some(&':') {
            chars.next();
            while matches!(chars.peek(), Some(ch) if !ch.is_whitespace()) {
                chars.next();
            }
            out.push_str("_:b");
            continue;
        }
        out.push(c);
    }
    out
}

/// One manifest line per graph: `graph\tcount\thash`.
pub fn manifest_line(graph: &str, count: usize, hash: u64) -> String {
    format!("{graph}\t{count}\t{hash:016x}")
}

pub fn parse_manifest(text: &str) -> Vec<(String, usize, String)> {
    text.lines()
        .filter_map(|l| {
            let mut it = l.split('\t');
            Some((it.next()?.to_string(), it.next()?.parse().ok()?, it.next()?.to_string()))
        })
        .collect()
}

fn auth() -> Vec<String> {
    match std::env::var("FUSEKI_ADMIN_PASSWORD") {
        Ok(pw) if !pw.is_empty() => {
            let user = std::env::var("FUSEKI_ADMIN_USER").unwrap_or_else(|_| "admin".into());
            vec!["-u".into(), format!("{user}:{pw}")]
        }
        _ => Vec::new(),
    }
}

fn curl(args: &[&str]) -> Result<(String, String), String> {
    let out = Command::new("curl")
        .args(["-s", "--max-time", "300", "-w", "\n%{http_code}"])
        .args(auth())
        .args(args)
        .output()
        .map_err(|e| format!("curl: {e}"))?;
    let body = String::from_utf8_lossy(&out.stdout).into_owned();
    let (b, code) = body.rsplit_once('\n').unwrap_or(("", ""));
    Ok((b.to_string(), code.to_string()))
}

fn graph_url(base: &str, ds: &str, graph: &str) -> String {
    format!("{base}/{ds}/data?graph={graph}")
}

/// `athena-deploy staging copy`: create /staging (TDB2) if absent, replace each
/// graph in it with prod's, refuse on any count mismatch, write the manifest.
pub fn copy(base: &str, prod: &str, staging: &str, manifest_path: &str) -> Result<String, String> {
    if staging == prod {
        return Err(format!("staging-is-prod: refusing to copy /{prod} onto itself"));
    }
    let (_, code) = curl(&[&format!("{base}/$/datasets/{staging}")])?;
    if code != "200" {
        let (_, c) = curl(&["-X", "POST", &format!("{base}/$/datasets"),
            "--data", &format!("dbName={staging}&dbType=tdb2")])?;
        if c != "200" {
            return Err(format!("staging-create-http-{c}"));
        }
    }
    let q = "SELECT DISTINCT ?g WHERE { GRAPH ?g { } }";
    let (csv, c) = curl(&["--data-urlencode", &format!("query={q}"), "-H", "Accept: text/csv",
        &format!("{base}/{prod}/query")])?;
    if c != "200" {
        return Err(format!("prod-graph-list-http-{c}"));
    }
    let all: Vec<String> = csv.lines().skip(1).map(|l| l.trim().to_string()).collect();
    let graphs = staging_graphs(&all);
    if graphs.is_empty() {
        return Err("prod lists no urn:chorus graphs — unmeasured, not copying nothing".into());
    }
    let dir = std::path::Path::new(manifest_path).parent().map(|p| p.to_path_buf())
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let tmp = dir.join("graph.nt");
    let tmp_s = tmp.to_string_lossy().to_string();
    let mut lines = Vec::new();
    for g in &graphs {
        let (nt, c) = curl(&["-H", "Accept: application/n-triples", &graph_url(base, prod, g)])?;
        if c != "200" {
            return Err(format!("export-http-{c}:{g}"));
        }
        let (count, hash) = fingerprint(&nt);
        std::fs::write(&tmp, &nt).map_err(|e| format!("{tmp_s}: {e}"))?;
        let (_, c) = curl(&["-X", "PUT", "-H", "Content-Type: application/n-triples",
            "--data-binary", &format!("@{tmp_s}"), &graph_url(base, staging, g)])?;
        if !matches!(c.as_str(), "200" | "201" | "204") {
            return Err(format!("load-http-{c}:{g}"));
        }
        let (back, c) = curl(&["-H", "Accept: application/n-triples", &graph_url(base, staging, g)])?;
        let (bcount, _) = fingerprint(&back);
        if c != "200" || bcount != count {
            return Err(format!("count-mismatch:{g}: prod {count}, staging {bcount}"));
        }
        lines.push(manifest_line(g, count, hash));
    }
    let _ = std::fs::remove_file(&tmp);
    std::fs::write(manifest_path, lines.join("\n") + "\n").map_err(|e| format!("{manifest_path}: {e}"))?;
    let total: usize = parse_manifest(&lines.join("\n")).iter().map(|(_, n, _)| n).sum();
    Ok(format!("staging copy: {} graphs, {} triples, counts equal prod; manifest {}", lines.len(), total, manifest_path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copies_chorus_graphs_skips_test_results_and_other_products() {
        let all: Vec<String> = ["urn:chorus:domains:domains", "urn:chorus:domains:tests",
            "urn:gathering:music", "urn:chorus:ontology", "urn:chorus:domains:domains"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(staging_graphs(&all), vec!["urn:chorus:domains:domains", "urn:chorus:ontology"]);
    }

    #[test]
    fn two_exports_of_one_graph_agree_even_with_different_bnode_labels() {
        let a = "<s> <p> _:b0 .\n_:b0 <q> \"x\" .\n<s> <r> \"_:not a bnode\" .\n";
        let b = "_:zz9 <q> \"x\" .\n<s> <r> \"_:not a bnode\" .\n<s> <p> _:zz9 .\n";
        assert_eq!(fingerprint(a), fingerprint(b));
    }

    #[test]
    fn negative_proof_a_changed_named_triple_changes_the_fingerprint() {
        let a = "<s> <p> \"1\" .\n<s> <q> <o> .\n";
        let b = "<s> <p> \"2\" .\n<s> <q> <o> .\n";
        assert_eq!(fingerprint(a).0, fingerprint(b).0);
        assert_ne!(fingerprint(a).1, fingerprint(b).1);
    }

    #[test]
    fn a_literal_that_looks_like_a_bnode_is_not_normalized() {
        assert_ne!(fingerprint("<s> <p> \"_:a\" .\n"), fingerprint("<s> <p> \"_:b\" .\n"));
    }

    #[test]
    fn manifest_round_trips() {
        let l = manifest_line("urn:chorus:domains:domains", 1045, 0xabc);
        assert_eq!(parse_manifest(&l), vec![("urn:chorus:domains:domains".to_string(), 1045, format!("{:016x}", 0xabc))]);
    }

    #[test]
    fn copying_prod_onto_itself_is_refused() {
        assert!(copy("http://localhost:1", "pods", "pods", "/tmp/x").unwrap_err().starts_with("staging-is-prod"));
    }
}
