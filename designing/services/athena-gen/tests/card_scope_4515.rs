// @test-type: unit
// @domain: domains
//! #4515 — `athena-gen generate --card <n>`, the Make step of the v2 athena
//! pipeline. The card's werk ($CHORUS_WERK_BASE/<role>-<n>) and its model change
//! (model sources since it left main) decide which products are generated: every
//! product serving a class the card's files declare or shape, and every product
//! whose own rows they change. A touched class no product serves, a change that
//! touches no product, and a card with no model change are refused by name.

use athena_gen::card::{products_for, touched};
use std::path::{Path, PathBuf};
use std::process::Command;

const P: &str = "@prefix c: <https://jeffbridwell.com/chorus#> .\n\
@prefix owl: <http://www.w3.org/2002/07/owl#> .\n\
@prefix sh: <http://www.w3.org/ns/shacl#> .\n\
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .\n";

fn product_ttl() -> String {
    let keys = athena_gen::model::RUNTIME_SETTINGS;
    let mut s = format!("{P}c:gizmo a c:Product ; c:hasDomain c:widgets .\n");
    for (i, k) in keys.iter().enumerate() {
        let v = match *k {
            "model.namespace" => "https://jeffbridwell.com/chorus#",
            "store.graph_prefix" => "urn:gizmo:",
            _ => "x",
        };
        s.push_str(&format!("c:gizmo c:hasProperty c:gz{i} . c:gz{i} c:propertyKey \"{k}\" ; c:propertyValue \"{v}\" .\n"));
    }
    s.push_str("c:widgets c:definesVocabulary c:Widget .\n");
    s
}

const WIDGET: &str = "c:Widget a owl:Class .\nc:WidgetShape a sh:NodeShape ; sh:targetClass c:Widget ;\n  sh:property [ sh:path c:size ; sh:datatype xsd:integer ] .\n";

fn bin() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let b = exe.parent().unwrap().parent().unwrap().join("athena-gen");
    assert!(b.is_file(), "athena-gen not built at {}", b.display());
    b
}

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false"])
        .arg("-C").arg(dir).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
}

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

fn werk(name: &str, card: &str, changes: &[(&str, String)]) -> (PathBuf, PathBuf) {
    werk_with_main(name, card, &[], changes)
}

fn werk_with_main(name: &str, card: &str, on_main: &[(&str, String)], changes: &[(&str, String)]) -> (PathBuf, PathBuf) {
    let base = std::env::temp_dir().join(format!("athena-gen-4515-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let w = base.join(format!("wren-{card}"));
    std::fs::create_dir_all(&w).unwrap();
    git(&w, &["init", "-q", "-b", "main"]);
    write(&w, "designing/data/products.ttl", &product_ttl());
    write(&w, "roles/wren/ontology/widgets.ttl", &format!("{P}{WIDGET}"));
    for (rel, body) in on_main {
        write(&w, rel, body);
    }
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "main"]);
    git(&w, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    for (rel, body) in changes {
        write(&w, rel, body);
    }
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "--allow-empty", "-m", "card"]);
    (base, w)
}

