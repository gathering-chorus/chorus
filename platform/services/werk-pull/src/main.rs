//! Thin entry point. All logic lives in the library (one file: src/lib.rs).
fn main() {
    std::process::exit(match werk_pull::run_pull() {
        Ok(branch) => {
            println!("{}", branch);
            0
        }
        Err(e) => {
            eprintln!("werk-pull: {}", e);
            1
        }
    });
}
