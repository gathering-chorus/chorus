fn main() {
    match werk_unpull::run_unpull() {
        Ok(prior_branch) => println!("{}", prior_branch),
        Err(e) => {
            eprintln!("werk-unpull: {}", e);
            if e.starts_with("usage:") {
                std::process::exit(2);
            }
            std::process::exit(1);
        }
    }
}
