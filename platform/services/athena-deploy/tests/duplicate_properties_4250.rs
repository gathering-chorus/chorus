//! #4250 — the deploy refuses a property declared more than once.
//!
//! The bug this guards never announced itself. Nine properties were declared
//! twice; what Jeff saw was four unrelated reds in three suites and four API
//! routes that silently did not exist. The guard has to fire on the state
//! itself, at the deploy, naming both lines.

use athena_deploy::duplicate_property_declarations;

fn f(label: &str, body: &str) -> (String, String) {
    (label.to_string(), body.to_string())
}

#[test]
fn one_declaration_per_property_is_not_an_offence() {
    let files = vec![
        f("a.ttl", "chorus:filePath a owl:DatatypeProperty ;\n    rdfs:range xsd:string .\n"),
        f("b.ttl", "chorus:fileType a owl:DatatypeProperty ;\n    rdfs:range xsd:string .\n"),
    ];
    assert_eq!(duplicate_property_declarations(&files), Vec::<String>::new());
}

#[test]
fn negative_proof_the_real_shape_is_caught_across_two_files() {
    // The exact state the store was in tonight: one declaration on CodeFile
    // with no writeOwner, another on File that carries it.
    let files = vec![
        f("roles/silas/ontology/chorus.ttl",
          "chorus:CodeFile a owl:Class .\n\
           chorus:filePath a owl:DatatypeProperty ;\n    rdfs:domain chorus:CodeFile .\n"),
        f("roles/kade/ontology/werk-domains.ttl",
          "chorus:filePath a owl:DatatypeProperty ;\n    chorus:writeOwner chorus:crawler .\n"),
    ];
    let hits = duplicate_property_declarations(&files);
    assert_eq!(hits.len(), 1, "one property is duplicated, got {hits:?}");
    let h = &hits[0];
    assert!(h.contains("chorus:filePath"), "names the property: {h}");
    assert!(h.contains("chorus.ttl:2"), "names the first line: {h}");
    assert!(h.contains("werk-domains.ttl:1"), "names the second line: {h}");
}

#[test]
fn negative_proof_three_declarations_are_reported_as_three() {
    let files = vec![
        f("a.ttl", "chorus:httpMethod a owl:DatatypeProperty .\n"),
        f("b.ttl", "chorus:httpMethod a owl:DatatypeProperty .\n"),
        f("c.ttl", "chorus:httpMethod a owl:DatatypeProperty .\n"),
    ];
    let hits = duplicate_property_declarations(&files);
    assert_eq!(hits.len(), 1);
    assert!(hits[0].contains("declared 3 times"), "{}", hits[0]);
}

#[test]
fn an_object_property_and_a_datatype_property_of_one_name_still_collide() {
    // chorus:result was exactly this — a datatype string in one file and an
    // object property pointing at GateResult in another. Different kinds is
    // worse than a duplicate, not better, so the guard must not skip it.
    let files = vec![
        f("a.ttl", "chorus:result a owl:DatatypeProperty ;\n    rdfs:range xsd:string .\n"),
        f("b.ttl", "chorus:result a owl:ObjectProperty ;\n    rdfs:range chorus:GateResult .\n"),
    ];
    assert_eq!(duplicate_property_declarations(&files).len(), 1);
}

#[test]
fn a_commented_out_declaration_is_not_a_declaration() {
    // The #3725 lesson: the first fixture written to prove a grep-based check
    // passed for the wrong reason, because the comment marking the omission
    // contained the string being matched.
    let files = vec![
        f("a.ttl", "chorus:filePath a owl:DatatypeProperty .\n"),
        f("b.ttl", "# chorus:filePath a owl:DatatypeProperty .  (retired #4250)\n"),
    ];
    assert_eq!(duplicate_property_declarations(&files), Vec::<String>::new());
}

#[test]
fn the_shipped_model_files_are_clean_of_this_today() {
    // Runs against the real tree, at run time — never env!(), which bakes in
    // the path of whichever werk compiled the binary (#4245).
    let crate_dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let root = format!("{crate_dir}/../../..");
    let mut files = Vec::new();
    for role in ["silas", "kade", "wren"] {
        let dir = format!("{root}/roles/{role}/ontology");
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for e in entries.flatten() {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("ttl") {
                let label = format!("roles/{role}/ontology/{}",
                    p.file_name().unwrap().to_string_lossy());
                files.push((label, std::fs::read_to_string(&p).unwrap_or_default()));
            }
        }
    }
    assert!(files.len() >= 5, "found only {} ttl files — the walk is wrong", files.len());
    assert_eq!(duplicate_property_declarations(&files), Vec::<String>::new());
}

