//! athena-make CLI (#3350): generate | serve.
//!
//!   athena-make generate [--class Domain]            → prints routes.json (the artifact)
//!   athena-make serve [--class Domain] [--port 3360] → generates, then serves

use athena_make::{all_vocab_classes, dal_skeleton_ts, dashboards_json, generate, generate_product_index, generate_verb, mcp_binding, openapi_json, page_html, routes_json, serve, tests_manifest};
use std::process::ExitCode;

fn arg(args: &[String], flag: &str, default: &str) -> String {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned()).unwrap_or_else(|| default.to_string())
}

fn main() -> ExitCode {
    ExitCode::from(run(std::env::args().skip(1).collect()))
}

/// #3701 — the CLI body, extracted behavior-preserving from main() so the
/// subcommand dispatch is unit-testable (main() is a one-line shim; 0=success,
/// 1=failure, mapped to ExitCode there — ExitCode itself has no PartialEq).
fn run(args: Vec<String>) -> u8 {
    let class = arg(&args, "--class", "Domain");
    match args.first().map(String::as_str) {
        Some("generate") => match generate(&class) {
            Ok(t) => {
                print!("{}", routes_json(&t));
                0
            }
            Err(e) => {
                eprintln!("athena-make: {}", e);
                1
            }
        },
        Some("generate-openapi") => match generate(&class) {
            Ok(t) => {
                print!("{}", openapi_json(&t));
                0
            }
            Err(e) => {
                eprintln!("athena-make: {}", e);
                1
            }
        },
        Some("generate-dashboard") => match generate(&class) {
            Ok(t) => {
                print!("{}", dashboards_json(&t));
                0
            }
            Err(e) => {
                eprintln!("athena-make: {}", e);
                1
            }
        },
        Some("generate-page") => match generate(&class) {
            Ok(t) => {
                print!("{}", page_html(&t));
                0
            }
            Err(e) => {
                eprintln!("athena-make: {}", e);
                1
            }
        },
        Some("generate-tests") => match generate(&class) {
            Ok(t) => {
                print!("{}", tests_manifest(&t));
                0
            }
            Err(e) => {
                eprintln!("athena-make: {}", e);
                1
            }
        },
        Some("generate-mcp") => match generate(&class) {
            Ok(t) => {
                print!("{}", mcp_binding(&t));
                0
            }
            Err(e) => {
                eprintln!("athena-make: {}", e);
                1
            }
        },
        // #3551 — the `verb` make-target: read a VerbShape instance from the graph and
        // emit the GENERATED half of the verb crate (<verb>_generated.rs). Same
        // read-shape → emit spine as the api/mcp/page targets, pointed at a VerbShape.
        Some("generate-verb") => {
            let verb = arg(&args, "--verb", "athena-deploy");
            match generate_verb(&verb) {
                Ok(code) => {
                    print!("{}", code);
                    0
                }
                Err(e) => {
                    eprintln!("athena-make: {}", e);
                    1
                }
            }
        }
        // #3567 SPIKE — the `dal` make-target: emit a STANDALONE per-product TS
        // write-edge for a class, projected from its shape (generate()→RouteTable
        // reused; only the emitter is new). Scope is the per-product generate-time
        // param; for the PoC it derives from the class's instances graph (where the
        // writes land). Same read-shape → emit spine as the verb/mcp targets, Rust→TS.
        Some("generate-dal") => match generate(&class) {
            Ok(t) => {
                let scope = vec![t.instances_graph.clone()];
                print!("{}", dal_skeleton_ts(&t, &scope));
                0
            }
            Err(e) => {
                eprintln!("athena-make: {}", e);
                1
            }
        },
        // #3488 — print the resolved repo land location (chorus:repoTarget or
        // class-keyed default) so the land/drift scripts know WHERE to write +
        // diff this class's generated artifacts. The config-as-data location.
        Some("generate-target") => match generate(&class) {
            Ok(t) => {
                println!("{}", t.repo_target);
                0
            }
            Err(e) => {
                eprintln!("athena-make: {}", e);
                1
            }
        },
        // #3488 — the PRODUCT API index: the aggregate of the product's domains,
        // derived from its hasDomain edges. Binding domain→product is automation,
        // not a manual register — regenerate and the binding follows the graph.
        Some("generate-product") => {
            let product = arg(&args, "--product", "athena");
            match generate_product_index(&product) {
                Ok(idx) => {
                    println!("{}", idx);
                    0
                }
                Err(e) => {
                    eprintln!("athena-make: {}", e);
                    1
                }
            }
        }
        Some("serve") => {
            let port: u16 = arg(&args, "--port", "3360").parse().unwrap_or(3360);
            // #3640 (ADR-051) — the serve list IS the model: mount exactly the
            // classes some domain definesVocabulary. The #3466 hardcoded candidate
            // array is DELETED — a class with no declaring domain does not mount,
            // and generate() refuses it (no silent instance-graph fallback). Adding
            // a domain+vocabulary edge to the model adds its API with zero code
            // change; nothing serves by accident.
            let mut tables = Vec::new();
            match all_vocab_classes() {
                Ok(vocab) => {
                    for c in vocab {
                        match generate(&c) {
                            Ok(t) => {
                                eprintln!("athena-make: + {} API (model-declared)", c);
                                tables.push(t);
                            }
                            Err(e) => eprintln!("athena-make: refuse {} ({})", c, e),
                        }
                    }
                }
                Err(e) => eprintln!("athena-make: vocabulary read failed ({})", e),
            }
            if tables.is_empty() {
                eprintln!("athena-make: no classes generated — nothing to serve");
                return 1;
            }
            match serve(port, &tables) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("athena-make: {}", e);
                    1
                }
            }
        }
        _ => {
            eprintln!("usage: athena-make generate|generate-dashboard|generate-openapi|generate-page|generate-tests|generate-mcp|generate-target [--class Domain] | athena-make generate-product [--product athena] | athena-make serve [--class Domain] [--port 3360]");
            1
        }
    }
}

