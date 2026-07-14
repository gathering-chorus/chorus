//! owl-api CLI (#3350): generate | serve.
//!
//!   owl-api generate [--class Domain]            → prints routes.json (the artifact)
//!   owl-api serve [--class Domain] [--port 3360] → generates, then serves

use owl_api::{all_vocab_classes, dal_skeleton_ts, dashboards_json, generate, generate_product_index, generate_verb, mcp_binding, openapi_json, page_html, routes_json, serve, tests_manifest};
use std::process::ExitCode;

fn arg(args: &[String], flag: &str, default: &str) -> String {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1).cloned()).unwrap_or_else(|| default.to_string())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let class = arg(&args, "--class", "Domain");
    match args.first().map(String::as_str) {
        Some("generate") => match generate(&class) {
            Ok(t) => {
                print!("{}", routes_json(&t));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("owl-api: {}", e);
                ExitCode::FAILURE
            }
        },
        Some("generate-openapi") => match generate(&class) {
            Ok(t) => {
                print!("{}", openapi_json(&t));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("owl-api: {}", e);
                ExitCode::FAILURE
            }
        },
        Some("generate-dashboard") => match generate(&class) {
            Ok(t) => {
                print!("{}", dashboards_json(&t));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("owl-api: {}", e);
                ExitCode::FAILURE
            }
        },
        Some("generate-page") => match generate(&class) {
            Ok(t) => {
                print!("{}", page_html(&t));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("owl-api: {}", e);
                ExitCode::FAILURE
            }
        },
        Some("generate-tests") => match generate(&class) {
            Ok(t) => {
                print!("{}", tests_manifest(&t));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("owl-api: {}", e);
                ExitCode::FAILURE
            }
        },
        Some("generate-mcp") => match generate(&class) {
            Ok(t) => {
                print!("{}", mcp_binding(&t));
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("owl-api: {}", e);
                ExitCode::FAILURE
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
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("owl-api: {}", e);
                    ExitCode::FAILURE
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
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("owl-api: {}", e);
                ExitCode::FAILURE
            }
        },
        // #3488 — print the resolved repo land location (chorus:repoTarget or
        // class-keyed default) so the land/drift scripts know WHERE to write +
        // diff this class's generated artifacts. The config-as-data location.
        Some("generate-target") => match generate(&class) {
            Ok(t) => {
                println!("{}", t.repo_target);
                ExitCode::SUCCESS
            }
            Err(e) => {
                eprintln!("owl-api: {}", e);
                ExitCode::FAILURE
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
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("owl-api: {}", e);
                    ExitCode::FAILURE
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
                                eprintln!("owl-api: + {} API (model-declared)", c);
                                tables.push(t);
                            }
                            Err(e) => eprintln!("owl-api: refuse {} ({})", c, e),
                        }
                    }
                }
                Err(e) => eprintln!("owl-api: vocabulary read failed ({})", e),
            }
            if tables.is_empty() {
                eprintln!("owl-api: no classes generated — nothing to serve");
                return ExitCode::FAILURE;
            }
            match serve(port, &tables) {
                Ok(()) => ExitCode::SUCCESS,
                Err(e) => {
                    eprintln!("owl-api: {}", e);
                    ExitCode::FAILURE
                }
            }
        }
        _ => {
            eprintln!("usage: owl-api generate|generate-dashboard|generate-openapi|generate-page|generate-tests|generate-mcp|generate-target [--class Domain] | owl-api generate-product [--product athena] | owl-api serve [--class Domain] [--port 3360]");
            ExitCode::FAILURE
        }
    }
}
