//! #3467 — athena-make projects two MORE surfaces from the model, alongside
//! routes/openapi/page: a TEST manifest (unit + conformance + security) and the
//! ADR-031 MCP tool binding. Both pure functions of the RouteTable. Red-first.

use athena_make::{mcp_binding, tests_manifest, RouteTable};

fn fixture() -> RouteTable {
    RouteTable { unbounded: vec![], write_authority: String::new(), domain: String::new(), base_path: String::new(),
        class: "https://jeffbridwell.com/chorus#Domain".into(),
        fields: vec![
            "label|datatype:string".into(),
            "comment|datatype:string".into(),
            "port|datatype:integer".into(),   // a STRICT datatype → a wrong value is rejectable
            "partOf|edge:Product".into(),     // an edge → target-type is enforced
        ],
        routes: vec![
            // #4237 — the fixture now declares the full verb set, because a real
            // generated class does (Domain carries PUT and DELETE). The quartet is
            // read OFF this list, so a fixture missing them was asserting against a
            // route table no class actually has.
            "GET /domains".into(),
            "GET /domains/:name".into(),
            "POST /domains".into(),
            "PUT /domains/:name".into(),
            "DELETE /domains/:name".into(),
            "POST /domains/:name/partof".into(),
            "GET /schema/domain".into(),
        ],
        secured: vec!["/schema/domain".into()],
        mandatory: vec!["label".into(), "comment".into()],
        write_required: vec!["label".into(), "comment".into()], allowed_values: vec![],
        repo_target: "generated/domain".into(),  // #3488 — repo land location
        exposure: vec![],
        instances_graph: "urn:chorus:instances".into(),  // #3570 — default home (back-compat)
        tree_edges: vec![],
        tree_order: None, model_version: "v2".to_string(),
    }
}

#[test]
fn tests_manifest_projects_unit_conformance_security() {
    let m = tests_manifest(&fixture());
    // identity
    assert!(m.contains("\"class\""), "names the class");
    assert!(m.contains("Domain"));
    // unit snapshot carries the projected route/mandatory/secured sets
    assert!(m.contains("\"unit\"") && m.contains("\"mandatory\"") && m.contains("label"));
    // conformance: the list route asserts 200
    assert!(m.contains("\"conformance\"") && m.contains("\"GET /domains\"") || m.contains("\"GET\""));
    assert!(m.contains("200"));
    // security from the model: unauth write → 401
    assert!(m.contains("401"), "unauth write must assert 401");
    // secured surface (sh:requiresAuth-projected) → 401
    assert!(m.contains("/schema/domain"));
    // injection guard → 400
    assert!(m.contains("400"), "injection name must assert 400");
    // completeness floor → incomplete create 422
    assert!(m.contains("422"), "incomplete create must assert 422");
    // #3467 finish — the generated tests ASSERT the new constraint-enforcement:
    // datatype rejection (a strict-typed field gets a bad value → 422) and
    // edge-target-type rejection (an edge points at a wrong-typed target → 422).
    assert!(m.contains("\"constraints\""), "manifest carries a constraints block");
    assert!(m.contains("datatype") && m.contains("port"), "datatype-rejection case for the strict field 'port'");
    assert!(m.contains("edge-target-type") && m.contains("partOf"), "edge-target-type rejection case for the partOf edge");
}

#[test]
fn mcp_binding_is_adr031_conformant() {
    let b = mcp_binding(&fixture());
    // ADR-031 shape: chorus_<plural-resource>_<verb>, closed verb set get/list/add
    assert!(b.contains("chorus_domains_get"), "get tool");
    assert!(b.contains("chorus_domains_list"), "list tool");
    assert!(b.contains("chorus_domains_add"), "add tool");
    // no bare verbs / verb-first / actor suffixes — every tool starts chorus_domains_
    // add delegates to the DAL (the one write authority)
    assert!(b.to_lowercase().contains("dal"), "add delegates to the DAL");
    // pluralized resource, not the bare class
    assert!(!b.contains("chorus_domain_get"), "must pluralize the resource (domains, not domain)");
}

