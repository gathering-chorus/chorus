// @test-type: integration — the built binary against a scratch Fuseki loaded with fixture rows
// @domain: domains
// @card: #5002
//! #5002 AC1 — owner-rule-missing names every class whose rows carry ownedBy while no
//! shape says what an owner is, with the count of such rows, and a shape on a superclass
//! governs its subclasses (SHACL targetClass covers subclass instances; Silas 11-18 10:06).
//! Runs `athena-validate --store-only` against a scratch Fuseki, never the live store.

use std::process::Command;

const C: &str = "https://jeffbridwell.com/chorus#";

fn boot() -> Option<(std::process::Child, u16, std::path::PathBuf)> {
    let home = match std::env::var("FUSEKI_HOME") {
        Ok(h) => std::path::PathBuf::from(h),
        Err(_) => std::path::PathBuf::from(std::env::var("HOME").ok()?).join(".gathering/data/fuseki-5.1.0"),
    };
    let jar = home.join("fuseki-server.jar");
    if !jar.exists() { return None; }
    let port = std::net::TcpListener::bind("127.0.0.1:0").ok()?.local_addr().ok()?.port();
    let dir = std::env::temp_dir().join(format!("athena-validate-5002-{}-{port}", std::process::id()));
    std::fs::create_dir_all(&dir).ok()?;
    std::fs::write(dir.join("config.ttl"), "@prefix fuseki: <http://jena.apache.org/fuseki#> .\n@prefix rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#> .\n@prefix ja: <http://jena.hpl.hp.com/2005/11/Assembler#> .\n[] rdf:type fuseki:Server ; fuseki:services ( <#s> ) .\n<#s> a fuseki:Service ; fuseki:name \"t\" ; fuseki:endpoint [ fuseki:operation fuseki:query ; fuseki:name \"query\" ] ; fuseki:endpoint [ fuseki:operation fuseki:update ; fuseki:name \"update\" ] ; fuseki:dataset [ a ja:MemoryDataset ] .\n").ok()?;
    let java = std::env::var("JAVA").unwrap_or_else(|_| "/opt/homebrew/opt/openjdk/bin/java".into());
    let child = Command::new(java).current_dir(&dir).env("FUSEKI_BASE", &dir).env("FUSEKI_HOME", &home)
        .args(["-Xmx256m", "-cp", jar.to_str()?, "org.apache.jena.fuseki.cmd.FusekiCmd", "--config=config.ttl", &format!("--port={port}")])
        .stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn().ok()?;
    // 90s, not 30: under a loaded box (load 7-9) the 30s window missed on 11-06 and 11-18
    for _ in 0..180 {
        if Command::new("curl").args(["-sf", "-o", "/dev/null", "--max-time", "1", &format!("http://127.0.0.1:{port}/$/ping")]).status().is_ok_and(|s| s.success()) {
            return Some((child, port, dir));
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
    None
}

fn update(port: u16, q: &str) {
    let ok = Command::new("curl").args(["-sf", "--max-time", "20", "--data-urlencode", &format!("update={q}"), &format!("http://127.0.0.1:{port}/t/update")])
        .status().is_ok_and(|s| s.success());
    assert!(ok, "fixture load failed");
}

/// Shapes: Owned and Memory carry the owner rule. Ontology: LongTerm ⊂ Memory, Orphan ⊂ Other
/// (Other has no shape). Rows: one of each covered class, two Bare rows and one Orphan row
/// with owners and no governing shape.
fn fixture(with_memory_rule: bool) -> String {
    let mem = if with_memory_rule {
        "c:MemoryShape sh:targetClass c:Memory ; sh:property [ sh:path c:ownedBy ; sh:class c:Principal ] ."
    } else { "" };
    format!("PREFIX c: <{C}> PREFIX sh: <http://www.w3.org/ns/shacl#> PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
INSERT DATA {{
  GRAPH <urn:chorus:ontology> {{
    c:OwnedShape sh:targetClass c:Owned ; sh:property [ sh:path c:ownedBy ; sh:class c:Principal ] .
    {mem}
    c:LongTerm rdfs:subClassOf c:Memory . c:Orphan rdfs:subClassOf c:Other .
  }}
  GRAPH <urn:chorus:domains:t> {{
    c:r1 a c:Owned ; c:ownedBy c:principal-p .
    c:r2 a c:LongTerm ; c:ownedBy c:principal-p .
    c:r3 a c:Bare ; c:ownedBy c:principal-p .
    c:r4 a c:Bare ; c:ownedBy c:principal-p .
    c:r5 a c:Orphan ; c:ownedBy c:principal-p .
  }}
}}")
}

fn owner_rule_lines(port: u16) -> Vec<String> {
    let out = Command::new(env!("CARGO_BIN_EXE_athena-validate"))
        .arg("--store-only")
        .env("FUSEKI_QUERY", format!("http://127.0.0.1:{port}/t/query"))
        .output().expect("athena-validate runs");
    String::from_utf8_lossy(&out.stdout).lines().chain(String::from_utf8_lossy(&out.stderr).lines())
        .filter(|l| l.contains("|owner-rule-missing|")).map(str::to_string).collect()
}

/// One scratch Fuseki for both cases: two JVMs at once missed the boot window at load 22.
#[test]
fn owner_rule_missing_counts_each_class_and_follows_subclass_of() {
    let (mut child, port, dir) = boot().expect("no scratch Fuseki: ~/.gathering/data/fuseki-5.1.0 and java are needed, or it did not answer in 90s");
    update(port, &fixture(true));
    let with_rule = owner_rule_lines(port).join("\n");
    // NEGATIVE PROOF: take Memory's rule away and LongTerm, governed only through it, turns red
    update(port, "DROP ALL");
    update(port, &fixture(false));
    let without_rule = owner_rule_lines(port).join("\n");
    let _ = child.kill();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(with_rule.contains("|owner-rule-missing|Bare|2"), "Bare's two owned rows, counted: {with_rule}");
    // NEGATIVE PROOF (Silas 10:06): a subclass row with no governing shape up its chain stays red
    assert!(with_rule.contains("|owner-rule-missing|Orphan|1"), "Orphan has no shape up its chain: {with_rule}");
    assert!(!with_rule.contains("|Owned|"), "a class with its own rule is clean: {with_rule}");
    assert!(!with_rule.contains("|LongTerm|"), "a subclass governed by Memory's rule is clean: {with_rule}");
    assert!(!with_rule.contains("UNMEASURED"), "the check ran: {with_rule}");
    assert!(without_rule.contains("|owner-rule-missing|LongTerm|1"), "LongTerm has no rule once Memory's is gone: {without_rule}");
}
