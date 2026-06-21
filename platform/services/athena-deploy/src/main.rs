//! athena-deploy binary — the Athena value-stream DEPLOY verb.
//!
//! Atomic peer of werk-* (ADR-032 verb-contract-v1, ADR-037 atomic-verb-execution).
//! Deploys a domain's model (TTL) into the live ontology graph ADDITIVELY — replaces
//! only the deploying domain's own subjects, never a sibling's (fixes the #3540/#3496
//! whole-graph-COPY clobber). Distinct from werk-deploy: that ships CODE (binaries),
//! this ships the MODEL (ontology). Thin shell over the testable core.
use athena_deploy::run_athena_deploy;

fn main() {
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
