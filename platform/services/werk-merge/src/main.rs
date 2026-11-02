//! Thin entry point. All logic lives in the library (one file: src/lib.rs).
fn main() {
    // #4771 AC4 — git calls this binary back as GIT_ASKPASS; answer from the
    // deploy credential file and nothing else (never argv, never logged).
    if std::env::var_os(werk_merge::ASKPASS_ENV).is_some() {
        let prompt = std::env::args().nth(1).unwrap_or_default();
        let cred = std::env::var_os(werk_merge::CRED_FILE_ENV)
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_default();
        println!("{}", werk_merge::askpass_answer(&prompt, &cred));
        std::process::exit(if cred.trim().is_empty() { 1 } else { 0 });
    }
    std::process::exit(match werk_merge::run_merge() {
        Ok(sha) => {
            println!("{}", sha);
            0
        }
        Err(e) => {
            eprintln!("werk-merge: {}", e);
            1
        }
    });
}
