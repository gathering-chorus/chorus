//! Phase A1 — entity-generic `add-batch` acceptance tests.
//!
//! These exercise the public library boundary with a hermetic Store and the
//! real binary's stdin parser. The load-bearing property is whole-batch
//! atomicity: every refusal occurs before the one and only Store::update.

use athena_model::{add_batch, verify_identity, write, Store, WriteReq, NS, R};
use std::cell::RefCell;
use std::io::Write as _;
use std::process::{Command, Stdio};

#[derive(Default)]
struct BatchStore {
    required: Vec<String>,
    datatypes: Vec<(String, String)>,
    plain_fields: Vec<String>,
    edge_classes: Vec<(String, String)>,
    unique_within: Vec<(String, String)>,
    unique_global: Vec<String>,
    /// Rows returned by the vectorized target query: `subject|rdf:type`.
    target_rows: Vec<String>,
    /// Fully minted subjects returned by the create-only existence query.
    existing_subjects: Vec<String>,
    /// Subjects that appear only after the conditional update, simulating a
    /// concurrent identity winner for post-proof diagnosis.
    post_update_existing_subjects: Vec<String>,
    existence_error: Option<String>,
    /// Candidate indexes returned by the vectorized uniqueness read.
    uniqueness_rows: Vec<String>,
    /// Candidate indexes that appear only after the update, simulating a
    /// concurrent different-subject uniqueness winner.
    post_update_uniqueness_rows: Vec<String>,
    proof_missing_subjects: Vec<String>,
    proof_error: Option<String>,
    asks: RefCell<Vec<String>>,
    selects: RefCell<Vec<String>>,
    updates: RefCell<Vec<String>>,
}

impl Store for BatchStore {
    fn ask(&self, sparql: &str) -> R<bool> {
        self.asks.borrow_mut().push(sparql.to_string());
        if sparql.contains("urn:chorus:domains:security") {
            return Ok(true); // verify_identity fixture
        }
        if sparql.contains("?other") {
            return Ok(false); // no collision in the pre-transaction store
        }
        Ok(false)
    }

    fn select_v(&self, sparql: &str) -> R<Vec<String>> {
        self.selects.borrow_mut().push(sparql.to_string());
        if sparql.contains("# athena-model create-only outcome proof") {
            if let Some(error) = &self.proof_error {
                return Err(error.clone());
            }
            return Ok(proof_subjects(sparql)
                .into_iter()
                .filter(|subject| !self.proof_missing_subjects.contains(subject))
                .collect());
        }
        if sparql.contains("# athena-model uniqueness candidates") {
            return Ok(if self.updates.borrow().is_empty() {
                self.uniqueness_rows.clone()
            } else {
                self.post_update_uniqueness_rows.clone()
            });
        }
        if sparql.contains("VALUES ?target") {
            return Ok(self.target_rows.clone());
        }
        if sparql.contains("VALUES ?candidate") {
            if let Some(error) = &self.existence_error {
                return Err(error.clone());
            }
            let subjects = if self.updates.borrow().is_empty() {
                &self.existing_subjects
            } else {
                &self.post_update_existing_subjects
            };
            return Ok(subjects
                .iter()
                .filter(|subject| sparql.contains(subject.as_str()))
                .cloned()
                .collect());
        }
        if sparql.contains("sh:minCount") {
            return Ok(self.required.clone());
        }
        if sparql.contains("sh:datatype") {
            let mut rows = self
                .datatypes
                .iter()
                .map(|(property, datatype)| format!("{}|{}", property, datatype))
                .collect::<Vec<_>>();
            rows.extend(self.plain_fields.iter().map(|property| format!("{}|", property)));
            return Ok(rows);
        }
        if sparql.contains("sh:class") {
            return Ok(self
                .edge_classes
                .iter()
                .map(|(property, class)| format!("{}|{}", property, class))
                .collect());
        }
        if sparql.contains("uniqueGlobal") {
            return Ok(self.unique_global.clone());
        }
        if sparql.contains("uniqueWithin") {
            return Ok(self
                .unique_within
                .iter()
                .map(|(property, partition)| format!("{}|{}", property, partition))
                .collect());
        }
        Ok(Vec::new())
    }

