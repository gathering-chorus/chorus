//! chorus-make generate <pipeline> <rows.tsv>            write the workflow to stdout
//! chorus-make check    <pipeline> <rows.tsv> <file>     exit 1 on drift (pre-commit / pipeline)
//! chorus-make graph    <pipeline>                       read the rows from the store (FUSEKI_QUERY), write the workflow
//! chorus-make walk     <stream label>                   walk stream → steps → pipeline → skills → domains; exit 1 on a broken link (AC7)
//! chorus-make parity   <rows.tsv> <mcp server.ts>       exit 1 when MCP and the skills wrap different verbs (ADR-062 §9)
use std::process::exit;

fn read(p: &str) -> String {
    std::fs::read_to_string(p).unwrap_or_else(|e| {
        eprintln!("chorus-make: cannot read {p}: {e}");
        exit(2)
    })
}

fn rows(p: &str) -> Vec<chorus_make::Row> {
    chorus_make::parse_rows(&read(p)).unwrap_or_else(|errs| {
        for e in errs {
            eprintln!("chorus-make: {e}");
        }
        exit(2)
    })
}

/// One SELECT against the store (FUSEKI_QUERY), TSV back. A refusal is exit 2.
fn sparql(query: &str) -> String {
    let endpoint = std::env::var("FUSEKI_QUERY").unwrap_or_else(|_| "http://localhost:3030/pods/query".into());
    let out = std::process::Command::new("curl")
        .args(["-sfS", "--max-time", "60", "-H", "Accept: text/tab-separated-values", "--data-urlencode"])
        .arg(format!("query={query}"))
        .arg(&endpoint)
        .output()
        .unwrap_or_else(|e| { eprintln!("chorus-make: curl: {e}"); exit(2) });
    if !out.status.success() {
        eprintln!("chorus-make: {endpoint} refused the query: {}", String::from_utf8_lossy(&out.stderr).trim());
        exit(2);
    }
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let usage = "usage: chorus-make generate <pipeline> <rows.tsv> | check <pipeline> <rows.tsv> <file> | graph <pipeline> | parity <rows.tsv> <server.ts> | walk <stream>";
    let text = |pipeline: &str, tsv: &str| {
        chorus_make::generate(pipeline, &rows(tsv)).unwrap_or_else(|errs| {
            for e in errs {
                eprintln!("chorus-make: refused: {e}");
            }
            exit(1)
        })
    };
    match a.get(1).map(String::as_str) {
        Some("generate") if a.len() == 4 => print!("{}", text(&a[2], &a[3])),
        Some("check") if a.len() == 5 => {
            if let Err(e) = chorus_make::drift(&text(&a[2], &a[3]), &read(&a[4])) {
                eprintln!("chorus-make: {} — {e}", a[4]);
                exit(1);
            }
            println!("chorus-make: {} matches the graph", a[4]);
        }
        Some("walk") if a.len() == 3 => {
            let tsv = sparql(&chorus_make::walk_query(&a[2]));
            let w = chorus_make::walk_from_sparql_tsv(&tsv).unwrap_or_else(|errs| {
                for e in errs { eprintln!("chorus-make: {e}"); }
                exit(1)
            });
            println!("{} ← pipeline {}", a[2], w.pipeline);
            for r in &w.rows {
                println!("  {:<10} {:<8} {:>2} {:<32} {}", r.stream_step, r.pipeline_step, r.order, r.skill, r.domain);
            }
            if !w.uncovered.is_empty() { println!("  outside the pipeline: {}", w.uncovered.join(", ")); }
            if let Err(errs) = chorus_make::walk_check(&w) {
                for e in errs { eprintln!("chorus-make: {e}"); }
                exit(1);
            }
        }
        Some("graph") if a.len() == 3 => {
            let rows = chorus_make::rows_from_sparql_tsv(&sparql(&chorus_make::rows_query(&a[2]))).unwrap_or_else(|errs| {
                for e in errs { eprintln!("chorus-make: {e}"); }
                exit(1)
            });
            match chorus_make::generate(&a[2], &rows) {
                Ok(t) => print!("{t}"),
                Err(errs) => { for e in errs { eprintln!("chorus-make: refused: {e}"); } exit(1) }
            }
        }
        Some("parity") if a.len() == 4 => {
            match chorus_make::verb_parity(&chorus_make::skill_verbs(&rows(&a[2])), &chorus_make::mcp_verbs(&read(&a[3]))) {
                Ok(()) => println!("chorus-make: MCP and the skills wrap the same verbs"),
                Err(e) => { eprintln!("chorus-make: {e}"); exit(1) }
            }
        }
        _ => {
            eprintln!("{usage}");
            exit(2)
        }
    }
}
