// @test-type: unit
// @domain: domains
//! #4515 — `athena-validate --card <n>`, the Model step of the v2 athena pipeline.
//! The card's werk is $CHORUS_WERK_BASE/<role>-<n>; its model change is the
//! model sources its diff since main touched. Every changed TTL must parse and
//! every class it declares must have a shape. A card with no werk or no model
//! change is refused by name, never passed.
//!
//! Fixtures are throwaway git repos under the temp dir: `origin/main` is a ref
//! set by hand, so nothing here reaches a remote, a store or a service.

#[allow(dead_code)]
mod card_scope {
    include!("../../shared/card_scope.rs");
}

use std::path::{Path, PathBuf};
use std::process::Command;

const PREFIXES: &str = "@prefix chorus: <https://jeffbridwell.com/chorus#> .\n\
@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
@prefix sh: <http://www.w3.org/ns/shacl#> .\n";

/// The built athena-validate, found at RUN time next to this test binary.
fn bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let dir = exe.parent().unwrap().parent().unwrap();
    let b = dir.join("athena-validate");
    assert!(b.is_file(), "athena-validate not built at {}", b.display());
    b
}

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false"])
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .unwrap();
    assert!(ok.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&ok.stderr));
}

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// A werk base holding `wren-<card>`: main carries a shaped class; the card's
/// commit writes `changes` on top. Returns the werk base.
fn werk(name: &str, card: &str, changes: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!("athena-validate-4515-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let w = base.join(format!("wren-{card}"));
    std::fs::create_dir_all(&w).unwrap();
    git(&w, &["init", "-q", "-b", "main"]);
    write(&w, "roles/wren/ontology/base.ttl", &format!(
        "{PREFIXES}chorus:Base a owl:Class .\nchorus:BaseShape a sh:NodeShape ; sh:targetClass chorus:Base .\n"));
    write(&w, "README", "x\n");
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "main"]);
    git(&w, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    for (rel, body) in changes {
        write(&w, rel, body);
    }
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "--allow-empty", "-m", "card"]);
    base
}

fn run(base: &Path, card: &str) -> (i32, String) {
    let out = Command::new(bin())
        .args(["--card", card])
        .env("CHORUS_WERK_BASE", base)
        .env("CHORUS_ROLE", "wren")
        .env_remove("ROLE")
        .env("ATHENA_VALIDATE_NUDGE", "0")
        .output()
        .unwrap();
    (out.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr)))
}

#[test]
fn a_card_whose_class_has_a_shape_is_clean() {
    let base = werk("clean", "9001", &[("roles/wren/ontology/widgets.ttl", &format!(
        "{PREFIXES}chorus:Widget a owl:Class .\nchorus:WidgetShape a sh:NodeShape ; sh:targetClass chorus:Widget .\n"))]);
    let (code, out) = run(&base, "9001");
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("card-model|roles/wren/ontology/widgets.ttl|parses"), "{out}");
    assert!(out.contains("card-shape|roles/wren/ontology/widgets.ttl|Widget|WidgetShape"), "{out}");
    assert!(out.contains("card-summary|0|clean"), "{out}");
    // only the card's change is checked: main's Base class is not this card's
    assert!(!out.contains("|Base|"), "{out}");
}

#[test]
fn a_shape_in_another_file_of_the_werk_counts() {
    // the shape for a class can live in any model file of the werk's tree
    let base = werk("elsewhere", "9002", &[
        ("roles/wren/ontology/gadget.ttl", &format!("{PREFIXES}chorus:Gadget a owl:Class .\n")),
        ("designing/data/gadget-shapes.ttl", &format!("{PREFIXES}chorus:GadgetShape a sh:NodeShape .\n")),
    ]);
    let (code, out) = run(&base, "9002");
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("|Gadget|GadgetShape"), "{out}");
}

#[test]
fn negative_proof_a_class_with_no_shape_fails_and_is_named() {
    let base = werk("noshape", "9003", &[("roles/wren/ontology/sprocket.ttl", &format!(
        "{PREFIXES}chorus:Sprocket a owl:Class .\n"))]);
    let (code, out) = run(&base, "9003");
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("card-shape|roles/wren/ontology/sprocket.ttl|Sprocket|NO SHAPE"), "{out}");
    assert!(out.contains("card-summary|1|dirty"), "{out}");
}

