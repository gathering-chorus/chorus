// @test-type: unit
// @domain: athena
//! #4515 — `athena-validate migration`, the V1→V2 per-domain stage matrix ported
//! from the retired MCP tool chorus_migration_readout (#3594). The load-bearing
//! invariant is honesty: athena-make-run and v1-code-retired render as — on every
//! row and are never faked, `done` is never ✓ while a stage is unproven, and a
//! store that cannot be read exits 2 without printing a matrix.
//!
//! The pure functions are driven directly; the binary is driven against a
//! loopback fake store and a dead port. Nothing reaches the real store.

#[allow(dead_code)]
mod migration {
    include!("../src/migration.rs");
}

use migration::{compute_rows, parse_domains, render, src_domains_in, Row};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::process::Command;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

fn data_rows(out: &str) -> Vec<&str> {
    out.lines().skip(3).take_while(|l| !l.is_empty()).collect()
}

// ---- ported from platform/mcp-server/tests/migration-readout.test.ts ----

#[test]
fn compute_rows_graph_and_source_stages_real_uninstrumented_null_done_false() {
    let rows = compute_rows(&s(&["search", "code"]), &s(&["code"]));
    assert_eq!(rows.iter().map(|r| r.domain.as_str()).collect::<Vec<_>>(), vec!["code", "search"]);
    let code = &rows[0];
    let search = &rows[1];
    assert_eq!(code.created, Some(true));
    assert_eq!(code.loaded, Some(true));
    assert_eq!(code.owl_src, Some(true));
    assert_eq!(search.owl_src, Some(false));
    assert!(rows.iter().all(|r| r.v1_retired == Some(true)));
    assert!(rows.iter().all(|r| r.athena_make.is_none() && r.v1_code.is_none()));
    assert!(rows.iter().all(|r| r.done == Some(false)));
}

#[test]
fn render_per_domain_matrix_honest_cells_real_counts_deterministic() {
    let out = render(&compute_rows(&s(&["code", "search"]), &s(&["code"])));
    assert!(out.contains("per-domain stage matrix"));
    assert!(out.contains("  domain            created owl-src loaded athena-make v1-retired v1-code done"));
    assert!(out.contains("\n  code "));
    assert!(out.contains("\n  search "));
    assert!(out.contains('—'));
    let rows = data_rows(&out);
    assert_eq!(rows.len(), 2, "{out}");
    assert!(rows.iter().all(|l| l.trim_end().ends_with('·')), "{out}");
    assert!(out.contains("2 domains — created 2/2 · owl-src 1/2 · loaded 2/2 · v1-retired 2/2"), "{out}");
    assert!(out.contains("not-yet-instrumented (athena-make-run, v1-code-retired)"));
    let r = || render(&compute_rows(&s(&["x"]), &[]));
    assert_eq!(r(), r());
}

#[test]
fn empty_graph_degrades_honestly() {
    let out = render(&compute_rows(&[], &[]));
    assert!(out.contains("0 domains — created 0/0"), "{out}");
    assert!(data_rows(&out).is_empty());
}

// ---- negative proofs ----

/// The athena-make and v1-code columns (5th and 7th cells) are —, never ✓.
#[test]
fn negative_proof_an_uninstrumented_stage_prints_dash_not_check() {
    let out = render(&compute_rows(&s(&["code"]), &s(&["code"])));
    let cells: Vec<&str> = data_rows(&out)[0].split_whitespace().collect();
    // domain created owl-src loaded athena-make v1-retired v1-code done
    assert_eq!(cells, vec!["code", "✓", "✓", "✓", "—", "✓", "—", "·"], "{out}");
}

#[test]
fn negative_proof_a_domain_with_no_owl_source_shows_dot_for_owl_src() {
    let out = render(&compute_rows(&s(&["orphan"]), &[]));
    let cells: Vec<&str> = data_rows(&out)[0].split_whitespace().collect();
    assert_eq!(cells[2], "·", "{out}");
    assert!(out.contains("owl-src 0/1"), "{out}");
}