// #3482 (folded into #3488) — the ADR-031 name GATE: not just "the expected
// names exist" but "EVERY generated tool name conforms" — so a future generator
// change that emits a non-conformant name (verb-first, bad verb, un-pluralized,
// uppercase) FAILS here instead of drifting silently. Pure check over the
// generated binding; no regex crate (athena-make is zero-dep).
#[test]
fn every_generated_mcp_name_obeys_adr031_grain() {
    let b = mcp_binding(&fixture());
    // extract every "name": "<tool>" value
    let names: Vec<String> = b
        .match_indices("\"name\":")
        .filter_map(|(i, _)| {
            let after = &b[i + 7..];
            let start = after.find('"')? + 1;
            let end = after[start..].find('"')? + start;
            Some(after[start..end].to_string())
        })
        .collect();
    assert!(!names.is_empty(), "binding must emit at least one tool name");
    let verbs = ["get", "list", "add"];
    for n in &names {
        let parts: Vec<&str> = n.split('_').collect();
        assert_eq!(parts.len(), 3, "ADR-031 grain is chorus_<resource>_<verb>: '{}'", n);
        assert_eq!(parts[0], "chorus", "must be chorus-namespaced: '{}'", n);
        assert!(verbs.contains(&parts[2]), "verb must be one of {:?}: '{}'", verbs, n);
        let resource = parts[1];
        assert!(
            !resource.is_empty()
                && resource.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
            "resource must be lowercase alnum: '{}'", n
        );
        assert!(resource.ends_with('s'), "resource must be pluralized: '{}'", n);
    }
}

// --- #4237 — the quartet per endpoint -------------------------------------
//
// Jeff, 2026-09-20: "the api must apply all data types shacl etc that defines
// rhe contract general pattern is create update get delete per endpoint".
//
// Before this, the manifest emitted GET conformance plus four security cases and
// two constraint cases. Nothing for PUT. Nothing for DELETE. Nothing that wrote a
// row and read it back. A status code says the door answered; only a read-back
// says it kept what it was given — that is the class #4167 found, where the store
// held five hasDomain edges and the API returned none.
//
// These tests are the shape of the contract, asserted on the projection, not on a
// live server: the manifest is a pure function of the RouteTable.

#[test]
fn quartet_emits_a_case_for_every_verb() {
    let m = tests_manifest(&fixture());
    assert!(m.contains("\"quartet\""), "manifest carries a quartet block");
    for id in ["create", "read", "update", "delete"] {
        assert!(m.contains(&format!("\"step\": \"{id}\"")), "quartet is missing the {id} step");
    }
    assert!(m.contains("\"method\": \"PUT\""), "no PUT case — update was never generated");
    assert!(m.contains("\"method\": \"DELETE\""), "no DELETE case — delete was never generated");
}

#[test]
fn the_write_reads_its_row_back_field_by_field() {
    let m = tests_manifest(&fixture());
    assert!(m.contains("\"readBack\""), "the create step must read its row back");
    // every write-required field is compared, not just the status code
    assert!(m.contains("\"compareFields\""), "read-back must name the fields it compares");
    assert!(m.contains("label") && m.contains("comment"), "write-required fields are compared");
}

#[test]
fn the_quartet_cleans_up_after_itself() {
    let m = tests_manifest(&fixture());
    assert!(m.contains("\"throwawaySubject\""), "the quartet works on a throwaway subject");
    assert!(m.contains("\"refuseIfNoCleanup\": true"),
        "a generated write case that cannot clean up must refuse to run, not leave a row");
}

#[test]
fn the_refusal_set_is_generated_from_the_shape() {
    let m = tests_manifest(&fixture());
    // Jeff's list: unauthenticated, wrong owner, malformed name, incomplete body,
    // undeclared field, wrong datatype.
    for refusal in [
        "unauth-create-401",
        "wrong-owner-403",
        "injection-name-400",
        "incomplete-create-422",
        "undeclared-field-422",
    ] {
        assert!(m.contains(refusal), "refusal case missing: {refusal}");
    }
    // the wrong-datatype refusal already existed as a constraint case
    assert!(m.contains("datatype-reject"), "datatype refusal missing");
}

#[test]
fn negative_proof_a_class_with_no_write_required_fields_emits_no_hollow_readback() {
    // If a class declares nothing required, a read-back that compares zero fields
    // would pass for every response — a green that cannot go red. The generator
    // must say so rather than emit an empty comparison.
    let mut t = fixture();
    t.write_required = vec![];
    t.mandatory = vec![];
    let m = tests_manifest(&t);
    assert!(m.contains("\"readBack\": null") || m.contains("\"readBackSkipped\""),
        "with no required fields the read-back must be explicitly absent, never an empty compare");
    assert!(!m.contains("\"compareFields\": []"),
        "an empty compareFields list is a hollow check and must not be emitted");
}