    fn update(&self, sparql: &str) -> R<()> {
        self.updates.borrow_mut().push(sparql.to_string());
        Ok(())
    }
}

fn proof_subjects(query: &str) -> Vec<String> {
    let block = query
        .split_once("VALUES (?candidateGraph ?candidate) {")
        .and_then(|(_, tail)| tail.split_once('}'))
        .map(|(values, _)| values)
        .unwrap_or("");
    block
        .split_whitespace()
        .collect::<Vec<_>>()
        .chunks(2)
        .filter_map(|pair| pair.get(1))
        .map(|term| term.trim_matches(|c| matches!(c, '<' | '>' | '(' | ')')).to_string())
        .collect()
}

fn identity(store: &BatchStore) -> athena_model::Identity {
    verify_identity(Some("kade"), store).expect("test principal verifies")
}

fn req(kind: &str, name: &str) -> WriteReq {
    WriteReq { kind: kind.into(), name: name.into(), ..Default::default() }
}

#[test]
fn batch_members_may_reference_each_other_and_commit_once_with_add_audit() {
    let store = BatchStore {
        edge_classes: vec![("partOf".into(), "Product".into())],
        ..Default::default()
    };
    let mut product = req("product", "Athena");
    product.fields.insert("vision".into(), "One governed model".into());
    let mut domain = req("domain", "tests");
    domain.edges.push(("partOf".into(), "product".into(), "Athena".into()));

    let report = add_batch(&store, &[product, domain], &identity(&store))
        .expect("the target arrives in the same transaction and has the required type");

    assert_eq!(report.subjects, vec![format!("{}athena", NS), format!("{}tests", NS)]);
    let updates = store.updates.borrow();
    assert_eq!(updates.len(), 1, "all entities must land through one Store::update");
    let update = &updates[0];
    assert_eq!(update.matches("DELETE WHERE").count(), 0, "create-only batch never deletes subjects");
    assert_eq!(update.matches("INSERT {").count(), 1, "one conditional transaction body");
    assert!(update.contains("FILTER NOT EXISTS"), "absence check and insert share one atomic update: {update}");
    assert!(update.contains("creator> \"kade\""), "verified add audit is retained: {update}");
    assert!(update.contains("created> \""), "created audit stamp is retained: {update}");

    let non_identity_asks = store
        .asks
        .borrow()
        .iter()
        .filter(|q| !q.contains("urn:chorus:domains:security"))
        .count();
    assert_eq!(non_identity_asks, 0, "an in-batch target needs no redundant store existence/type ASK");
}

#[test]
fn one_invalid_entity_refuses_the_whole_batch_and_names_it() {
    let store = BatchStore { required: vec!["vision".into()], ..Default::default() };
    let mut good = req("product", "good");
    good.fields.insert("vision".into(), "clear".into());
    let bad = req("product", "broken");

    let err = add_batch(&store, &[good, bad], &identity(&store))
        .expect_err("a missing required field must refuse every entity");

    assert!(err.contains("entity 'product:broken'"), "offending identity is explicit: {err}");
    assert!(err.contains("requires 'vision'"), "real validation cause survives: {err}");
    assert!(store.updates.borrow().is_empty(), "validation completes before the first update");
}

#[test]
fn sh_class_property_in_fields_refuses_the_whole_batch_before_update() {
    let store = BatchStore {
        edge_classes: vec![("ownedBy".into(), "Principal".into())],
        ..Default::default()
    };
    let mut invalid = req("domain", "literal-owner");
    invalid.fields.insert("ownedBy".into(), "principal-kade".into());

    let err = add_batch(&store, &[invalid], &identity(&store))
        .expect_err("an sh:class property cannot use the literal field channel");

    assert!(err.contains("entity 'domain:literal-owner'"), "offending identity is explicit: {err}");
    assert!(err.contains("sh:class Principal") && err.contains("through edges, not fields"), "{err}");
    assert!(store.updates.borrow().is_empty(), "channel refusal precedes every update");
    assert!(
        !store.selects.borrow().iter().any(|query| query.contains("VALUES ?target")),
        "channel mismatch refuses before target prefetch",
    );
}

