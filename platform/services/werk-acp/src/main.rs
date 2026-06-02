//! Thin entry point. All logic lives in the library (one file: src/lib.rs).
fn main() {
    std::process::exit(match werk_acp::run_acp() {
        Ok(summary) => {
            println!("{}", summary);
            0
        }
        Err(e) => {
            eprintln!("werk-acp: {}", e);
            1
        }
    });
}
