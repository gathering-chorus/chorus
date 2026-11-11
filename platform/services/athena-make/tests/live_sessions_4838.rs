// @test-type: integration — boots a scratch Fuseki (the live jar, in memory, a free port); never touches :3030.
// @domain: identity
// @card: #4838
// @owner: silas
// wipe-guard: exempt — every write goes to the scratch Fuseki this file boots on a free port, never :3030
//! #4838 — `endedAt.absent=true&expiresAt.after=<t>` returns only live sessions,
//! decided by the store. Against real Jena: 5,000 ended sessions, 3 open but
//! expired (Jeff's never-swept browser sign-ins), and 2 live ones (one written
//! with Z, one with +00:00, one typed xsd:dateTime) → exactly the 2 live.
//! NEGATIVE: drop either half of the filter and ended or expired rows come back.
use athena_make::{collection_filter, collection_page_query_filtered};
use std::process::Command;

const C: &str = "https://jeffbridwell.com/chorus#";
const G: &str = "urn:chorus:domains:identity";

fn boot() -> Option<(std::process::Child, u16, std::path::PathBuf)> {
    // #4950 — FUSEKI_HOME first, as 4718 and 4717 do: a role account's HOME has no Fuseki
    let home = match std::env::var("FUSEKI_HOME") { Ok(h) => std::path::PathBuf::from(h), Err(_) => std::path::PathBuf::from(std::env::var("HOME").ok()?).join(".gathering/data/fuseki-5.1.0") };
    let jar = home.join("fuseki-server.jar");
    if !jar.exists() { return None; }
    let port = std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok()?.port();
    let dir = std::env::temp_dir().join(format!("athena-make-4728-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("config.ttl"), "@prefix fuseki: <http://jena.apache.org/fuseki#> .\n@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n@prefix ja: <http://jena.hpl.hp.com/2005/11/Assembler#> .\n[] rdf:type fuseki:Server ; fuseki:services ( <#s> ) .\n<#s> a fuseki:Service ; fuseki:name \"t\" ; fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name \"query\" ] ; fuseki:endpoint [ fuseki:operation fuseki:update ; fuseki:name \"update\" ] ; fuseki:dataset [ a ja:MemoryDataset ] .\n").ok()?;
    let java = std::env::var("JAVA").unwrap_or_else(|_| "/opt/homebrew/opt/openjdk/bin/java".into());
    let child = Command::new(java).current_dir(&dir).env("FUSEKI_BASE", &dir).env("FUSEKI_HOME", &home)
        .args(["-Xmx256m", "-cp", jar.to_str()?, "org.apache.jena.fuseki.cmd.FusekiCmd", "--config=config.ttl", &format!("--port={port}")])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok()?;
    for _ in 0..60 {
        if Command::new("curl").args(["-sf", "-o", "/dev/null", "--max-time", "1", &format!("http://127.0.0.1:{port}/$/ping")]).status().is_ok_and(|s| s.success()) {
            return Some((child, port, dir));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    None
}


fn rows(port: u16, q: &str) -> Vec<String> {
    let out = Command::new("curl").args(["-sf", "--max-time", "20", "-H", "Accept: text/tab-separated-values", "--data-urlencode", &format!("query={q}"), &format!("http://127.0.0.1:{port}/t/query")]).output().unwrap();
    String::from_utf8_lossy(&out.stdout).lines().skip(1).filter(|l| !l.is_empty()).map(|l| l.trim_matches('"').to_string()).collect()
}

#[test]
fn only_live_sessions_come_back_and_dropping_either_half_lets_dead_ones_in() {
    let (mut child, port, dir) = boot().expect("no scratch Fuseki: ~/.gathering/data/fuseki-5.1.0 and java are needed for this proof");
    let xsd = "http://www.w3.org/2001/XMLSchema#";
    let mut data = String::new();
    for i in 0..5000 {
        data.push_str(&format!("<{C}s-ended-{i}> a <{C}Session> ; <{C}endedAt> \"2026-10-20T10:00:00Z\" ; <{C}expiresAt> \"2026-12-01T00:00:00Z\" . "));
    }
    for i in 0..3 {
        data.push_str(&format!("<{C}s-expired-{i}> a <{C}Session> ; <{C}expiresAt> \"2026-10-1{i}T00:00:00Z\" . "));
    }
    data.push_str(&format!("<{C}s-live-z> a <{C}Session> ; <{C}expiresAt> \"2026-11-03T20:10:00Z\" . "));
    data.push_str(&format!("<{C}s-live-offset> a <{C}Session> ; <{C}expiresAt> \"2026-11-03T15:10:00-05:00\"^^<{xsd}dateTime> . "));
    // 19:39:59+00:00 is BEFORE 19:40:00Z: equal text would sort it after; a datetime compare must not
    data.push_str(&format!("<{C}s-just-expired> a <{C}Session> ; <{C}expiresAt> \"2026-11-03T19:39:59+00:00\" . "));
    let up = format!("INSERT DATA {{ GRAPH <{G}> {{ {data} }} }}");
    // 5,000 rows is past the argv limit: the update rides a file
    let upf = dir.join("update.rq");
    std::fs::write(&upf, &up).unwrap();
    let ok = Command::new("curl").args(["-sf", "--max-time", "60", "--data-urlencode", &format!("update@{}", upf.display()), &format!("http://127.0.0.1:{port}/t/update")]).status().unwrap().success();
    let fields = vec![("endedAt".to_string(), false), ("expiresAt".to_string(), false)];
    let q = |f: &str| collection_page_query_filtered(G, &format!("{C}Session"), &collection_filter(f, &fields).unwrap().0, 10_000, 0);
    let t0 = std::time::Instant::now();
    let live = rows(port, &q("endedAt.absent=true&expiresAt.after=2026-11-03T19:40:00Z"));
    let took = t0.elapsed();
    let no_absent = rows(port, &q("expiresAt.after=2026-11-03T19:40:00Z"));
    let no_after = rows(port, &q("endedAt.absent=true"));
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(ok, "the fixture was written");
    assert_eq!(live, vec![format!("{C}s-live-offset"), format!("{C}s-live-z")], "exactly the live ones");
    assert!(took.as_millis() < 2000, "the store decides fast: {took:?}");
    assert_eq!(no_absent.len(), 5002, "NEGATIVE: without endedAt.absent the 5,000 ended rows come back");
    assert_eq!(no_after.len(), 6, "NEGATIVE: without expiresAt.after the expired-but-open rows come back");
}
