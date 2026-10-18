// @test-type: unit
// @domain: athena
//! #4576 reopen — a model deploy keeps CoreShape's fields on every shape.
//!
//! The flatten (#4576) links each shape that names `sh:node chorus:CoreShape`
//! to CoreShape's OWN property nodes; about 105 shapes share two of them (name,
//! description). The blank-node cleanup (#4029) walked from every staged shape
//! into those shared nodes and emptied them, so on 2026-10-18 every deploy of a
//! CoreShape shape (the #4544 land at 05:53 and 06:10) left 105 shapes with
//! two empty fields, and Actor, which has no others, lost its route.
//!
//! The SPARQL runs for real, in Jena's in-memory store (`update`, the same
//! engine as Fuseki): a string check cannot tell which nodes a WHERE reaches.
use athena_deploy::{bnode_cleanup, flatten_node_shapes_sparql, merge_sparql};
use std::process::Command;

const STORE: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix c: <https://jeffbridwell.com/chorus#> .
GRAPH <urn:test:ont> {
  c:CoreShape a sh:NodeShape ; sh:property _:name .
  _:name sh:path c:label ; sh:name "name" .
  c:AShape a sh:NodeShape ; sh:targetClass c:A ; sh:node c:CoreShape ; sh:property _:a1 , _:name .
  _:a1 sh:path c:aField .
  c:BShape a sh:NodeShape ; sh:targetClass c:B ; sh:node c:CoreShape ; sh:property _:name .
}
"#;

/// The deploy's write for one staged file, in order: cleanup + merge, then flatten.
fn deploy(staged: &str, cleanup: bool) -> String {
    let dir = std::env::temp_dir().join(format!("core-nodes-4576-{}-{}", std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst)));
    std::fs::create_dir_all(&dir).unwrap();
    let data = dir.join("store.trig");
    std::fs::write(&data, format!("{STORE}\n{staged}")).unwrap();
    let bnodes = if cleanup { bnode_cleanup("urn:test:stg", "urn:test:ont", 6, 6) } else { String::new() };
    let ru = dir.join("deploy.ru");
    std::fs::write(&ru, format!("{bnodes}{} ;\n{}", merge_sparql("urn:test:stg", "urn:test:ont"),
        flatten_node_shapes_sparql("urn:test:ont"))).unwrap();
    let out = Command::new("update").arg(format!("--data={}", data.display()))
        .arg(format!("--update={}", ru.display())).arg("--dump").output()
        .expect("UNMEASURED: Jena's `update` is not installed (brew install jena) — this test cannot run without it");
    assert!(out.status.success(), "update failed: {}", String::from_utf8_lossy(&out.stderr));
    let _ = std::fs::remove_dir_all(&dir);
    String::from_utf8_lossy(&out.stdout).into_owned()
}
static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// `update --dump` prints TriG; riot turns it into one quad per line.
fn nquads(trig: &str) -> String {
    use std::io::Write;
    let mut c = Command::new("riot").args(["--syntax=trig", "--output=nquads", "-"])
        .stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped()).spawn()
        .expect("UNMEASURED: Jena's `riot` is not installed");
    c.stdin.take().unwrap().write_all(trig.as_bytes()).unwrap();
    let out = c.wait_with_output().unwrap();
    assert!(out.status.success(), "riot could not read the dump: {}\n{trig}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// What `shape` serves: the path of every property node it links that has one,
/// and how many linked nodes are empty, in the ontology graph only (staging
/// shares the inserted nodes, so reading both would count each twice).
fn fields(dump: &str, shape: &str) -> (Vec<String>, usize) {
    let dump = nquads(dump);
    let q: Vec<Vec<&str>> = dump.lines().map(|l| l.split(' ').collect()).filter(|t: &Vec<&str>| t.len() >= 4 && t[t.len() - 2] == "<urn:test:ont>").collect();
    let me = format!("<https://jeffbridwell.com/chorus#{shape}>");
    let mut paths = Vec::new();
    let mut empty = 0;
    for t in q.iter().filter(|t| t[0] == me && t[1] == "<http://www.w3.org/ns/shacl#property>") {
        let node = t[2];
        match q.iter().find(|u| u[0] == node && u[1] == "<http://www.w3.org/ns/shacl#path>") {
            Some(u) => paths.push(u[2].rsplit('#').next().unwrap().trim_end_matches('>').to_string()),
            None => empty += 1,
        }
    }
    paths.sort();
    (paths, empty)
}

const STAGED_A: &str = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix c: <https://jeffbridwell.com/chorus#> .
GRAPH <urn:test:stg> { c:AShape a sh:NodeShape ; sh:targetClass c:A ; sh:node c:CoreShape ; sh:property [ sh:path c:aField ] . }
"#;

#[test]
fn deploying_one_core_shape_user_leaves_every_other_users_core_fields_whole() {
    let dump = deploy(STAGED_A, true);
    assert_eq!(fields(&dump, "BShape"), (vec!["label".to_string()], 0), "B keeps CoreShape's name\n{dump}");
    assert_eq!(fields(&dump, "CoreShape"), (vec!["label".to_string()], 0), "{dump}");
    // the staged shape itself: its own field once (the old body is gone) and the core one
    assert_eq!(fields(&dump, "AShape"), (vec!["aField".to_string(), "label".to_string()], 0), "{dump}");
}

/// A changed CoreShape still replaces its fields on every user: the owner's own
/// nodes are cleaned, and no user keeps a link to an emptied one.
#[test]
fn deploying_core_shape_itself_replaces_its_fields_on_every_user() {
    let staged = r#"
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix c: <https://jeffbridwell.com/chorus#> .
GRAPH <urn:test:stg> { c:CoreShape a sh:NodeShape ; sh:property [ sh:path c:title ; sh:name "name" ] . }
"#;
    let dump = deploy(staged, true);
    for s in ["CoreShape", "BShape"] {
        assert_eq!(fields(&dump, s), (vec!["title".to_string()], 0), "{s}\n{dump}");
    }
    assert_eq!(fields(&dump, "AShape"), (vec!["aField".to_string(), "title".to_string()], 0), "{dump}");
}

/// NEGATIVE PROOF: with the cleanup on and the shared-node guard gone, the same
/// deploy empties B's name field — the 10-18 break. Proven by running the
/// unguarded walk (#4029's original level-1 delete) over the same store.
#[test]
fn negative_proof_the_unguarded_cleanup_empties_the_other_users_field() {
    let guarded = bnode_cleanup("urn:test:stg", "urn:test:ont", 6, 6);
    let unguarded = athena_deploy::bnode_cleanup_unguarded_for_proof("urn:test:stg", "urn:test:ont", 6, 6);
    assert_ne!(guarded, unguarded, "the guard is in the SPARQL");
    let dir = std::env::temp_dir().join(format!("core-nodes-4576-neg-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("s.trig"), format!("{STORE}\n{STAGED_A}")).unwrap();
    std::fs::write(dir.join("d.ru"), format!("{unguarded}{}", merge_sparql("urn:test:stg", "urn:test:ont"))).unwrap();
    let out = Command::new("update").arg(format!("--data={}", dir.join("s.trig").display()))
        .arg(format!("--update={}", dir.join("d.ru").display())).arg("--dump").output().expect("UNMEASURED: no `update`");
    let _ = std::fs::remove_dir_all(&dir);
    let dump = String::from_utf8_lossy(&out.stdout);
    assert_eq!(fields(&dump, "BShape").1, 1, "the unguarded walk empties B's shared name node\n{dump}");
}
