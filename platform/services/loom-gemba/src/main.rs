fn main() {
    std::process::exit(match loom_gemba::run() {
        // The banner contract: output is never empty — always print.
        Ok(s) => {
            println!("{}", s);
            0
        }
        Err(e) => {
            eprintln!("loom-gemba: {}", e);
            1
        }
    });
}