#[test]
fn negative_proof_a_ttl_that_does_not_parse_fails_and_is_named() {
    let base = werk("noparse", "9004", &[("roles/wren/ontology/broken.ttl",
        "chorus:Broken a owl:Class .\n")]); // no @prefix: riot rejects it
    let (code, out) = run(&base, "9004");
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("card-model|roles/wren/ontology/broken.ttl|PARSE FAILED"), "{out}");
}

#[test]
fn negative_proof_a_card_with_no_model_change_is_refused_by_name() {
    let base = werk("nomodel", "9005", &[("platform/scripts/x.sh", "echo\n")]);
    let (code, out) = run(&base, "9005");
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("card #9005: no model change"), "{out}");
    assert!(!out.contains("card-summary|0|clean"), "{out}");
}

#[test]
fn negative_proof_a_card_with_no_werk_is_refused_by_name() {
    let base = werk("nowerk", "9006", &[]);
    let (code, out) = run(&base, "9099");
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("card #9099: no werk at"), "{out}");
}

#[test]
fn negative_proof_main_moving_after_the_cut_is_not_the_cards_change() {
    // main changes a model file after the werk was cut; the card itself touches
    // no model. A two-dot tree diff reads main's edit as the card's change.
    let base = werk("mainmoved", "9007", &[("README", "card\n")]);
    let w = base.join("wren-9007");
    git(&w, &["checkout", "-q", "-b", "other", "origin/main"]);
    write(&w, "roles/wren/ontology/base.ttl", &format!("{PREFIXES}chorus:Base a owl:Class .\nchorus:Landed a owl:Class .\n"));
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "someone else's land"]);
    git(&w, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    git(&w, &["checkout", "-q", "main"]);
    let wp = w.to_str().unwrap();
    // the violated state: the two-dot range says this card changed the model
    assert_eq!(card_scope::changed_sources(wp, "origin/main..HEAD").unwrap(), vec!["roles/wren/ontology/base.ttl"]);
    // the rule's range does not
    assert!(card_scope::changed_sources(wp, card_scope::CARD_RANGE).unwrap().is_empty());
    let (code, out) = run(&base, "9007");
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("no model change"), "{out}");
}

#[test]
fn the_role_comes_from_chorus_role_then_role() {
    let base = werk("role", "9008", &[("roles/wren/ontology/r.ttl", &format!("{PREFIXES}chorus:R a owl:Class .\nchorus:RShape a sh:NodeShape .\n"))]);
    let out = Command::new(bin()).args(["--card", "9008"]).env("CHORUS_WERK_BASE", &base)
        .env_remove("CHORUS_ROLE").env("ROLE", "wren").env("ATHENA_VALIDATE_NUDGE", "0").output().unwrap();
    assert_eq!(out.status.code(), Some(0), "{}", String::from_utf8_lossy(&out.stdout));
    let none = card_scope::resolve_from("9008", None, base.to_str().unwrap()).unwrap_err();
    assert!(none.contains("card #9008: no role"), "{none}");
}

#[test]
fn negative_proof_a_card_id_that_is_not_a_number_is_refused() {
    // a card id becomes a path segment; `../x` must never resolve a werk
    let e = card_scope::resolve_from("../kade-1", Some("wren".into()), "/tmp").unwrap_err();
    assert!(e.contains("not a card number"), "{e}");
}

#[test]
fn shapes_are_found_by_target_class_or_by_name_and_only_for_node_shapes() {
    let c = "<https://jeffbridwell.com/chorus#Thing>";
    let t = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let ns = "<http://www.w3.org/ns/shacl#NodeShape>";
    let by_target = format!("<urn:x#S> {t} {ns} .\n<urn:x#S> <http://www.w3.org/ns/shacl#targetClass> {c} .\n");
    assert_eq!(card_scope::shape_for(c, &by_target).as_deref(), Some("S"));
    let by_name = format!("<https://jeffbridwell.com/chorus#ThingShape> {t} {ns} .\n");
    assert_eq!(card_scope::shape_for(c, &by_name).as_deref(), Some("ThingShape"));
    // negative: a ThingShape that is only MENTIONED (not a NodeShape) is no shape
    let mention = "<urn:x:a> <urn:x:p> <https://jeffbridwell.com/chorus#ThingShape> .\n";
    assert_eq!(card_scope::shape_for(c, mention), None);
    let classes = card_scope::declared_classes(&format!("{c} {t} <http://www.w3.org/2002/07/owl#Class> .\n<urn:x:i> {t} {c} .\n"));
    assert_eq!(classes, vec![c.to_string()], "an instance of a class is not a class");
}
