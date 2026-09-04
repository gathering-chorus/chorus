//! werk-deploy binary — thin shell over the testable core (ADR-032 §1).
use werk_deploy::run_deploy;

fn main() {
    // #4102 — `werk-deploy env-port <service> <role>` answers which port a
    // variant service listens on, from the ONE table that assigns them
    // (demo_env::env_services). A lane that wants to talk to the card's own
    // stack asks here instead of hardcoding 3365 — a second copy of the port
    // map is how a lane ends up pointed at canonical without noticing.
    let args: Vec<String> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("env-port") {
        match (args.get(2), args.get(3)) {
            (Some(service), Some(role)) => match werk_deploy::demo_env::env_port_for(service, role) {
                Ok(p) => { println!("{}", p); std::process::exit(0); }
                Err(e) => { eprintln!("werk-deploy: {}", e); std::process::exit(1); }
            },
            _ => { eprintln!("werk-deploy: usage: werk-deploy env-port <service> <role>"); std::process::exit(1); }
        }
    }
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
