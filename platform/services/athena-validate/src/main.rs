//! athena-validate — the post-write conformance sweep over the live graph.
//!
//! #4167: Rust, like its three athena-* siblings. Jeff asked for this on
//! 2026-09-13; the bash it replaces is platform/scripts/athena-validate.sh.
//!
//! Jeff, 2026-09-19: "a job we keep chipping away at until it runs clean."
//! Exit codes serve that: 0 clean, 1 dirty, 2 UNMEASURED. The third one exists
//! because a sweep that could not run must never be reported as a healthy graph.
mod checks;
mod door;
mod registry;
mod store;

use checks::{Verdict, COMPLETENESS, SURVIVES_THE_DOOR};

fn main() {
    let mut total = 0usize;
    let mut unmeasured = 0usize;

    // The store checks.
    for check in [&COMPLETENESS] {
        let (verdict, findings) = store::run(check);
        for f in &findings {
            println!("graph-issue|{}|{}|{}", f.check, f.subject, f.detail);
        }
        match verdict {
            Verdict::Found(n) => total += n,
            Verdict::Clean => {}
            Verdict::Unmeasured(ref why) => {
                unmeasured += 1;
                println!("graph-issue|{}|UNMEASURED|{}", check.id, why);
            }
        }
    }

    // The door check: one sampled row per served collection, stored predicates
    // against served keys. A discovery it cannot read is unmeasured, never an
    // empty walk reporting clean.
    match registry::served_collections(&door::api_base()) {
        Err(why) => {
            unmeasured += 1;
            println!("graph-issue|{}|UNMEASURED|{}", SURVIVES_THE_DOOR.id, why);
        }
        Ok(served) => {
            for s in &served {
                let Some((name, preds)) = store::sample_subject(&s.kind) else { continue };
                let (verdict, findings) = door::check_subject(&s.collection, &name, &preds);
                for f in &findings {
                    println!("graph-issue|{}|{}|{}", f.check, f.subject, f.detail);
                }
                match verdict {
                    Verdict::Found(n) => total += n,
                    Verdict::Clean => {}
                    Verdict::Unmeasured(ref why) => {
                        unmeasured += 1;
                        println!("graph-issue|{}|UNMEASURED|{}", SURVIVES_THE_DOOR.id, why);
                    }
                }
            }
        }
    }

    if unmeasured > 0 {
        println!("graph-summary|UNMEASURED|unreachable");
        std::process::exit(2);
    }
    if total > 0 {
        println!("graph-summary|{total}|dirty");
        std::process::exit(1);
    }
    println!("graph-summary|0|clean");
}