#[test]
fn no_case_is_invented_for_a_verb_the_model_does_not_declare() {
    // The quartet takes its paths from the route table. A class whose model
    // declares no DELETE must produce no delete case — not a case against a
    // guessed URL, which would 404 forever and read as "the door refused it".
    // That is the hollow-check shape: a red that means nothing and a green that
    // means less.
    let mut t = fixture();
    t.routes.retain(|r| !r.starts_with("DELETE "));
    let m = tests_manifest(&t);
    assert!(!m.contains("\"step\": \"delete\""),
        "a delete case was emitted for a class with no DELETE route");
    assert!(m.contains("\"step\": \"create\""), "the rest of the quartet still stands");
}

// --- #4259 — refusals address a real route, or are not emitted ------------
//
// Kade, cold-eyes on #4237's land: the five refusal cases were built from
// "/{plural}" while the quartet read the route table, so for Domain they
// addressed /domains and the served route is /domains/domains. A security case
// pointing at a path the API does not have measures the router, not the door —
// it cannot go red on a real authz failure and cannot go green either.
//
// My own proof missed it because the loopback stub answered any path. These two
// tests are what that stub could not be.

// Extract the "path" value of each refusal case, exactly — not by substring.
// My first cut of these tests asserted !contains("\"path\": \"/domains/bad%20name\"")
// and that string IS a substring of "/domains/domains/bad%20name", so the test
// failed against a generator that was already correct. A substring assertion
// cannot tell a path from a path that ends with it — the same defect these tests
// exist to catch, in the test.
fn refusal_paths(manifest: &str) -> Vec<(String, String)> {
    let mut out = vec![];
    for line in manifest.lines() {
        let l = line.trim();
        if !l.starts_with("{ \"id\"") { continue; }
        let id = l.split("\"id\": \"").nth(1).and_then(|r| r.split('"').next()).unwrap_or("").to_string();
        if id.starts_with("conform ") { continue; }
        let path = l.split("\"path\": \"").nth(1).and_then(|r| r.split('"').next()).unwrap_or("").to_string();
        out.push((id, path));
    }
    out
}

#[test]
fn every_refusal_case_addresses_a_declared_route() {
    let m = tests_manifest(&fixture());
    let paths = refusal_paths(&m);
    for id in ["unauth-create-401", "injection-name-400", "incomplete-create-422", "undeclared-field-422"] {
        assert!(paths.iter().any(|(i, _)| i == id), "refusal case missing: {id}");
    }
    for (id, path) in &paths {
        assert!(!path.contains(":name"), "refusal {id} carries an unsubstituted :name path: {path}");
    }
}

#[test]
fn refusals_follow_the_base_path_not_the_plural() {
    let mut t = fixture();
    t.routes = vec![
        "GET /domains/domains".into(),
        "GET /domains/domains/:name".into(),
        "POST /domains/domains".into(),
        "PUT /domains/domains/:name".into(),
        "DELETE /domains/domains/:name".into(),
    ];
    let m = tests_manifest(&t);
    for (id, path) in refusal_paths(&m) {
        // secured-401 cases address a declared secured SURFACE (/schema/domain),
        // which is a real route and deliberately outside the resource base path.
        // Excluded by what it is, not by name-matching the one in this fixture.
        if id.starts_with("secured-401 ") { continue; }
        assert!(path.starts_with("/domains/domains"),
            "refusal {id} addresses {path}, outside the declared base path /domains/domains");
    }
}

#[test]
fn negative_proof_no_wrong_owner_case_without_a_declared_put() {
    // If the model declares no PUT, emitting wrong-owner-403 against a guessed
    // path produces a case that 404s forever and reads as "the door refused".
    let mut t = fixture();
    t.routes.retain(|r| !r.starts_with("PUT "));
    let m = tests_manifest(&t);
    assert!(!m.contains("wrong-owner-403"),
        "a wrong-owner case was emitted for a class with no PUT route");
    assert!(m.contains("unauth-create-401"), "the rest of the refusal set still stands");
}
