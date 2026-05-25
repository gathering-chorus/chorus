//! Thin entry point. All logic lives in the library (one file: src/lib.rs).
fn main() {
    std::process::exit(match werk_push::run_push() {
        Ok(sha) => {
            println!("{}", sha);
            0
        }
        Err(e) => {
            eprintln!("werk-push: {}", e);
            1
        }
    });
}
