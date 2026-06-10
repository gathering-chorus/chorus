fn main() {
    match werk_sync::run_sync() {
        Ok(msg) => println!("{}", msg),
        Err(e) => {
            eprintln!("werk-sync: {}", e);
            // usage errors exit 2 (Bash parity); operational failures exit 1.
            if e.starts_with("usage:") {
                std::process::exit(2);
            }
            std::process::exit(1);
        }
    }
}