#[test]
fn a_fully_proven_row_would_render_check_in_done() {
    // The renderer is not hard-wired to ·: a real ✓ would show. Proves the
    // done-column assertions above can tell the two states apart.
    let r = Row {
        domain: "x".into(),
        created: Some(true),
        owl_src: Some(true),
        loaded: Some(true),
        athena_make: Some(true),
        v1_retired: Some(true),
        v1_code: Some(true),
        done: Some(true),
    };
    let out = render(&[r]);
    assert!(data_rows(&out)[0].ends_with('✓'), "{out}");
    assert!(!data_rows(&out)[0].contains('—'), "{out}");
}

// ---- source and store parsing ----

#[test]
fn src_domains_reads_the_declaration_pattern_only() {
    let ttl = "chorus:cards a chorus:Domain ;\n  chorus:hasOwner chorus:wren .\nchorus:spine  a chorus:Domain .\n";
    assert_eq!(src_domains_in(ttl), s(&["cards", "spine"]));
}

#[test]
fn parse_domains_header_only_is_zero_domains_no_header_is_none() {
    assert_eq!(parse_domains("s\r\n"), Some(vec![]));
    assert_eq!(
        parse_domains("s\r\nhttps://jeffbridwell.com/chorus#cards\r\nhttps://jeffbridwell.com/chorus#spine\r\n"),
        Some(s(&["cards", "spine"]))
    );
    assert_eq!(parse_domains(""), None);
    assert_eq!(parse_domains("<html>error</html>"), None);
}

// ---- the binary ----

fn bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let b = exe.parent().unwrap().parent().unwrap().join("athena-validate");
    assert!(b.is_file(), "athena-validate not built at {}", b.display());
    b
}

/// A loopback store answering every request with `status` and `body`.
fn fake_store(status: &'static str, body: &'static str) -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        for st in l.incoming().flatten() {
            let mut st = st;
            let mut buf = [0u8; 8192];
            let _ = st.read(&mut buf);
            let _ = write!(
                st,
                "HTTP/1.1 {status}\r\nContent-Type: text/csv\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    format!("http://{addr}/query")
}

fn run_bin(endpoint: &str, root: &str) -> (i32, String) {
    let out = Command::new(bin())
        .arg("migration")
        .env("FUSEKI_QUERY", endpoint)
        .env("CHORUS_ROOT", root)
        .env("ATHENA_VALIDATE_NUDGE", "0")
        .output()
        .unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8_lossy(&out.stdout).to_string())
}

fn root_with_source() -> String {
    let dir = std::env::temp_dir().join(format!("av-migration-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("roles/silas/ontology")).unwrap();
    std::fs::write(dir.join("roles/silas/ontology/chorus.ttl"), "chorus:cards a chorus:Domain .\n").unwrap();
    dir.to_string_lossy().to_string()
}

#[test]
fn negative_proof_an_unreadable_store_exits_2_and_prints_no_matrix() {
    let root = root_with_source();
    for ep in [
        "http://127.0.0.1:9/query".to_string(),
        fake_store("500 Internal Server Error", ""),
        fake_store("200 OK", ""),
    ] {
        let (code, out) = run_bin(&ep, &root);
        assert_eq!(code, 2, "{ep}: {out}");
        assert!(out.contains("UNMEASURED"), "{ep}: {out}");
        assert!(!out.contains("stage matrix"), "{ep}: {out}");
        assert!(!out.contains("domains —"), "{ep}: {out}");
    }
}

#[test]
fn a_readable_store_prints_the_matrix_and_exits_0() {
    let root = root_with_source();
    let ep = fake_store(
        "200 OK",
        "s\r\nhttps://jeffbridwell.com/chorus#spine\r\nhttps://jeffbridwell.com/chorus#cards\r\n",
    );
    let (code, out) = run_bin(&ep, &root);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("2 domains — created 2/2 · owl-src 1/2"), "{out}");
    let rows = data_rows(&out);
    assert!(rows[0].starts_with("  cards "), "{out}");
    assert!(rows[1].starts_with("  spine "), "{out}");
    // the three absent source files are named, not silent
    assert_eq!(out.matches("note: source unreadable").count(), 3, "{out}");
}