#[test]
fn sh_datatype_property_in_edges_refuses_the_whole_batch_before_update() {
    let store = BatchStore {
        datatypes: vec![("externalId".into(), "string".into())],
        ..Default::default()
    };
    let mut invalid = req("role", "iri-external-id");
    invalid.edges.push(("externalId".into(), "role".into(), "other".into()));

    let err = add_batch(&store, &[invalid], &identity(&store))
        .expect_err("an sh:datatype property cannot use the object-edge channel");

    assert!(err.contains("entity 'role:iri-external-id'"), "offending identity is explicit: {err}");
    assert!(err.contains("modeled literal property") && err.contains("through fields, not edges"), "{err}");
    assert!(store.updates.borrow().is_empty(), "channel refusal precedes every update");
    assert!(
        !store.selects.borrow().iter().any(|query| query.contains("VALUES ?target")),
        "channel mismatch refuses before target prefetch",
    );
}

#[test]
fn plain_shape_property_in_edges_refuses_the_whole_batch_before_update() {
    let store = BatchStore {
        plain_fields: vec!["status".into()],
        ..Default::default()
    };
    let mut invalid = req("role", "iri-status");
    invalid.edges.push(("status".into(), "role".into(), "other".into()));

    let err = add_batch(&store, &[invalid], &identity(&store))
        .expect_err("a modeled plain property cannot use the object-edge channel");

    assert!(err.contains("entity 'role:iri-status'"), "offending identity is explicit: {err}");
    assert!(err.contains("modeled literal property") && err.contains("through fields, not edges"), "{err}");
    assert!(store.updates.borrow().is_empty(), "channel refusal precedes every update");
    assert!(
        !store.selects.borrow().iter().any(|query| query.contains("VALUES ?target")),
        "channel mismatch refuses before target prefetch",
    );
}

#[test]
fn normalized_duplicate_identities_are_refused_before_commit() {
    let store = BatchStore::default();
    let first = req("role", "Same Name");
    let second = req("role", "same-name");
    let err = add_batch(&store, &[first, second], &identity(&store)).unwrap_err();

    assert!(err.contains("entity 'role:same-name'"), "offending record is named: {err}");
    assert!(err.contains("'role:Same Name'") && err.contains("duplicate-identity"), "both claimants are named: {err}");
    assert!(store.updates.borrow().is_empty());
}

#[test]
fn in_batch_global_uniqueness_is_checked_against_final_state() {
    let store = BatchStore { unique_global: vec!["externalId".into()], ..Default::default() };
    let mut first = req("domain", "first");
    first.fields.insert("externalId".into(), "same".into());
    let mut second = req("domain", "second");
    second.fields.insert("externalId".into(), "same".into());

    let err = add_batch(&store, &[first, second], &identity(&store)).unwrap_err();
    assert!(err.contains("entity 'domain:second'"), "{err}");
    assert!(err.contains("also used by entity 'domain:first'") && err.contains("uniqueGlobal"), "{err}");
    assert!(store.updates.borrow().is_empty());
}

#[test]
fn in_batch_partition_uniqueness_is_checked_against_final_state() {
    let store = BatchStore {
        unique_within: vec![("rank".into(), "inGroup".into())],
        ..Default::default()
    };
    let product = req("product", "athena");
    let mut first = req("domain", "first");
    first.fields.insert("rank".into(), "1".into());
    first.edges.push(("inGroup".into(), "product".into(), "athena".into()));
    let mut second = req("domain", "second");
    second.fields.insert("rank".into(), "1".into());
    second.edges.push(("inGroup".into(), "product".into(), "athena".into()));

    let err = add_batch(&store, &[product, first, second], &identity(&store)).unwrap_err();
    assert!(err.contains("entity 'domain:second'"), "{err}");
    assert!(err.contains("also used by entity 'domain:first'") && err.contains("uniqueWithin"), "{err}");
    assert!(store.updates.borrow().is_empty());
}