// #3701 — hermetic CLI tests: an in-test SPARQL stub (std TcpListener, port 0)
// stands in for Fuseki; CHORUS_FUSEKI env selects it. ALL env mutation happens
// inside the ONE #[test] fn below (cargo runs tests in parallel threads; a
// single fn serializes by construction — the only other test here is pure).
#[cfg(test)]
mod cli_tests {
    use super::*;
    use std::io::Write;
    use std::net::TcpListener;

    // arg() assertions live INSIDE the one #[test] below — a second test fn,
    // even a pure one, would make the test binary multi-threaded while it
    // set_vars (gate:code finding, run 2687d8401bf3). One test = single-
    // threaded by construction.
    fn assert_arg_parses_flag_value_and_defaults() {
        let args: Vec<String> = vec!["generate".into(), "--class".into(), "Product".into()];
        assert_eq!(arg(&args, "--class", "Domain"), "Product");
        assert_eq!(arg(&args, "--port", "3360"), "3360");
        // flag present but value missing → default
        let dangling: Vec<String> = vec!["generate".into(), "--class".into()];
        assert_eq!(arg(&dangling, "--class", "Domain"), "Domain");
    }

    /// Just enough model for generate("Domain") / generate-verb / generate-product
    /// to succeed (mirrors the tests/coverage_3701_hermetic.rs fixture, trimmed).
    fn rows_for(q: &str) -> Vec<String> {
        let s = |v: &str| v.to_string();
        if q.contains("OPTIONAL { ?p sh:datatype") {
            return if q.contains("#Domain>") {
                vec![s("comment|datatype:string"), s("label|plain"), s("ownedBy|edge:Role")]
            } else {
                vec![]
            };
        }
        if q.contains("chorus:requiresAuth") || q.contains("chorus:exposure") || q.contains("chorus:treeEdge") || q.contains("chorus:treeOrder") || q.contains("chorus:repoTarget") || q.contains("#repoTarget>") {
            return vec![];
        }
        if q.contains("sh:minCount") {
            return vec![s("comment|field")];
        }
        if q.contains("chorus:atStep") {
            return vec![s("value-stream-step-designing")];
        }
        if q.contains("chorus:partOf ?t") {
            return vec![s("loom")];
        }
        if q.contains("chorus:instancesGraph") {
            return vec![s("urn:test:instances")];
        }
        if q.contains("SELECT DISTINCT ?v") && q.contains("definesVocabulary ?c") {
            return vec![s("Domain"), s("Nope")]; // Nope has no shape → serve's refuse arm
        }
        if q.contains("definesVocabulary <") {
            return vec![s("athena")];
        }
        if q.contains("#verbFamily>") {
            return vec![s("athena")];
        }
        if q.contains("#invocability>") {
            return vec![s("invoked")];
        }
        if q.contains("#verbInput>") {
            return vec![s("card|datatype:integer")];
        }
        if q.contains("#verbOutput>") || q.contains("#verbEdge>") {
            return vec![];
        }
        if q.contains("chorus:hasDomain ?d") {
            return vec![s("borg")];
        }
        vec![]
    }

