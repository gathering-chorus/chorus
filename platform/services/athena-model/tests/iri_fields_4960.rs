// @test-type: unit — pure: the door's value terms, then SHACL (Jena's CLI) over what it would write; no store.
// @domain: persistence
// @card: #4960
// @owner: silas
//! #4960 land: every Permission write through the door was refused (422) because
//! a field the shape holds as an IRI (sh:nodeKind sh:IRI, or sh:in over IRIs, like
//! Permission.mode in acl:Read..Control) went out as a string literal, and
//! xsd:anyURI (Permission.accessTo) went out untyped. Policy.derivedFrom had the
//! same flaw. The fix: such fields are marked "@iri" in the shape's datatypes and
//! written as <IRI>; anyURI is typed. An IRI is written only when it is safe to put
//! in Turtle — a value that could inject triples is refused (#3734 negative proof).
//! Exercises athena-model src/lib.rs field_literal, datatype_ok, to_turtle_typed.
use athena_model::{datatype_ok, field_literal, to_turtle_typed, WriteReq};
use std::collections::BTreeMap;

const ACL_WRITE: &str = "http://www.w3.org/ns/auth/acl#Write";

#[test]
fn an_iri_field_is_written_as_an_iri() {
    assert_eq!(field_literal(ACL_WRITE, Some("@iri")), format!("<{ACL_WRITE}>"));
    assert_eq!(field_literal("urn:chorus:domains:policies", Some("@iri")), "<urn:chorus:domains:policies>");
}

#[test]
fn any_uri_is_typed() {
    assert_eq!(field_literal("urn:chorus:domains:policies", Some("anyURI")),
               "\"urn:chorus:domains:policies\"^^<http://www.w3.org/2001/XMLSchema#anyURI>");
}

#[test]
fn negative_proof_a_value_that_could_inject_turtle_is_refused_never_written_as_an_iri() {
    for bad in ["https://x> <https://p> <https://o", "https://x y", "https://x\u{7f}", "https://x\u{1b}", "not-an-iri", "https://x\"", "ftp://x", ""] {
        assert!(!datatype_ok(bad, "@iri"), "accepted {bad:?}");
        assert!(!field_literal(bad, Some("@iri")).starts_with('<'), "wrote {bad:?} as an IRI");
    }
    assert!(datatype_ok(ACL_WRITE, "@iri"));
}

#[test]
fn plain_fields_are_unchanged() {
    assert_eq!(field_literal("hello", None), "\"hello\"");
    assert_eq!(field_literal(ACL_WRITE, None), format!("\"{ACL_WRITE}\""));
}

fn permission_ttl(datatypes: &BTreeMap<String, String>) -> String {
    let mut req = WriteReq { kind: "permission".into(), name: "silas-policies".into(), ..Default::default() };
    req.fields.insert("label".into(), "silas may write urn:chorus:domains:policies".into());
    req.fields.insert("mode".into(), ACL_WRITE.into());
    req.fields.insert("accessTo".into(), "urn:chorus:domains:policies".into());
    req.edges.push(("agent".into(), "principal".into(), "silas".into()));
    req.edges.push(("ownedBy".into(), "principal".into(), "silas".into()));
    to_turtle_typed(&req, datatypes).unwrap().1
}

fn conforms(ttl: &str) -> bool {
    let shapes = std::path::Path::new(&std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../../roles/silas/ontology/chorus.ttl");
    let dir = std::env::temp_dir().join(format!("iri-4960-{}-{}", std::process::id(), ttl.len()));
    std::fs::create_dir_all(&dir).unwrap();
    let data = dir.join("d.ttl");
    std::fs::write(&data, format!("<https://jeffbridwell.com/chorus#principal-silas> a <https://jeffbridwell.com/chorus#Principal> .\n{ttl}")).unwrap();
    // PermissionShape alone, cut from chorus.ttl with its prefixes, so CoreShape's own
    // rules (proven by #4576) do not decide this
    let text = std::fs::read_to_string(&shapes).unwrap();
    let prefixes: String = text.lines().filter(|l| l.starts_with("@prefix")).map(|l| format!("{l}\n")).collect();
    let start = text.find("chorus:PermissionShape a sh:NodeShape").expect("PermissionShape is in chorus.ttl");
    let end = start + text[start..].find("] .\n").unwrap() + 4;
    let shape = text[start..end].replace("sh:node chorus:CoreShape ;", "");
    let sfile = dir.join("s.ttl");
    std::fs::write(&sfile, format!("{prefixes}{shape}")).unwrap();
    let out = std::process::Command::new("shacl").args(["validate", "--shapes"]).arg(&sfile).arg("--data").arg(&data).output()
        .expect("UNMEASURED: no Jena shacl CLI on this box");
    let report = String::from_utf8_lossy(&out.stdout).to_string();
    let _ = std::fs::remove_dir_all(&dir);
    assert!(report.contains("sh:conforms"), "no verdict: {report}");
    report.contains("sh:conforms  true") || report.contains("sh:conforms true")
}

#[test]
fn negative_proof_the_permission_the_land_wrote_fails_its_shape_and_the_fixed_one_passes() {
    // as the door wrote it on 11-12 14:26: mode and accessTo as plain strings
    assert!(!conforms(&permission_ttl(&BTreeMap::new())), "the old write must fail PermissionShape");
    // as the door writes it now: mode an IRI, accessTo typed xsd:anyURI
    let mut dt = BTreeMap::new();
    dt.insert("mode".to_string(), "@iri".to_string());
    dt.insert("accessTo".to_string(), "anyURI".to_string());
    assert!(conforms(&permission_ttl(&dt)), "the fixed write must pass PermissionShape");
}
