// #4446 — a scheduled job logs its own failure.
#[allow(dead_code)] // each crate uses part of the shared helper
mod service_lifecycle {
    include!("../../shared/service_lifecycle.rs");
}

fn main() {
    // #4446 — under launchd, a failed run is logged as service.failed (shared/service_lifecycle.rs).
    service_lifecycle::run_as_job();
    std::process::exit(match pair_heartbeat::run() {
        Ok(s) => {
            if !s.is_empty() {
                println!("{}", s);
            }
            0
        }
        Err(e) => {
            eprintln!("pair-heartbeat: {}", e);
            1
        }
    });
}
