//! #4225 — prints the demo-fitness line and appends it to the series.
//!
//! usage: demo-fitness [role]        (default $CHORUS_ROLE, else silas)
//! env:   DEMO_FITNESS_LAUNCHCTL     launchctl to run (tests pass a stub)
//!        DEMO_FITNESS_SERIES        series file (default ~/.chorus/demo-fitness.jsonl)
//!
//! Read-only against launchd. The only write is one appended line.

use std::io::Write;
use std::process::Command;

fn env_or(k: &str, d: &str) -> String {
    std::env::var(k).unwrap_or_else(|_| d.to_string())
}

fn main() {
    let role = std::env::args()
        .nth(1)
        .unwrap_or_else(|| env_or("CHORUS_ROLE", "silas"));
    let launchctl = env_or("DEMO_FITNESS_LAUNCHCTL", "launchctl");
    let home = env_or("HOME", "/tmp");
    let series_path = env_or("DEMO_FITNESS_SERIES", &format!("{}/.chorus/demo-fitness.jsonl", home));

    let out = match Command::new(&launchctl).arg("list").output() {
        Ok(o) => String::from_utf8_lossy(&o.stdout).into_owned(),
        Err(e) => {
            // Loud, never a zero: "no services" and "cannot ask" are different
            // states and the check that cannot tell them apart is the one that
            // cries wolf (#4145).
            eprintln!("demo-fitness: UNMEASURED — cannot run {} list: {}", launchctl, e);
            std::process::exit(1);
        }
    };

    // A verb is owned when the werk has built its own copy, because that is
    // the binary the variant's athena-make calls (CHORUS_MODEL_BIN). Asking
    // launchd about it can only ever answer "missing" — it never runs.
    let werk_base = env_or(
        "CHORUS_WERK_BASE",
        "/Users/jeffbridwell/CascadeProjects/chorus-werk",
    );
    let own_verbs: Vec<String> = demo_fitness::TARGET
        .iter()
        .filter(|(_, k)| *k == demo_fitness::Piece::Verb)
        .map(|(n, _)| *n)
        .filter(|n| std::path::Path::new(&demo_fitness::verb_bin_path(n, &role, &werk_base)).is_file())
        .map(|n| n.to_string())
        .collect();

    let f = demo_fitness::measure(&out, &role, &own_verbs);
    let prev = std::fs::read_to_string(&series_path)
        .ok()
        .and_then(|s| demo_fitness::previous_own(&s));
    print!("{}", demo_fitness::report(&f, prev));

    let ts = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    if let Some(dir) = std::path::Path::new(&series_path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    match std::fs::OpenOptions::new().create(true).append(true).open(&series_path) {
        Ok(mut fh) => {
            if writeln!(fh, "{}", demo_fitness::series_line(&f, &ts)).is_err() {
                eprintln!("demo-fitness: could not append to {}", series_path);
            }
        }
        Err(e) => eprintln!("demo-fitness: could not open {}: {}", series_path, e),
    }
}
