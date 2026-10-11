//! borg-prove (#4456) — the I/O half. Modes match the Proving skills:
//!   intake|verify|stage|verdict|prod|record <card> <role>
//!   route <card> <role> <dagu-run-id> <step=route,…>
//! Exit: 0 ok, 1 refused/failed (the step fails, so its onFail routes it).
use borg_prove::*;
use std::process::{exit, Command};

fn env(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

fn curl(args: &[&str]) -> Option<String> {
    let out = Command::new("curl").args(["-sf", "--max-time", "15"]).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn sparql(query: &str, accept: &str) -> Option<String> {
    let q = format!("query={query}");
    curl(&["-H", &format!("Accept: {accept}"), "--data-urlencode", &q, &env("FUSEKI_QUERY", "http://localhost:3030/pods/query")])
}

/// What the running system answers for one check. Read-only: GETs and ASKs.
fn probe(c: &Check, base: &str) -> Seen {
    match c.kind.as_str() {
        "http" => {
            let url = if c.target.starts_with("http") { c.target.clone() } else { format!("{base}{}", c.target) };
            curl(&[&url])
        }
        "sparql" => sparql(&c.target, "application/sparql-results+json"),
        "test" => {
            // the latest recorded result of the named test, as the store holds it
            let q = format!(
                "PREFIX chorus: <https://jeffbridwell.com/chorus#> SELECT ?status WHERE {{ GRAPH ?g {{ ?r a chorus:TestResult ; chorus:testName {:?} ; chorus:status ?status ; chorus:ranAt ?t }} }} ORDER BY DESC(?t) LIMIT 1",
                c.target
            );
            sparql(&q, "text/tab-separated-values")
        }
        _ => None,
    }
}

fn checks(card: u32) -> Vec<Check> {
    let Some(tsv) = sparql(&checks_query(card), "text/tab-separated-values") else {
        eprintln!("borg-prove: the store did not answer for #{card}'s CardAC rows");
        exit(1);
    };
    checks_from_tsv(&tsv).unwrap_or_else(|e| {
        eprintln!("borg-prove: {e}");
        exit(1)
    })
}

fn run_checks(card: u32, base: &str) -> Vec<(Check, Outcome)> {
    let cs = checks(card);
    let results: Vec<(Check, Outcome)> = cs.into_iter().map(|c| { let o = judge(&c, &probe(&c, base)); (c, o) }).collect();
    for (c, o) in &results {
        println!("{:<12} {:<10} {}\n             {}", c.ac, o.word(), c.statement, o.evidence());
    }
    results
}

/// The step that failed in a dagu run, from `dagu status` ("├─name (…) [failed]").
fn failed_step(dag: &str, run: &str) -> String {
    let cfg = env("DAGU_CONFIG", &format!("{}/platform/config/dagu.yaml", env("CHORUS_HOME", "")));
    let out = Command::new("dagu").args(["status", dag, "--run-id", run, "--config", &cfg]).output();
    let text = out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default();
    failed_in_status(&text)
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let (Some(mode), Some(card)) = (a.first(), a.get(1).and_then(|c| c.trim_start_matches('#').parse::<u32>().ok())) else {
        eprintln!("usage: borg-prove intake|verify|stage|verdict|prod|record|route <card> <role> [run-id routes]");
        exit(2);
    };
    let role = a.get(2).cloned().unwrap_or_default();
    match mode.as_str() {
        "intake" => match intake(&checks(card)) {
            Ok(()) => println!("#{card} intake: every AC has a check"),
            Err(errs) => {
                eprintln!("#{card} Rejected at intake:");
                errs.iter().for_each(|e| eprintln!("  {e}"));
                exit(1);
            }
        },
        // verify and stage both read the demo copy until the werk-only probe exists
        "verify" | "stage" | "verdict" => {
            let base = env("BORG_DEMO_BASE", "http://localhost:3343");
            let r = run_checks(card, &base);
            let v = verdict(&r);
            println!("#{card} {mode}: {v}");
            if v != "Proven" { exit(1) }
        }
        "prod" => {
            let r = run_checks(card, &env("BORG_PROD_BASE", "http://localhost:3340"));
            let v = if verdict(&r) == "Proven" { "Proven" } else { "Regressed" };
            println!("#{card} verify in prod: {v}");
            if v != "Proven" { exit(1) }
        }
        "record" => println!("#{card} record: not built yet (the verdict row is the next AC)"),
        "route" => {
            let (Some(_run), Some(routes)) = (a.get(3), a.get(4)) else {
                eprintln!("usage: borg-prove route <card> <role> <dagu-run-id> <step=route,…>");
                exit(2);
            };
            // dagu names only the handler itself to the handler, so the failed
            // step is read from the run's own status (DAG_NAME + the run id)
            let failed = std::env::var("BORG_FAILED_STEP").ok().unwrap_or_else(|| failed_step(&env("DAG_NAME", ""), _run));
            let r = route_for(&failed, &parse_routes(routes), &role);
            println!("#{card} {failed:?} → {r:?}");
            if r == Route::Unrouted { exit(1) }
        }
        other => {
            eprintln!("borg-prove: unknown mode '{other}'");
            exit(2);
        }
    }
}
