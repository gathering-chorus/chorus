//! owl-api CLI (#3350): generate | serve.
//!
//!   owl-api generate [--class Domain]            → prints routes.json (the artifact)
//!   owl-api serve [--class Domain] [--port 3360] → generates, then serves

use owl_api::{dashboards_json, generate, routes_json, serve};
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
            eprintln!("usage: owl-api generate|generate-dashboard [--class Domain] | owl-api serve [--class Domain] [--port 3360]");
            ExitCode::FAILURE
        }
    }
}
