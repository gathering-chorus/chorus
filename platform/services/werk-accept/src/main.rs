//! Thin entry point. All logic lives in the library (one file: src/lib.rs).
fn main() {
    std::process::exit(match werk_accept::run_accept() {
        Ok(msg) => {
            println!("{}", msg);
            0
        }
        Err(e) => {
            eprintln!("werk-accept: {}", e);
            1
        }
    });
}