fn run(base: &Path, args: &[&str]) -> (i32, String) {
    let o = Command::new(bin()).args(args).env("CHORUS_WERK_BASE", base).env("CHORUS_ROLE", "wren")
        .env_remove("ROLE").output().unwrap();
    (o.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}

#[test]
fn a_card_that_changes_a_served_class_generates_its_product() {
    let (base, w) = werk("served", "9101", &[("roles/wren/ontology/widgets.ttl",
        format!("{P}{WIDGET}c:WidgetShape sh:property [ sh:path c:colour ; sh:datatype xsd:string ] .\n"))]);
    let (code, out) = run(&base, &["generate", "--card", "9101"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("card #9101: gizmo <- Widget"), "{out}");
    let toml = w.join("directing/products/gizmo/api/Cargo.toml");
    assert!(toml.is_file(), "{out}");
    assert!(std::fs::read_to_string(toml).unwrap().contains("name = \"gizmo-api\""));
    let model = std::fs::read_to_string(w.join("directing/products/gizmo/api/src/model.rs")).unwrap();
    assert!(model.contains("colour"));
}

#[test]
fn negative_proof_a_class_no_product_serves_is_refused_and_nothing_is_written() {
    let (base, w) = werk("orphan", "9103", &[("roles/wren/ontology/orphan.ttl",
        format!("{P}c:Orphan a owl:Class .\nc:OrphanShape a sh:NodeShape ; sh:targetClass c:Orphan .\n"))]);
    let (code, out) = run(&base, &["generate", "--card", "9103"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("in no product's domain") && out.contains("Orphan"), "{out}");
    assert!(!w.join("directing/products").exists());
}

#[test]
fn negative_proof_a_change_that_touches_no_product_is_refused() {
    let (base, _) = werk("noproduct", "9104", &[("roles/wren/ontology/rows.ttl",
        format!("{P}c:some-row c:propertyKey \"k\" .\n"))]);
    let (code, out) = run(&base, &["generate", "--card", "9104"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("card #9104: the card's model change touches no product"), "{out}");
}

#[test]
fn negative_proof_a_card_with_no_model_change_or_no_werk_is_refused() {
    let (base, _) = werk("nomodel", "9105", &[("README", "x\n".into())]);
    let (code, out) = run(&base, &["generate", "--card", "9105"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("card #9105: no model change"), "{out}");
    let (code, out) = run(&base, &["generate", "--card", "9199"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("card #9199: no werk at"), "{out}");
}

#[test]
fn products_for_maps_classes_and_own_rows_and_names_the_unserved() {
    let dir = std::env::temp_dir().join(format!("athena-gen-4515-{}-unit", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let model = dir.join("model.ttl");
    std::fs::write(&model, format!("{}{WIDGET}", product_ttl())).unwrap();
    let (store, skipped) = athena_gen::model::load(&[model]).unwrap();
    assert!(skipped.is_empty(), "{skipped:?}");
    let card = dir.join("card.ttl");
    std::fs::write(&card, format!("{P}c:WidgetShape sh:targetClass c:Widget .\nc:Loose a owl:Class .\n")).unwrap();
    let t = touched(&[card]).unwrap();
    let (products, unserved) = products_for(&store, &t).unwrap();
    assert_eq!(products.get("gizmo").map(Vec::as_slice), Some(&["Widget".to_string()][..]));
    assert_eq!(unserved, vec!["Loose".to_string()]);
    let bad = dir.join("bad.ttl");
    std::fs::write(&bad, "c:X a owl:Class .\n").unwrap();
    let e = touched(&[bad]).unwrap_err().0;
    assert!(e.contains("bad.ttl does not parse"), "{e}");
}

// #4547 — a product that declares no settings is served by the v1 door, not
// generated. #4536 (Kade, 2026-10-15) changed the Test shape; --card mapped it onto
// chorus and werk, refused for missing settings, and once settings were added wrote a
// whole chorus API into the werk. A card now skips such a product by name.
const DOORWAY: &str = "c:doorway a c:Product ; c:hasDomain c:gadgets .\nc:gadgets c:definesVocabulary c:Gadget .\n\
c:Gadget a owl:Class .\nc:GadgetShape a sh:NodeShape ; sh:targetClass c:Gadget ;\n  sh:property [ sh:path c:weight ; sh:datatype xsd:integer ] .\n";

#[test]
fn a_card_touching_only_a_door_served_product_generates_nothing_and_passes() {
    let (base, w) = werk("doorway", "9106", &[
        ("roles/wren/ontology/doorway.ttl", format!("{P}{DOORWAY}")),
        ("roles/wren/ontology/doorway-change.ttl", format!("{P}c:GadgetShape sh:property [ sh:path c:colour ; sh:datatype xsd:string ] .\n")),
    ]);
    let (code, out) = run(&base, &["generate", "--card", "9106"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("doorway declares no settings — served by the v1 door"), "{out}");
    assert!(!w.join("directing/products/doorway").exists(), "nothing is generated for a door-served product: {out}");
}

#[test]
fn negative_proof_naming_a_door_served_product_still_refuses() {
    let (_base, w) = werk("doorway-named", "9107", &[("roles/wren/ontology/doorway.ttl", format!("{P}{DOORWAY}"))]);
    let out_dir = std::env::temp_dir().join(format!("athena-gen-4547-{}", std::process::id()));
    let o = Command::new(bin()).args(["generate", "--root", w.to_str().unwrap(), "--product", "doorway", "--out", out_dir.to_str().unwrap()])
        .output().unwrap();
    let text = format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr));
    assert_ne!(o.status.code(), Some(0), "{text}");
    assert!(text.contains("doorway declares no model.namespace"), "{text}");
}

// #4547 reopen 1 — #4536 run 5 (Kade, 2026-10-15): touching chorus.ttl made gen
// answer for the 26 old domainless classes already in it. A card answers only for
// the classes and rows whose triples changed against main.
const OLD_ORPHAN: &str = "c:OldOrphan a owl:Class .\n";

#[test]
fn old_domainless_classes_in_a_touched_file_are_not_the_cards() {
    let (base, w) = werk_with_main("old-orphan", "9108",
        &[("roles/wren/ontology/widgets.ttl", format!("{P}{WIDGET}{OLD_ORPHAN}"))],
        &[("roles/wren/ontology/widgets.ttl",
            format!("{P}{WIDGET}{OLD_ORPHAN}c:WidgetShape sh:property [ sh:path c:colour ; sh:datatype xsd:string ] .\n"))]);
    let (code, out) = run(&base, &["generate", "--card", "9108"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("card #9108: gizmo <- Widget"), "{out}");
    assert!(out.contains("1 class(es) unchanged from main"), "{out}");
    assert!(!out.contains("OldOrphan"), "an unchanged class is not named: {out}");
    assert!(w.join("directing/products/gizmo/api/Cargo.toml").is_file(), "{out}");
}

#[test]
fn negative_proof_a_new_domainless_class_in_the_same_file_is_still_refused() {
    let (base, w) = werk_with_main("new-orphan", "9109",
        &[("roles/wren/ontology/widgets.ttl", format!("{P}{WIDGET}{OLD_ORPHAN}"))],
        &[("roles/wren/ontology/widgets.ttl", format!("{P}{WIDGET}{OLD_ORPHAN}c:NewOrphan a owl:Class .\n"))]);
    let (code, out) = run(&base, &["generate", "--card", "9109"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("in no product's domain") && out.contains("NewOrphan"), "{out}");
    assert!(!out.contains("OldOrphan"), "{out}");
    assert!(!w.join("directing/products").exists());
}

// #4590 — #4590 run 1 (2026-10-17): the card changed only CoreShape, the shared
// shape every class shape names through sh:node, and gen refused it as touching
// no product. A changed shared shape reaches the classes whose shapes name it.
const SHARED: &str = "c:BaseShape a sh:NodeShape ; sh:property [ sh:path c:id ; sh:datatype xsd:string ] .\n";

#[test]
fn a_changed_shared_shape_reaches_the_products_of_the_classes_that_name_it() {
    let named = format!("{P}{WIDGET}c:WidgetShape sh:node c:BaseShape .\n");
    let (base, w) = werk_with_main("shared", "9110",
        &[("roles/wren/ontology/widgets.ttl", named.clone()), ("roles/wren/ontology/base.ttl", format!("{P}{SHARED}"))],
        &[("roles/wren/ontology/base.ttl", format!("{P}c:BaseShape a sh:NodeShape ; sh:property [ sh:path c:id ; sh:datatype xsd:integer ] .\n"))]);
    let (code, out) = run(&base, &["generate", "--card", "9110"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("1 class(es) reached through a changed shared shape"), "{out}");
    assert!(out.contains("card #9110: gizmo <- Widget"), "{out}");
    assert!(w.join("directing/products/gizmo/api/Cargo.toml").is_file(), "{out}");
}

#[test]
fn negative_proof_a_changed_shape_no_class_names_still_touches_no_product() {
    let (base, w) = werk_with_main("shared-unnamed", "9111",
        &[("roles/wren/ontology/base.ttl", format!("{P}{SHARED}"))],
        &[("roles/wren/ontology/base.ttl", format!("{P}c:BaseShape a sh:NodeShape ; sh:property [ sh:path c:id ; sh:datatype xsd:integer ] .\n"))]);
    let (code, out) = run(&base, &["generate", "--card", "9111"]);
    assert_eq!(code, 1, "{out}");
    assert!(out.contains("card #9111: the card's model change touches no product"), "{out}");
    assert!(!w.join("directing/products").exists());
}
