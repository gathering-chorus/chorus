//! athena-deploy binary — the Athena value-stream DEPLOY verb.
//!
//! Atomic peer of werk-* (ADR-032 verb-contract-v1, ADR-037 atomic-verb-execution).
//! Deploys a domain's model (TTL) into the live ontology graph ADDITIVELY — replaces
//! only the deploying domain's own subjects, never a sibling's (fixes the #3540/#3496
//! whole-graph-COPY clobber). Distinct from werk-deploy: that ships CODE (binaries),
//! this ships the MODEL (ontology). Thin shell over the testable core.
use athena_deploy::run_athena_deploy;

fn main() {
    // #4186 — `athena-deploy scope <root> <range>`: the one place the workflows
    // ask "did this land touch the model or the seed?"
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("scope") {
        match (args.get(2), args.get(3)) {
            (Some(root), Some(range)) => match athena_deploy::scope(root, range) {
                Ok(s) => { if !s.is_empty() { println!("{}", s); } std::process::exit(0); }
                Err(e) => { eprintln!("athena-deploy: {}", e); std::process::exit(2); }
            },
            _ => { eprintln!("usage: athena-deploy scope <root> <git-range>"); std::process::exit(2); }
        }
    }
    if args.get(1).map(String::as_str) == Some("prove-trace") {
        let trace = args.get(2).cloned().unwrap_or_default();
        let want: usize = args.get(3).and_then(|w| w.parse().ok()).unwrap_or(0);
        let spine = args.iter().position(|a| a == "--spine").and_then(|i| args.get(i + 1)).cloned()
            .unwrap_or_else(|| format!("{}/.chorus/chorus.log", std::env::var("HOME").unwrap_or_default()));
        if trace.is_empty() || want == 0 { eprintln!("usage: athena-deploy prove-trace <trace> <want> [--spine <path>]"); std::process::exit(2); }
        match athena_deploy::prove_trace(&trace, want, &spine) {
            Ok(s) => { println!("{}", s); std::process::exit(0); }
            Err(e) => { eprintln!("athena-deploy: {}", e); std::process::exit(1); }
        }
    }
    match run_athena_deploy() {
        Ok(summary) => {
            println!("{}", summary);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("athena-deploy: {}", e);
            std::process::exit(1);
        }
    }
}
