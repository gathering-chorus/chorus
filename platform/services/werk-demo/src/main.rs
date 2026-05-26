//! Thin entry point. All logic lives in the library (src/lib.rs). #3046 — demo v2:
//! the /demo proving gate folded into the werk binary (validate → gates → build →
//! deploy → verify, the act steps), mirroring werk-pull #3045.
fn main() {
    std::process::exit(match werk_demo::run_demo() {
        Ok(out) => {
            println!("{}", out);
            0
        }
        Err(e) => {
            eprintln!("werk-demo: {}", e);
            1
        }
    });
}
