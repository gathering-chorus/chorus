// @test-type: unit — hermetic: the real vocab-gen binary against a loopback stub of the store's query endpoint; no Fuseki.
// @domain: knowledge
// @card: #4755
//! #4755 — src/bin/vocab-gen.rs's process half: it reads classes, attributes
//! and declared IRIs from the store, writes the generated vocabulary, and
//! refuses (writing nothing) when the store returns no classes, because the
//! vocabulary set is `replace` and an empty file would delete every term.
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::process::Command;

/// Answers each SPARQL query with CSV picked by what it asks for.
fn stub(classes: &'static str) -> String {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://127.0.0.1:{}/query", l.local_addr().unwrap().port());
    std::thread::spawn(move || {
        for s in l.incoming().flatten() {
            let mut r = BufReader::new(s.try_clone().unwrap());
            let mut len = 0usize;
            loop {
                let mut h = String::new();
                if r.read_line(&mut h).is_err() || h == "\r\n" || h.is_empty() { break; }
                if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") { len = v.trim().parse().unwrap_or(0); }
            }
            let mut body = vec![0u8; len];
            let _ = r.read_exact(&mut body);
            let q = String::from_utf8_lossy(&body).to_string();
            let csv = if q.contains("definesVocabulary") {
                classes.to_string()
            } else if q.contains("sh%3Apath") || q.contains("sh:path") {
                "t,c\r\nhttps://jeffbridwell.com/chorus#ownedBy,who owns it\r\n".to_string()
            } else {
                "s\r\nhttps://jeffbridwell.com/chorus#Card\r\nhttps://jeffbridwell.com/chorus#ownedBy\r\n".to_string()
            };
            let mut s = s;
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Type: text/csv\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{csv}", csv.len());
        }
    });
    url
}

fn run(url: &str, out: &std::path::Path, root: &std::path::Path) -> (i32, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_vocab-gen"));
    c.arg(out).env("FUSEKI_QUERY", url).env("CHORUS_ROOT", root);
    let o = c.output().unwrap();
    (o.status.code().unwrap_or(-1), String::from_utf8_lossy(&o.stdout).into(), String::from_utf8_lossy(&o.stderr).into())
}

fn scratch(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("vocab-gen-4755-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("roles/silas/ontology")).unwrap();
    d
}

#[test]
fn the_model_becomes_a_generated_vocabulary_file() {
    let d = scratch("ok");
    let url = stub("t,c\r\nhttps://jeffbridwell.com/chorus#Card,a unit of work\r\n");
    let out = d.join("vocab.ttl");
    let (code, stdout, err) = run(&url, &out, &d);
    assert_eq!(code, 0, "{err}");
    assert!(stdout.contains("1 classes + 1 attributes"), "{stdout}");
    let ttl = std::fs::read_to_string(&out).unwrap();
    assert!(ttl.contains("a unit of work"), "{ttl}");
    assert!(ttl.contains("who owns it"), "{ttl}");
}

/// NEGATIVE PROOF: a store that answers no classes (unreachable or empty)
/// writes nothing, so the next deploy cannot delete every term.
#[test]
fn no_classes_is_refused_and_nothing_is_written() {
    let d = scratch("empty");
    let url = stub("t,c\r\n");
    let out = d.join("vocab.ttl");
    let (code, _, err) = run(&url, &out, &d);
    assert_eq!(code, 1);
    assert!(err.contains("REFUSED"), "{err}");
    assert!(!out.exists(), "an empty vocabulary was written");
}
