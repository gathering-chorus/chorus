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

/// The model's graph families. Borg's environments hang on chorus domains
/// (`usesEnvironment` in urn:borg:instances), so a model cleanup that moves them
/// must stage both (#4423 reopen, 2026-10-03: #4353's environment move could not
/// be staged because the copy held urn:chorus:* only).
pub const MODEL_PREFIXES: &[&str] = &["urn:chorus:", "urn:borg:"];

/// A publish's own bookkeeping graphs: never copied, never treated as stale.
const SWAP_PREFIXES: &[&str] = &["urn:chorus:previous:", "urn:chorus:incoming:"];

fn is_model_graph(g: &str) -> bool {
    MODEL_PREFIXES.iter().any(|p| g.starts_with(p)) && !SWAP_PREFIXES.iter().any(|p| g.starts_with(p))
}

/// The graphs a copy takes: every model graph except COPY_SKIP, sorted.
pub fn staging_graphs(all: &[String]) -> Vec<String> {
    let mut out: Vec<String> = all
        .iter()
        .filter(|g| is_model_graph(g) && !COPY_SKIP.contains(&g.as_str()))
        .cloned()
        .collect();
    out.sort();
    out.dedup();
    out
}

/// Graphs in staging that the new copy will not write: gone from prod, so dropped.
pub fn stale_in_staging(in_staging: &[String], copying: &[String]) -> Vec<String> {
    in_staging.iter().filter(|g| is_model_graph(g) && !copying.contains(g)).cloned().collect()
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
    let mut escaped = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_lit && escaped {
            escaped = false; // this char is escaped, whatever it is
        } else if in_lit && c == '\\' {
            escaped = true;
        } else if c == '"' {
            in_lit = !in_lit; // Wren: an escaped backslash before a quote must not hide it
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
    // Wren: a re-copy drops staging graphs prod no longer has, so staging is a copy, not a pile.
    let (scsv, c) = curl(&["--data-urlencode", &format!("query={q}"), "-H", "Accept: text/csv",
        &format!("{base}/{staging}/query")])?;
    if c != "200" {
        return Err(format!("staging-graph-list-http-{c}"));
    }
    let in_staging: Vec<String> = scsv.lines().skip(1).map(|l| l.trim().to_string()).collect();
    for g in stale_in_staging(&in_staging, &graphs) {
        let (_, c) = curl(&["-X", "DELETE", &graph_url(base, staging, &g)])?;
        if !matches!(c.as_str(), "200" | "204") {
            return Err(format!("drop-stale-http-{c}:{g}"));
        }
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



/// The data-quality counts the 2026-10-02/03 cleanup measured, read from one
/// dataset. Reported before -> after by the audit (they should go down); the
/// drop gate is what refuses.
pub const QUALITY_COUNTS: &[(&str, &str)] = &[
    ("rows the API can't open (urn: names)", "SELECT (COUNT(DISTINCT ?s) AS ?n) WHERE { GRAPH ?g { ?s a ?t } FILTER(STRSTARTS(STR(?g),\"urn:chorus:\") && STRSTARTS(STR(?s),\"urn:\")) }"),
    ("domain links to products that don't exist", "PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(*) AS ?n) WHERE { GRAPH <urn:chorus:domains:domains> { ?d a c:Domain ; c:partOf ?p } FILTER NOT EXISTS { GRAPH ?g { ?p a c:Product } } }"),
    ("domains with no layer", "PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?d) AS ?n) WHERE { GRAPH <urn:chorus:domains:domains> { ?d a c:Domain FILTER NOT EXISTS { ?d c:inLayer ?l } } }"),
    ("domains named *-domain", "PREFIX c: <https://jeffbridwell.com/chorus#> SELECT (COUNT(DISTINCT ?d) AS ?n) WHERE { GRAPH <urn:chorus:domains:domains> { ?d a c:Domain FILTER(STRENDS(STR(?d),\"-domain\")) } }"),
];

fn quality(base: &str, ds: &str) -> Vec<String> {
    QUALITY_COUNTS.iter().map(|(_, q)| {
        match curl(&["--data-urlencode", &format!("query={q}"), "-H", "Accept: text/csv", &format!("{base}/{ds}/query")]) {
            Ok((csv, c)) if c == "200" => csv.lines().nth(1).unwrap_or("?").trim().trim_matches('"').to_string(),
            _ => "unmeasured".into(),
        }
    }).collect()
}

/// One line per count: `name: prod -> staging`.
pub fn quality_report(prod: &[String], staging: &[String]) -> String {
    QUALITY_COUNTS.iter().zip(prod.iter().zip(staging.iter()))
        .map(|((name, _), (p, s))| format!("{name}: {p} -> {s}"))
        .collect::<Vec<_>>().join("; ")
}

/// Port the audit's own athena-make serves staging on (prod is 3360, werk slots 3363-3365).
pub const AUDIT_PORT: u16 = 3367;

/// Fingerprints of every manifest graph in staging, as a stable string: an audit is
/// valid only for the staging content it measured.
fn staging_state(base: &str, staging: &str, manifest_path: &str) -> Result<String, String> {
    let m = parse_manifest(&std::fs::read_to_string(manifest_path).map_err(|e| format!("no manifest {manifest_path}: {e} — run staging copy first"))?);
    let graphs: Vec<String> = m.into_iter().map(|(g, _, _)| g).collect();
    Ok(fingerprints(base, staging, &graphs)?.iter().map(|(g, h)| format!("{g}\t{h}")).collect::<Vec<_>>().join("\n"))
}

/// The audit's verdict, pure: what prod serves now vs what an athena-make reading
/// staging serves, with only the drops named in `allow` let through (#4338 rule).
pub fn audit_verdict(prod: &str, staged: &str, allow: &str) -> Result<String, String> {
    let all = crate::served_drops(prod, staged);
    let named: Vec<&String> = all.iter().filter(|d| allow.lines().any(|l| l.trim() == d.as_str())).collect();
    let unapproved = crate::unapproved_drops(all.clone(), allow);
    if !unapproved.is_empty() {
        return Err(format!("staging takes away what prod serves (name each drop in ATHENA_ALLOW_DROPS to publish it): {}", unapproved.join("; ")));
    }
    let last = |t: &str| t.lines().last().unwrap_or("").to_string();
    Ok(format!("audit pass: prod {} / staging {}; named drops: {}", last(prod), last(staged),
        if named.is_empty() { "none".to_string() } else { named.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ") }))
}

/// `athena-deploy staging audit`: serve staging from its own athena-make on
/// AUDIT_PORT, snapshot both, refuse any unnamed drop, and record the staging state
/// it passed so publish can insist the audited content is what it swaps.
pub fn audit(base: &str, prod: &str, staging: &str, manifest_path: &str, prod_api: &str, make_bin: &str) -> Result<String, String> {
    let before = crate::read_served_from(prod_api, &format!("{base}/{prod}/query"))?;
    let mut child = Command::new(make_bin)
        .args(["serve", "--port", &AUDIT_PORT.to_string()])
        .env("CHORUS_FUSEKI", format!("{base}/{staging}"))
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null())
        .spawn().map_err(|e| format!("{make_bin}: {e}"))?;
    let api = format!("http://localhost:{AUDIT_PORT}");
    let mut up = false;
    for _ in 0..120 {
        if let Ok((_, c)) = curl(&[&format!("{api}/health")]) { if c == "200" { up = true; break; } }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    let after = if up { crate::read_served_from(&api, &format!("{base}/{staging}/query")) } else { Err(format!("staging athena-make never answered {api}/health in 120 s — unmeasured")) };
    let _ = child.kill();
    let _ = child.wait();
    let verdict = audit_verdict(&before, &after?, &std::env::var("ATHENA_ALLOW_DROPS").unwrap_or_default())?;
    let verdict = format!("{verdict}\n  {}", quality_report(&quality(base, prod), &quality(base, staging)));
    let dir = std::path::Path::new(manifest_path).parent().unwrap_or(std::path::Path::new("."));
    std::fs::write(dir.join("audited.tsv"), staging_state(base, staging, manifest_path)?).map_err(|e| e.to_string())?;
    Ok(verdict)
}

/// Publish's audit gate: the staging content now must be exactly what the last
/// passing audit measured.
pub fn audited_matches(audited: Option<&str>, now: &str) -> Result<(), String> {
    match audited {
        None => Err("no passing audit — run `athena-deploy staging audit` first".into()),
        Some(a) if a == now => Ok(()),
        Some(_) => Err("staging changed since its audit — audit again before publishing".into()),
    }
}

/// The graph a published graph's previous version is kept in, for rollback.
pub fn previous_graph(g: &str) -> String {
    format!("urn:chorus:previous:{}", g.trim_start_matches("urn:chorus:"))
}

fn incoming_graph(g: &str) -> String {
    format!("urn:chorus:incoming:{}", g.trim_start_matches("urn:chorus:"))
}

/// What a publish would do, decided from fingerprints only (pure, so the refusals
/// have tests without a store). `manifest`: graph → hash at copy time.
/// `prod_now` / `staging_now`: graph → hash now. Returns the graphs to swap, or a
/// refusal naming every graph that blocks it:
/// - prod moved since the copy (a runtime write would be erased by the swap);
/// - a never-publish (live) graph was edited in staging.
pub fn plan_publish(
    manifest: &[(String, String)],
    prod_now: &[(String, String)],
    staging_now: &[(String, String)],
) -> Result<Vec<String>, String> {
    let get = |v: &[(String, String)], g: &str| v.iter().find(|(k, _)| k == g).map(|(_, h)| h.clone());
    let mut swap = Vec::new();
    let mut refusals = Vec::new();
    for (g, copied) in manifest {
        let staged = get(staging_now, g);
        if staged.as_deref() == Some(copied.as_str()) {
            continue; // unchanged in staging: nothing to publish
        }
        if PUBLISH_NEVER.contains(&g.as_str()) {
            refusals.push(format!("{g}: live graph edited in staging (never published)"));
            continue;
        }
        match get(prod_now, g) {
            Some(h) if &h == copied => swap.push(g.clone()),
            Some(_) => refusals.push(format!("{g}: prod changed since the copy (re-copy, redo the cleanup)")),
            None => refusals.push(format!("{g}: prod fingerprint unmeasured")),
        }
    }
    if refusals.is_empty() { Ok(swap) } else { Err(refusals.join("; ")) }
}

/// The one update that swaps every staged graph in: each prod graph moves to its
/// previous-graph, each incoming copy moves into its place. One request = one
/// TDB2 transaction, so readers see all old or all new.
pub fn swap_sparql(graphs: &[String]) -> String {
    graphs
        .iter()
        .map(|g| format!("MOVE SILENT <{g}> TO <{}> ; MOVE <{}> TO <{g}>", previous_graph(g), incoming_graph(g)))
        .collect::<Vec<_>>()
        .join(" ; ")
}

/// Rollback: the same move reversed, from previous-graph back into place.
pub fn rollback_sparql(graphs: &[String]) -> String {
    graphs
        .iter()
        .map(|g| format!("MOVE <{}> TO <{g}>", previous_graph(g)))
        .collect::<Vec<_>>()
        .join(" ; ")
}

fn fingerprints(base: &str, ds: &str, graphs: &[String]) -> Result<Vec<(String, String)>, String> {
    let mut out = Vec::new();
    for g in graphs {
        let (nt, c) = curl(&["-H", "Accept: application/n-triples", &graph_url(base, ds, g)])?;
        if c != "200" && c != "404" {
            return Err(format!("export-http-{c}:{g}"));
        }
        let (_, h) = fingerprint(if c == "404" { "" } else { &nt });
        out.push((g.clone(), format!("{h:016x}")));
    }
    Ok(out)
}

/// `athena-deploy staging publish`: swap in every graph edited in staging, if prod
/// has not moved since the copy. Records what it swapped for rollback.
pub fn publish(base: &str, prod: &str, staging: &str, manifest_path: &str) -> Result<String, String> {
    let m = parse_manifest(&std::fs::read_to_string(manifest_path).map_err(|e| format!("no manifest {manifest_path}: {e} — run staging copy first"))?);
    let graphs: Vec<String> = m.iter().map(|(g, _, _)| g.clone()).collect();
    let man: Vec<(String, String)> = m.into_iter().map(|(g, _, h)| (g, h)).collect();
    let swap = plan_publish(&man, &fingerprints(base, prod, &graphs)?, &fingerprints(base, staging, &graphs)?)?;
    // Wren: only audited content is published (#4423 AC: audit before publish).
    let dir0 = std::path::Path::new(manifest_path).parent().unwrap_or(std::path::Path::new("."));
    audited_matches(std::fs::read_to_string(dir0.join("audited.tsv")).ok().as_deref(), &staging_state(base, staging, manifest_path)?)?;
    if swap.is_empty() {
        return Ok("staging publish: nothing edited in staging — prod untouched".into());
    }
    let dir = std::path::Path::new(manifest_path).parent().unwrap_or(std::path::Path::new("."));
    let tmp = dir.join("graph.nt");
    let tmp_s = tmp.to_string_lossy().to_string();
    for g in &swap {
        let (nt, c) = curl(&["-H", "Accept: application/n-triples", &graph_url(base, staging, g)])?;
        if c != "200" { return Err(format!("export-http-{c}:{g}")); }
        std::fs::write(&tmp, &nt).map_err(|e| format!("{tmp_s}: {e}"))?;
        let (_, c) = curl(&["-X", "PUT", "-H", "Content-Type: application/n-triples",
            "--data-binary", &format!("@{tmp_s}"), &graph_url(base, prod, &incoming_graph(g))])?;
        if !matches!(c.as_str(), "200" | "201" | "204") { return Err(format!("stage-into-prod-http-{c}:{g}")); }
    }
    let _ = std::fs::remove_file(&tmp);
    // Check prod again right before the swap: the window is this one request.
    let man_swap: Vec<(String, String)> = man.iter().filter(|(g, _)| swap.contains(g)).cloned().collect();
    let again = fingerprints(base, prod, &swap)?;
    if let Some((g, _)) = man_swap.iter().find(|(g, h)| again.iter().find(|(k, _)| k == g).map(|(_, x)| x) != Some(h)) {
        return Err(format!("{g}: prod changed during publish — nothing swapped (incoming copies left for inspection)"));
    }
    let t0 = std::time::Instant::now();
    let (_, c) = curl(&["-X", "POST", "-H", "Content-Type: application/sparql-update",
        "--data-binary", &swap_sparql(&swap), &format!("{base}/{prod}/update")])?;
    if !matches!(c.as_str(), "200" | "204") { return Err(format!("swap-http-{c}")); }
    let ms = t0.elapsed().as_millis();
    let after = fingerprints(base, prod, &swap)?;
    let staged = fingerprints(base, staging, &swap)?;
    if after != staged { return Err("swap ran but prod does not match staging — roll back".into()); }
    std::fs::write(dir.join("published.tsv"), swap.join("\n") + "\n").map_err(|e| e.to_string())?;
    Ok(format!("staging publish: {} graph(s) swapped in one update in {ms} ms; previous versions kept for rollback: {}; {}", swap.len(), swap.join(", "), reserve_prod(prod)))
}


/// athena-make reads shapes and routes at boot: after a swap or a rollback it serves
/// the OLD model until restarted (the 2026-10-02 16:59 lockout was a stale serve).
/// Restart it through athena-serve and wait for /health, or say plainly it did not.
fn reserve_prod(prod: &str) -> String {
    if prod != "pods" {
        return format!("prod dataset is /{prod}, not /pods — live athena-make left alone");
    }
    let bin = std::env::var("ATHENA_SERVE_BIN").unwrap_or_else(|_| format!("{}/.chorus/bin/athena-serve", std::env::var("HOME").unwrap_or_default()));
    match Command::new(&bin).args(["com.chorus.athena-make", "http://localhost:3360/health", "--kickstart", "--timeout", "90"]).status() {
        Ok(s) if s.success() => "athena-make restarted and healthy".into(),
        Ok(s) => format!("WARNING athena-make restart exited {s} — prod may serve the old model until restarted"),
        Err(e) => format!("WARNING could not run {bin}: {e} — restart athena-make by hand"),
    }
}

/// `athena-deploy staging rollback`: put back the previous version of every graph
/// the last publish swapped, in one update.
pub fn rollback(base: &str, prod: &str, manifest_path: &str) -> Result<String, String> {
    let dir = std::path::Path::new(manifest_path).parent().unwrap_or(std::path::Path::new("."));
    let list = std::fs::read_to_string(dir.join("published.tsv")).map_err(|e| format!("nothing published to roll back: {e}"))?;
    let graphs: Vec<String> = list.lines().filter(|l| !l.is_empty()).map(str::to_string).collect();
    let t0 = std::time::Instant::now();
    let (_, c) = curl(&["-X", "POST", "-H", "Content-Type: application/sparql-update",
        "--data-binary", &rollback_sparql(&graphs), &format!("{base}/{prod}/update")])?;
    if !matches!(c.as_str(), "200" | "204") { return Err(format!("rollback-http-{c}")); }
    let _ = std::fs::remove_file(dir.join("published.tsv"));
    let ms = t0.elapsed().as_millis();
    Ok(format!("staging rollback: {} graph(s) restored in {ms} ms; {}", graphs.len(), reserve_prod(prod)))
}

#[cfg(test)]
mod tests {
    use super::*;


    fn h(v: &[(&str, &str)]) -> Vec<(String, String)> { v.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect() }

    #[test]
    fn publishes_only_graphs_edited_in_staging() {
        let m = h(&[("urn:chorus:domains:domains", "a"), ("urn:chorus:ontology", "o")]);
        let prod = h(&[("urn:chorus:domains:domains", "a"), ("urn:chorus:ontology", "o")]);
        let stg = h(&[("urn:chorus:domains:domains", "CLEANED"), ("urn:chorus:ontology", "o")]);
        assert_eq!(plan_publish(&m, &prod, &stg).unwrap(), vec!["urn:chorus:domains:domains"]);
    }

    #[test]
    fn negative_proof_prod_written_after_the_copy_refuses_the_publish() {
        let m = h(&[("urn:chorus:domains:services", "a")]);
        let prod = h(&[("urn:chorus:domains:services", "HARVESTED-SINCE")]);
        let stg = h(&[("urn:chorus:domains:services", "CLEANED")]);
        let e = plan_publish(&m, &prod, &stg).unwrap_err();
        assert!(e.contains("urn:chorus:domains:services: prod changed since the copy"), "{e}");
    }

    #[test]
    fn negative_proof_a_live_graph_edited_in_staging_is_refused() {
        let m = h(&[("urn:chorus:domains:cards", "a")]);
        let e = plan_publish(&m, &m, &h(&[("urn:chorus:domains:cards", "EDITED")])).unwrap_err();
        assert!(e.contains("live graph"), "{e}");
    }

    #[test]
    fn the_swap_is_one_request_and_rollback_reverses_it() {
        let g = vec!["urn:chorus:domains:domains".to_string()];
        assert_eq!(swap_sparql(&g), "MOVE SILENT <urn:chorus:domains:domains> TO <urn:chorus:previous:domains:domains> ; MOVE <urn:chorus:incoming:domains:domains> TO <urn:chorus:domains:domains>");
        assert_eq!(rollback_sparql(&g), "MOVE <urn:chorus:previous:domains:domains> TO <urn:chorus:domains:domains>");
    }


    #[test]
    fn an_escaped_backslash_before_a_quote_still_closes_the_literal() {
        // "a\\" is a literal ending in one backslash; the _:x after it IS a bnode
        let a = "<s> <p> \"a\\\\\" .\n_:x <q> <o> .\n";
        let b = "<s> <p> \"a\\\\\" .\n_:y <q> <o> .\n";
        assert_eq!(fingerprint(a), fingerprint(b));
        assert_eq!(normalize_bnodes("<s> <p> \"a\\\\\" , _:x ."), "<s> <p> \"a\\\\\" , _:b .");
    }

    #[test]
    fn a_recopy_drops_staging_graphs_prod_no_longer_has() {
        let st: Vec<String> = ["urn:chorus:gone", "urn:chorus:kept"].iter().map(|s| s.to_string()).collect();
        let cp = vec!["urn:chorus:kept".to_string()];
        assert_eq!(stale_in_staging(&st, &cp), vec!["urn:chorus:gone"]);
    }

    #[test]
    fn copies_chorus_graphs_skips_test_results_and_other_products() {
        let all: Vec<String> = ["urn:chorus:domains:domains", "urn:chorus:domains:tests",
            "urn:gathering:music", "urn:chorus:ontology", "urn:chorus:domains:domains"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(staging_graphs(&all), vec!["urn:chorus:domains:domains", "urn:chorus:ontology"]);
    }

    // #4423 reopen. Negative proof: with the copy limited to urn:chorus:*, this
    // goes red on urn:borg:instances, the graph #4353's environment move edits.
    #[test]
    fn copies_borg_graphs_and_skips_publish_bookkeeping() {
        let all: Vec<String> = ["urn:borg:instances", "urn:borg:ontology", "urn:chorus:ontology",
            "urn:chorus:previous:ontology", "urn:chorus:incoming:urn:borg:instances", "urn:gathering:music"]
            .iter().map(|s| s.to_string()).collect();
        assert_eq!(staging_graphs(&all), vec!["urn:borg:instances", "urn:borg:ontology", "urn:chorus:ontology"]);
        let st: Vec<String> = ["urn:borg:gone", "urn:chorus:previous:ontology"].iter().map(|s| s.to_string()).collect();
        assert_eq!(stale_in_staging(&st, &[]), vec!["urn:borg:gone"]);
        assert_eq!(previous_graph("urn:borg:instances"), "urn:chorus:previous:urn:borg:instances");
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

    #[test]
    fn audit_passes_an_unchanged_surface_and_names_its_drops() {
        let p = "route /v1/a\nroute /v1/b\ndomains 61\n";
        assert!(audit_verdict(p, p, "").is_ok());
        let s = "route /v1/a\ndomains 60\n";
        let ok = audit_verdict(p, s, "route gone: /v1/b\ndomains 61 -> 60\n").unwrap();
        assert!(ok.contains("named drops: route gone: /v1/b, domains 61 -> 60"), "{ok}");
    }

    #[test]
    fn negative_proof_an_unnamed_domain_drop_fails_the_audit() {
        let p = "route /v1/a\ndomains 61\n";
        let s = "route /v1/a\ndomains 49\n";
        let e = audit_verdict(p, s, "").unwrap_err();
        assert!(e.contains("domains 61 -> 49"), "{e}");
    }

    #[test]
    fn negative_proof_publish_refuses_without_an_audit_or_after_staging_moved() {
        assert!(audited_matches(None, "g\th").unwrap_err().contains("no passing audit"));
        assert!(audited_matches(Some("g\th1"), "g\th2").unwrap_err().contains("changed since its audit"));
        assert!(audited_matches(Some("g\th"), "g\th").is_ok());
    }

    #[test]
    fn quality_report_reads_prod_to_staging_per_count() {
        let p: Vec<String> = ["118", "20", "47", "3"].iter().map(|s| s.to_string()).collect();
        let s: Vec<String> = ["0", "0", "47", "0"].iter().map(|s| s.to_string()).collect();
        let r = quality_report(&p, &s);
        assert!(r.contains("rows the API can't open (urn: names): 118 -> 0"), "{r}");
        assert!(r.contains("domains named *-domain: 3 -> 0"), "{r}");
    }
}
