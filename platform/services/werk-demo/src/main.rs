//! Thin entry point. All logic lives in the library (src/lib.rs). #3046 — demo v2:
//! the /demo proving gate folded into the werk binary (validate → gates → build →
//! deploy → verify, the act steps), mirroring werk-pull #3045.
fn main() {
    std::process::exit(match werk_demo::run_demo() {
        // #3237 — the act ↔ werk-demo contract: go→0 (act merges), no-go|more→2
        // (act stops, clean), real error→1. The outcome carries the exit code.
        Ok(out) => {
            println!("{}", out.message);
            // #4502 — a step that stops says why on stderr too: dagu keeps the
            // stderr tail as the step's record, and /cws names the stop from it
            if out.exit != 0 {
                eprintln!("werk-demo: {}", out.message.lines().next().unwrap_or(""));
            }
            out.exit
        }
        Err(e) => {
            eprintln!("werk-demo: {}", e);
            1
        }
    });
}
