fn main() {
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
