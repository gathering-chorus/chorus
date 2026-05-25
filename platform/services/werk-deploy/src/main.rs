//! werk-deploy binary — thin shell over the testable core (ADR-032 §1).
use werk_deploy::run_deploy;

fn main() {
    match run_deploy() {
        Ok(summary) => {
            println!("{}", summary);
            std::process::exit(0);
        }
        Err(e) => {
            eprintln!("werk-deploy: {}", e);
            std::process::exit(1);
        }
    }
}
