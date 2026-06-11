fn main() {
    match werk_review::run_review() {
        Ok(msg) => println!("{}", msg),
        Err(e) => {
            eprintln!("werk-review: {}", e);
            if e.starts_with("usage:") {
                std::process::exit(2);
            }
            // pass=0 / fail-or-refuse=1 — the advisory→hard-gate flip needs no rewiring.
            std::process::exit(1);
        }
    }
}
