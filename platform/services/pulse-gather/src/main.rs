fn main() {
    std::process::exit(match pulse_gather::run() {
        Ok(s) => {
            if !s.is_empty() {
                println!("{}", s);
            }
            0
        }
        Err(e) => {
            eprintln!("pulse-gather: {}", e);
            1
        }
    });
}