#[test]
fn same_kind_shape_is_loaded_once_for_the_entire_batch() {
    let store = BatchStore::default();
    add_batch(&store, &[req("role", "one"), req("role", "two")], &identity(&store)).unwrap();

    let shape_selects = store
        .selects
        .borrow()
        .iter()
        .filter(|q| q.contains("urn:chorus:ontology") && q.contains("targetClass"))
        .count();
    assert_eq!(shape_selects, 6, "read_shape's six queries run once per distinct class, not once per entity");
    assert_eq!(store.updates.borrow().len(), 1);
}

#[test]
fn repeated_external_target_existence_and_type_checks_are_vectorized() {
    let target = format!("{}athena", NS);
    let store = BatchStore {
        edge_classes: vec![("partOf".into(), "Product".into())],
        target_rows: vec![format!("{}|{}Product", target, NS)],
        ..Default::default()
    };
    let mut one = req("domain", "one");
    one.edges.push(("partOf".into(), "product".into(), "athena".into()));
    let mut two = req("domain", "two");
    two.edges.push(("partOf".into(), "product".into(), "athena".into()));

    add_batch(&store, &[one, two], &identity(&store)).unwrap();
    let target_reads = store
        .selects
        .borrow()
        .iter()
        .filter(|q| q.contains("VALUES ?target"))
        .count();
    assert_eq!(target_reads, 1, "one subject+type read serves both entities");
}

#[test]
fn many_distinct_external_targets_still_use_one_target_read() {
    let count = 200usize;
    let mut target_rows = Vec::new();
    let mut requests = Vec::new();
    for index in 0..count {
        let target = format!("{}target-{}", NS, index); // Product is a bare-grain kind.
        target_rows.push(format!("{}|{}Product", target, NS));
        let mut entity = req("domain", &format!("source-{index}"));
        entity.edges.push(("partOf".into(), "product".into(), format!("target-{index}")));
        requests.push(entity);
    }
    let store = BatchStore {
        edge_classes: vec![("partOf".into(), "Product".into())],
        target_rows,
        ..Default::default()
    };

    let report = add_batch(&store, &requests, &identity(&store)).unwrap();
    assert_eq!(report.subjects.len(), count);
    let target_reads = store
        .selects
        .borrow()
        .iter()
        .filter(|query| query.contains("VALUES ?target"))
        .count();
    assert_eq!(target_reads, 1, "target reads must be O(1), not O(entity count)");
    let existence_reads = store
        .selects
        .borrow()
        .iter()
        .filter(|query| query.contains("VALUES ?candidate"))
        .count();
    assert_eq!(existence_reads, 1, "same-graph create-only existence is one read, not O(entity count)");
    assert_eq!(store.updates.borrow().len(), 1);
    assert!(!store.updates.borrow()[0].contains("DELETE WHERE"), "create-only commit is insert-only");
}

#[test]
fn prefixed_minted_identity_is_refused_create_only_with_one_read() {
    let minted = format!("{}test-result-existing", NS);
    let store = BatchStore {
        existing_subjects: vec![minted.clone()],
        ..Default::default()
    };

    let err = add_batch(&store, &[req("test-result", "existing")], &identity(&store))
        .expect_err("the correctly minted prefixed identity already exists");

    assert!(err.contains("entity 'test-result:existing'"), "offending input identity is named: {err}");
    assert!(err.contains("already-exists"), "typed create-only conflict: {err}");
    assert!(err.contains(&minted), "the authoritative minted subject is named: {err}");
    let selects = store.selects.borrow();
    let reads = selects
        .iter()
        .filter(|query| query.contains("VALUES ?candidate"))
        .collect::<Vec<_>>();
    assert_eq!(reads.len(), 1, "one existence read per graph");
    assert!(reads[0].contains(&format!("<{}>", minted)), "query uses the minted prefixed subject: {}", reads[0]);
    assert!(!reads[0].contains(&format!("<{}existing>", NS)), "raw caller name must never be queried: {}", reads[0]);
    assert!(store.updates.borrow().is_empty(), "existing identity refuses before commit");
}

