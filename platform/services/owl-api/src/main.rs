//! owl-api CLI (#3350): generate | serve.
//!
//!   owl-api generate [--class Domain]            → prints routes.json (the artifact)
//!   owl-api serve [--class Domain] [--port 3360] → generates, then serves

use owl_api::{dashboards_json, generate, generate_product_index, mcp_binding, openapi_json, page_html, routes_json, serve, tests_manifest};
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
            match generate(&class).and_then(|t| serve(port, &t)) {
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