    fn urldecode(s: &str) -> String {
        let mut out = Vec::new();
        let b = s.as_bytes();
        let mut i = 0;
        while i < b.len() {
            match b[i] {
                b'+' => {
                    out.push(b' ');
                    i += 1;
                }
                b'%' if i + 2 < b.len() => {
                    if let Ok(v) = u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or(""), 16) {
                        out.push(v);
                        i += 3;
                    } else {
                        out.push(b[i]);
                        i += 1;
                    }
                }
                c => {
                    out.push(c);
                    i += 1;
                }
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }

    fn start_stub() -> u16 {
        let l = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = l.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for c in l.incoming() {
                let Ok(mut c) = c else { continue };
                std::thread::spawn(move || {
                    let _ = c.set_read_timeout(Some(std::time::Duration::from_secs(5)));
                    let req = athena_make::read_http_request(&mut c, 1 << 20);
                    let body = req.splitn(2, "\r\n\r\n").nth(1).unwrap_or("");
                    // #4022 — the client now POSTs a raw application/sparql-query
                    // body (no `query=` form field); the stub answers both shapes.
                    let raw_sparql = req.to_ascii_lowercase().contains("content-type: application/sparql-query");
                    let rows = if raw_sparql {
                        Some(rows_for(body))
                    } else {
                        body.find("query=").map(|i| rows_for(&urldecode(&body[i + 6..])))
                    }
                        .unwrap_or_default();
                    let bindings = rows
                        .iter()
                        .map(|v| format!("{{\"v\":{{\"value\":\"{}\"}}}}", v))
                        .collect::<Vec<_>>()
                        .join(",");
                    let rb = format!("{{\"results\":{{\"bindings\":[{}]}}}}", bindings);
                    let resp = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        rb.len(),
                        rb
                    );
                    let _ = c.write_all(resp.as_bytes());
                });
            }
        });
        port
    }

    #[test]
    fn run_covers_every_subcommand_branch() {
        // ---- pure arg() parsing (no env) ----
        assert_arg_parses_flag_value_and_defaults();

        // ---- usage (no / unknown subcommand) ----
        assert_eq!(run(vec![]), 1);
        assert_eq!(run(vec!["frobnicate".into()]), 1);

        // ---- Err arms: point CHORUS_FUSEKI at a dead port (curl -f fails fast) ----
        let dead = {
            // bind-then-drop: the OS port is closed → connection refused, deterministic
            let l = TcpListener::bind("127.0.0.1:0").unwrap();
            let p = l.local_addr().unwrap().port();
            drop(l);
            p
        };
        std::env::set_var("CHORUS_FUSEKI", format!("http://127.0.0.1:{}", dead));
        for sub in [
            "generate",
            "generate-openapi",
            "generate-dashboard",
            "generate-page",
            "generate-tests",
            "generate-mcp",
            "generate-verb",
            "generate-dal",
            "generate-target",
            "generate-product",
        ] {
            assert_eq!(run(vec![sub.to_string()]), 1, "{} errs on dead Fuseki", sub);
        }
        // serve: vocabulary read fails → zero tables → refuse to serve
        assert_eq!(run(vec!["serve".into()]), 1);

        // ---- Ok arms: the in-test stub answers the shape queries ----
        let port = start_stub();
        std::env::set_var("CHORUS_FUSEKI", format!("http://127.0.0.1:{}", port));
        for sub in [
            "generate",
            "generate-openapi",
            "generate-dashboard",
            "generate-page",
            "generate-tests",
            "generate-mcp",
            "generate-dal",
            "generate-target",
        ] {
            assert_eq!(
                run(vec![sub.to_string(), "--class".into(), "Domain".into()]),
                0,
                "{} succeeds against the stub model",
                sub
            );
        }
        assert_eq!(run(vec!["generate-verb".into(), "--verb".into(), "athena-deploy".into()]), 0);
        assert_eq!(run(vec!["generate-product".into(), "--product".into(), "athena".into()]), 0);
        // serve: tables generate (Domain ok, Nope refused → the refuse arm), then
        // the privileged port 1 refuses to bind as non-root → serve() Err arm.
        assert_eq!(run(vec!["serve".into(), "--port".into(), "1".into()]), 1);
        // bad --port falls back to 3360 via unwrap_or; prove the parse-fallback
        // WITHOUT binding 3360 by making generation fail first (dead Fuseki again).
        std::env::set_var("CHORUS_FUSEKI", format!("http://127.0.0.1:{}", dead));
        assert_eq!(run(vec!["serve".into(), "--port".into(), "notaport".into()]), 1);
    }
}