#[test]
fn create_only_existence_read_fails_closed_and_names_entity() {
    let store = BatchStore {
        existence_error: Some("fuseki-read-unavailable".into()),
        ..Default::default()
    };

    let err = add_batch(&store, &[req("role", "first")], &identity(&store))
        .expect_err("an unknown model state must refuse create-only insertion");

    assert!(err.contains("entity 'role:first'"), "the affected identity is named: {err}");
    assert!(err.contains("fuseki-read-unavailable"), "the store cause survives: {err}");
    assert!(store.updates.borrow().is_empty());
}

#[test]
fn first_existing_identity_is_reported_in_input_order() {
    let first = format!("{}role-first", NS);
    let second = format!("{}role-second", NS);
    let store = BatchStore {
        // Deliberately reverse store row order: request order owns reporting.
        existing_subjects: vec![second, first],
        ..Default::default()
    };

    let err = add_batch(
        &store,
        &[req("role", "first"), req("role", "second")],
        &identity(&store),
    )
    .expect_err("both identities already exist");

    assert!(err.contains("entity 'role:first'"), "first request conflict wins: {err}");
    assert!(!err.contains("entity 'role:second'"), "store row ordering must not choose the error: {err}");
    assert!(store.updates.borrow().is_empty());
}

#[test]
fn caller_controlled_field_and_edge_names_cannot_escape_sparql() {
    for (label, request) in [
        (
            "field",
            {
                let mut request = req("role", "bad-field");
                request.fields.insert(
                    "label> <urn:attacker:predicate".into(),
                    "payload".into(),
                );
                request
            },
        ),
        (
            "edge",
            {
                let mut request = req("role", "bad-edge");
                request.edges.push((
                    "partOf> } INSERT DATA { <urn:x> <urn:y> <urn:z>".into(),
                    "product".into(),
                    "athena".into(),
                ));
                request
            },
        ),
    ] {
        let store = BatchStore::default();
        let single_err = write(&store, &request, &identity(&store))
            .expect_err("single write must refuse an injection-shaped property local name");
        assert!(single_err.contains("bad-property"), "single {label} refusal is typed: {single_err}");
        assert!(store.updates.borrow().is_empty(), "single {label} refusal writes nothing");
        let err = add_batch(&store, &[request], &identity(&store))
            .expect_err("an injection-shaped property local name must be refused");
        assert!(err.contains("bad-property"), "{label} refusal is typed: {err}");
        assert!(store.updates.borrow().is_empty(), "{label} refusal writes nothing");
    }
}

#[test]
fn write_and_add_batch_refuse_off_realm_or_malformed_instance_graphs() {
    for graph in [
        "urn:gathering:icd",
        "https://attacker.invalid/graph",
        "urn:chorus:domain:tests",
        "urn:chorus:domains:tests> } INSERT DATA { <urn:x> <urn:y> <urn:z>",
    ] {
        let single_store = BatchStore::default();
        let mut single = req("role", "single-off-realm");
        single.graph = Some(graph.into());
        let single_err = write(&single_store, &single, &identity(&single_store))
            .expect_err("single write must reject an unsafe graph");
        assert!(
            single_err.contains("graph-not-instance-home"),
            "single graph refusal names the gate for {graph}: {single_err}"
        );
        assert!(single_store.updates.borrow().is_empty());

        let batch_store = BatchStore::default();
        let mut batch = req("role", "batch-off-realm");
        batch.graph = Some(graph.into());
        let batch_err = add_batch(&batch_store, &[batch], &identity(&batch_store))
            .expect_err("batch write must reject an unsafe graph");
        assert!(
            batch_err.contains("graph-not-instance-home"),
            "batch graph refusal names the gate for {graph}: {batch_err}"
        );
        assert!(batch_store.updates.borrow().is_empty());
    }
}

