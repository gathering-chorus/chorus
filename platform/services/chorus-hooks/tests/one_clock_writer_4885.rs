// @domain: time — the platform domain this suite guards (#4885)
//! #4885 — /tmp/wall-clock.txt has one writer, clock_sync. shim.rs:1613 (`wall-clock --write`)
//! was a second, with its own format and its own clock read.
use std::path::Path;

/// Every source file under `dir` that writes the clock file: a line naming the file and a write.
fn writers(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    for e in std::fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() { out.extend(writers(&p)); continue; }
        if p.extension().and_then(|x| x.to_str()) != Some("rs") { continue; }
        let src = std::fs::read_to_string(&p).unwrap();
        let names_file = src.contains("/tmp/wall-clock.txt");
        let writes = src.lines().any(|l| l.contains("fs::write(") && (l.contains("wall-clock.txt") || l.contains("CLOCK_FILE")));
        if names_file && writes { out.push(p.file_name().unwrap().to_string_lossy().to_string()); }
    }
    out.sort();
    out
}

#[test]
fn clock_sync_is_the_one_writer_of_the_clock_file() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_eq!(writers(&src), vec!["clock_sync.rs".to_string()]);
}

#[test]
fn negative_proof_a_second_writer_is_caught() {
    let d = std::env::temp_dir().join(format!("one-clock-{}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    std::fs::write(d.join("clock_sync.rs"), "const CLOCK_FILE: &str = \"/tmp/wall-clock.txt\";\nlet _ = fs::write(CLOCK_FILE, &h);\n").unwrap();
    std::fs::write(d.join("shim.rs"), "let _ = std::fs::write(\"/tmp/wall-clock.txt\", s);\n").unwrap();
    assert_eq!(writers(&d), vec!["clock_sync.rs".to_string(), "shim.rs".to_string()]);
    std::fs::remove_dir_all(&d).ok();
}
