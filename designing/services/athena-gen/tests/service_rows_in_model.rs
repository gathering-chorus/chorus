// @domain: domains
// @test-type: unit — the repo's model through the generator's own functions; no service, no store
//! #5042 AC2 — the generator states the model facts that make a generated API a service
//! (surfaces/service.json: its Service row, the Domain row that hosts it, its launchd unit), and
//! the model the athena land loads holds exactly those facts: the Service row in
//! designing/data/service-instances.ttl, the hosts edge in roles/silas/ontology/chorus.ttl
//! (Silas, 11-27 12:09), and the unit tied to it in platform/config/service-instance-map.json.
//! For principles: Service principles-api, hosted by principles, unit com.chorus.principles-api;
//! for pulse: Service pulse-api, hosted by awareness, unit com.chorus.pulse-api.
//! NEGATIVE PROOF: with the hosts edge left out, or one field changed, the difference is named.
use oxigraph::model::{NamedNodeRef, Term};
use oxigraph::sparql::{QueryResults, SparqlEvaluator};
use oxigraph::store::Store;
use serde_json::Value;
use std::path::PathBuf;

const NS: &str = "https://jeffbridwell.com/chorus#";

fn repo() -> PathBuf {
    // run-time, not env!(): the werk-test 4030 guard
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR")).join("../../..")
}

fn model() -> Store {
    athena_gen::model::load(&athena_gen::model::model_files(&repo())).unwrap().0
}

/// What the generator states for a unit, as it writes it to surfaces/service.json.
fn stated(store: &Store, unit: &str, domain_unit: bool) -> Value {
    let p = if domain_unit { athena_gen::model::domain(store, unit) } else { athena_gen::model::product(store, unit) }
        .unwrap_or_else(|e| panic!("{}", e.0));
    let settings = athena_gen::model::settings(store, unit).unwrap();
    serde_json::from_str(&athena_gen::surfaces::service_json(&p, &settings)).unwrap()
}

/// Every value of `pred` on `subject`, in any graph, as text (an IRI by its local name).
fn values(store: &Store, subject: &str, pred: &str) -> Vec<String> {
    let q = format!("SELECT DISTINCT ?o WHERE {{ {{ <{NS}{subject}> <{NS}{pred}> ?o }} UNION {{ GRAPH ?g {{ <{NS}{subject}> <{NS}{pred}> ?o }} }} }}");
    let mut out = Vec::new();
    if let QueryResults::Solutions(sols) = SparqlEvaluator::new().parse_query(&q).unwrap().on_store(store).execute().unwrap() {
        for s in sols {
            let v = match s.unwrap().get("o").unwrap() {
                Term::NamedNode(n) => n.as_str().rsplit('#').next().unwrap_or("").to_string(),
                Term::Literal(l) => l.value().to_string(),
                other => other.to_string(),
            };
            if !out.contains(&v) {
                out.push(v);
            }
        }
    }
    out
}

/// Every difference between what the generator states for a unit and what the model holds.
fn faults(store: &Store, facts: &Value) -> Vec<String> {
    let mut out = Vec::new();
    let s = &facts["service"];
    let name = s["name"].as_str().unwrap();
    let iri = format!("service-{name}");
    if values(store, &iri, "label").is_empty() {
        return vec![format!("{iri}: no Service row in the model")];
    }
    for (k, v) in s.as_object().unwrap().iter().filter(|(k, _)| *k != "name") {
        let have = values(store, &iri, k);
        if have != vec![v.as_str().unwrap().to_string()] {
            out.push(format!("{iri}.{k}: the model holds {have:?}, the generator states {v}"));
        }
    }
    let host = facts["hostedBy"]["name"].as_str().unwrap();
    if !values(store, host, "hosts").contains(&iri) {
        out.push(format!("{host} does not host {iri}"));
    }
    out
}

const UNITS: [(&str, bool, &str); 2] = [("principles", true, "principles"), ("pulse", false, "awareness")];

#[test]
fn the_model_holds_what_the_generator_states_for_each_generated_api() {
    let store = model();
    let map: Value = serde_json::from_str(&std::fs::read_to_string(repo().join("platform/config/service-instance-map.json")).unwrap()).unwrap();
    for (unit, domain_unit, host) in UNITS {
        let facts = stated(&store, unit, domain_unit);
        assert_eq!(facts["hostedBy"]["name"], host, "{unit}: hosted by {}", facts["hostedBy"]);
        let f = faults(&store, &facts);
        assert!(f.is_empty(), "{unit}: the model and the generator differ:\n{}", f.join("\n"));
        // the unit is tied to the Service the generator names
        let want = format!("service-{}", facts["service"]["name"].as_str().unwrap());
        assert_eq!(map[facts["unit"].as_str().unwrap()], want.as_str(), "{unit}: the unit map");
        // and the generator wrote the same facts into the crate
        let dir = if domain_unit { format!("shaping/domains/{unit}/api") } else { format!("directing/products/{unit}/api") };
        let committed: Value = serde_json::from_str(&std::fs::read_to_string(repo().join(dir).join("surfaces/service.json")).unwrap()).unwrap();
        assert_eq!(committed, facts, "{unit}: surfaces/service.json is not what the generator writes today");
    }
}

#[test]
fn negative_proof_a_missing_hosts_edge_and_a_changed_field_are_named() {
    let store = model();
    let facts = stated(&store, "principles", true);
    let s = NamedNodeRef::new(&format!("{NS}principles")).unwrap().into_owned();
    let p = NamedNodeRef::new(&format!("{NS}hosts")).unwrap().into_owned();
    let o = NamedNodeRef::new(&format!("{NS}service-principles-api")).unwrap().into_owned();
    // the edge is held wherever the model put it: remove it from every graph
    let quads: Vec<_> = store.quads_for_pattern(Some(s.as_ref().into()), Some(p.as_ref()), Some(o.as_ref().into()), None).map(|q| q.unwrap()).collect();
    assert!(!quads.is_empty(), "the model holds no hosts edge for principles-api; this proof would pass vacuously");
    for q in &quads {
        store.remove(q).unwrap();
    }
    let f = faults(&store, &facts);
    assert!(f.iter().any(|x| x == "principles does not host service-principles-api"), "{f:?}");
    for q in &quads {
        store.insert(q).unwrap();
    }
    let mut changed = facts.clone();
    changed["service"]["overview"] = Value::String("hand-written".into());
    let f = faults(&store, &changed);
    assert!(f.iter().any(|x| x.starts_with("service-principles-api.overview:")), "{f:?}");
}
