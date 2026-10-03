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
    // #4338 — the land compares what athena-make serves before the deploy and
    // after the restart, and fails on any drop (a route gone, fewer Domains).
    if args.get(1).map(String::as_str) == Some("served-snapshot") {
        match (args.get(2), args.get(3)) {
            (Some(api), Some(out)) => match athena_deploy::read_served(api)
                .and_then(|s| std::fs::write(out, &s).map(|_| s).map_err(|e| format!("{out}: {e}"))) {
                Ok(s) => { println!("served-snapshot: {} route(s), {}", s.lines().count() - 1, s.lines().last().unwrap_or("")); std::process::exit(0); }
                Err(e) => { eprintln!("athena-deploy: {}", e); std::process::exit(1); }
            },
            _ => { eprintln!("usage: athena-deploy served-snapshot <api> <out-file>"); std::process::exit(2); }
        }
    }
    if args.get(1).map(String::as_str) == Some("served-compare") {
        match (args.get(2), args.get(3)) {
            (Some(before), Some(api)) => {
                let before = std::fs::read_to_string(before).unwrap_or_else(|e| {
                    eprintln!("athena-deploy: served-compare: no before snapshot at {before}: {e} — unmeasured"); std::process::exit(1)
                });
                let after = athena_deploy::read_served(api).unwrap_or_else(|e| { eprintln!("athena-deploy: {}", e); std::process::exit(1) });
                let all = athena_deploy::served_drops(&before, &after);
                let allow = std::env::var("ATHENA_ALLOW_DROPS").unwrap_or_default();
                for d in all.iter().filter(|d| allow.lines().any(|l| l.trim() == d.as_str())) {
                    println!("served-compare: allowed by ATHENA_ALLOW_DROPS: {d}");
                }
                let drops = athena_deploy::unapproved_drops(all, &allow);
                if drops.is_empty() {
                    println!("served-compare: nothing served before is missing ({})", after.lines().last().unwrap_or(""));
                    std::process::exit(0);
                }
                eprintln!("athena-deploy: REFUSED — this land took away what was served before it (#4338):");
                for d in &drops { eprintln!("  {d}"); }
                std::process::exit(1);
            }
            _ => { eprintln!("usage: athena-deploy served-compare <before-file> <api>"); std::process::exit(2); }
        }
    }
    // #4338 — an argument this binary does not know is refused, never read as
    // "deploy everything". A 15:52 binary handed `served-snapshot` would have run a
    // full prod deploy and exited 0.
    if let Some(a) = args.get(1) {
        eprintln!("athena-deploy: unknown argument '{a}' — refusing (a bare `athena-deploy` deploys; subcommands: scope, prove-trace, served-snapshot, served-compare)");
        std::process::exit(2);
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