#[test]
fn many_unique_fields_use_one_store_read_not_one_ask_each() {
    let count = 200usize;
    let store = BatchStore {
        unique_global: vec!["externalId".into()],
        ..Default::default()
    };
    let requests = (0..count)
        .map(|index| {
            let mut request = req("role", &format!("unique-{index}"));
            request
                .fields
                .insert("externalId".into(), format!("external-{index}"));
            request
        })
        .collect::<Vec<_>>();

    add_batch(&store, &requests, &identity(&store)).expect("all values are unique");

    let selects = store.selects.borrow();
    let uniqueness_queries = selects
        .iter()
        .filter(|query| query.contains("# athena-model uniqueness candidates"))
        .collect::<Vec<_>>();
    assert_eq!(uniqueness_queries.len(), 1, "all candidate values share one SELECT");
    assert!(
        uniqueness_queries[0].len() < 100_000,
        "candidate and replacement VALUES tables must grow O(N), not repeat an O(N) exclusion in every arm ({} bytes)",
        uniqueness_queries[0].len(),
    );
    let uniqueness_asks = store
        .asks
        .borrow()
        .iter()
        .filter(|query| query.contains("?other"))
        .count();
    assert_eq!(uniqueness_asks, 0, "batch uniqueness never shells one ASK per entity");
}

#[test]
fn vectorized_uniqueness_conflict_names_the_indexed_entity_and_writes_nothing() {
    let store = BatchStore {
        unique_global: vec!["externalId".into()],
        uniqueness_rows: vec!["1".into()],
        ..Default::default()
    };
    let mut first = req("role", "first-external");
    first.fields.insert("externalId".into(), "available".into());
    let mut second = req("role", "second-external");
    second.fields.insert("externalId".into(), "taken".into());

    let err = add_batch(&store, &[first, second], &identity(&store))
        .expect_err("the store-reported second candidate conflicts");

    assert!(err.contains("entity 'role:second-external'"), "{err}");
    assert!(err.contains("duplicate 'externalId'") && err.contains("uniqueGlobal"), "{err}");
    assert!(store.updates.borrow().is_empty(), "uniqueness refusal precedes mutation");
}

#[test]
fn conditional_insert_is_atomic_and_missing_outcome_proof_fails_closed() {
    let missing = format!("{}role-raced", NS);
    let store = BatchStore {
        proof_missing_subjects: vec![missing.clone()],
        post_update_existing_subjects: vec![missing.clone()],
        ..Default::default()
    };

    let err = add_batch(&store, &[req("role", "raced")], &identity(&store))
        .expect_err("a conditional no-op must not be reported as a successful create");

    assert!(err.contains("already-exists") && err.contains("role:raced"), "{err}");
    let updates = store.updates.borrow();
    assert_eq!(updates.len(), 1, "absence check and insertion are one store update");
    assert!(updates[0].contains("INSERT {") && updates[0].contains("FILTER NOT EXISTS"));
    assert!(updates[0].contains(&format!("<{}>", missing)));
    assert!(!updates[0].contains("DELETE WHERE"), "create-only race handling is non-destructive");
}

#[test]
fn concurrent_global_uniqueness_winner_blocks_the_same_atomic_insert() {
    let subject = format!("{}role-global-race", NS);
    let store = BatchStore {
        unique_global: vec!["externalId".into()],
        proof_missing_subjects: vec![subject],
        post_update_uniqueness_rows: vec!["0".into()],
        ..Default::default()
    };
    let mut request = req("role", "global-race");
    request.fields.insert("externalId".into(), "shared-value".into());

    let err = add_batch(&store, &[request], &identity(&store))
        .expect_err("a concurrent different-subject global duplicate must win atomically");

    assert!(err.contains("concurrent-uniqueness-conflict"), "{err}");
    assert!(err.contains("externalId") && err.contains("across all role"), "{err}");
    let updates = store.updates.borrow();
    assert_eq!(updates.len(), 1);
    let update = &updates[0];
    assert!(update.contains("# athena-model atomic uniqueness guard"));
    assert!(update.contains("VALUES (?v ?graph ?class ?property ?wanted)"));
    assert!(update.contains(&format!("<{}externalId>", NS)));
    assert!(update.contains("\"shared-value\""));
    assert!(update.matches("FILTER NOT EXISTS").count() >= 2, "identity and uniqueness are gated in one WHERE: {update}");
}

