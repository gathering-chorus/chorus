//! werk-build binary — thin shell over the testable core (ADR-032 §1).
use werk_build::run_build;

fn main() {
    match run_build() {
        Ok(summary) => {
            println!("{}", summary);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("werk-build: {}", e);
            std::process::exit(1);
        }
    }
}
