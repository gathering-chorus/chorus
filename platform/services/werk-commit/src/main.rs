//! Thin entry point. All logic lives in the library (one file: src/lib.rs).
fn main() {
    std::process::exit(match werk_commit::run_commit() {
        Ok(sha) => {
            println!("{}", sha);
            0
        }
        Err(e) => {
            eprintln!("werk-commit: {}", e);
            1
        }
    });
}