#[test]
fn concurrent_partition_uniqueness_winner_blocks_the_same_atomic_insert() {
    let domain_subject = format!("{}partition-race", NS);
    let store = BatchStore {
        unique_within: vec![("rank".into(), "inGroup".into())],
        proof_missing_subjects: vec![domain_subject],
        post_update_uniqueness_rows: vec!["0".into()],
        ..Default::default()
    };
    let product = req("product", "athena");
    let mut domain = req("domain", "partition-race");
    domain.fields.insert("rank".into(), "1".into());
    domain.edges.push(("inGroup".into(), "product".into(), "athena".into()));

    let err = add_batch(&store, &[product, domain], &identity(&store))
        .expect_err("a concurrent duplicate in the same partition must win atomically");

    assert!(err.contains("concurrent-uniqueness-conflict"), "{err}");
    assert!(err.contains("rank") && err.contains("within 'inGroup'"), "{err}");
    let updates = store.updates.borrow();
    assert_eq!(updates.len(), 1);
    let update = &updates[0];
    assert!(update.contains("VALUES (?v ?graph ?property ?partition ?partitionIri ?wanted)"));
    assert!(update.contains(&format!("<{}rank>", NS)));
    assert!(update.contains(&format!("<{}inGroup>", NS)));
    assert!(update.contains(&format!("<{}athena>", NS)));
    assert!(update.contains("\"1\""));
}

#[test]
fn missing_unique_within_partition_refuses_single_and_batch_before_update() {
    let mut request = req("domain", "unscoped-rank");
    request.fields.insert("rank".into(), "1".into());

    let single_store = BatchStore {
        unique_within: vec![("rank".into(), "inGroup".into())],
        ..Default::default()
    };
    let single_err = write(&single_store, &request, &identity(&single_store))
        .expect_err("single add cannot silently skip a declared uniqueness partition");
    assert!(single_err.contains("uniqueWithin 'inGroup'") && single_err.contains("no 'inGroup' edge"), "{single_err}");
    assert!(single_store.updates.borrow().is_empty());

    let batch_store = BatchStore {
        unique_within: vec![("rank".into(), "inGroup".into())],
        ..Default::default()
    };
    let batch_err = add_batch(&batch_store, &[request], &identity(&batch_store))
        .expect_err("batch add cannot silently skip a declared uniqueness partition");
    assert!(batch_err.contains("entity 'domain:unscoped-rank'"), "{batch_err}");
    assert!(batch_err.contains("uniqueWithin 'inGroup'") && batch_err.contains("no 'inGroup' edge"), "{batch_err}");
    assert!(batch_store.updates.borrow().is_empty());
}

fn run_cli(stdin: &str) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_athena-model"))
        .arg("add-batch")
        .env_remove("CHORUS_IDENTITY_TOKEN")
        .env("CHORUS_FUSEKI", "http://127.0.0.1:1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("athena-model binary starts");
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    child.wait_with_output().unwrap()
}

#[test]
fn cli_reads_strict_ndjson_before_identity_or_store_contact() {
    let malformed = concat!(
        "{\"kind\":\"role\",\"name\":\"one\"}\n",
        "{\"kind\":\"role\",\"name\":\"two\",\"unknown\":true}\n",
    );
    let out = run_cli(malformed);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success());
    assert!(stderr.contains("invalid NDJSON record at line 2"), "{stderr}");
    assert!(!stderr.contains("identity-token-required"), "the full input is parsed before identity resolution: {stderr}");
    assert!(!stderr.to_lowercase().contains("fuseki"), "parse refusal makes no store contact: {stderr}");
}

#[test]
fn cli_accepts_write_req_tuple_edge_wire_shape_then_reaches_identity_gate() {
    let valid = concat!(
        "{\"kind\":\"product\",\"name\":\"athena\"}\n",
        "{\"kind\":\"domain\",\"name\":\"tests\",\"edges\":[[\"partOf\",\"product\",\"athena\"]]}\n",
    );
    let out = run_cli(valid);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(!out.status.success(), "no identity token means no write");
    assert!(stderr.contains("identity-token-required"), "valid NDJSON reaches the normal add identity gate: {stderr}");
    assert!(!stderr.contains("invalid NDJSON"), "tuple-edge contract parsed: {stderr}");
}
