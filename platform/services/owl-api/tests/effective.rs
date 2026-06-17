// #3435 — effective-config read substrate (config-as-data).
// propertyValueType coercion: a Property's string-encoded value is coerced ONCE
// to its typed JSON form per propertyValueType (string | int | bool | json | list).
// Pure + hermetic — no Fuseki. This is the heart of AC#1 ("coerces ONCE per
// propertyValueType"); the SPARQL scope-chain fetch is the I/O half, tested live.
use owl_api::coerce_effective;

#[test]
fn coerce_int_emits_bare_number() {
    assert_eq!(coerce_effective("3000", "int").unwrap(), "3000");
    assert_eq!(coerce_effective("-7", "int").unwrap(), "-7");
}

#[test]
fn coerce_int_rejects_non_integer() {
    // "3e3" is a float/scientific form, not an integer literal — refuse, don't guess.
    assert!(coerce_effective("3e3", "int").is_err());
    assert!(coerce_effective("abc", "int").is_err());
    assert!(coerce_effective("", "int").is_err());
}

#[test]
fn coerce_bool_emits_bare_bool() {
    assert_eq!(coerce_effective("true", "bool").unwrap(), "true");
    assert_eq!(coerce_effective("false", "bool").unwrap(), "false");
    // Only the two canonical tokens — "yes"/"1"/"True" are refusals, not coercions.
    assert!(coerce_effective("yes", "bool").is_err());
    assert!(coerce_effective("True", "bool").is_err());
}

#[test]
fn coerce_string_quotes_and_escapes() {
    assert_eq!(coerce_effective("hello", "string").unwrap(), "\"hello\"");
    // embedded quote + backslash must be JSON-escaped so the response stays valid
    assert_eq!(coerce_effective("a\"b", "string").unwrap(), "\"a\\\"b\"");
    assert_eq!(coerce_effective("a\\b", "string").unwrap(), "\"a\\\\b\"");
}

#[test]
fn coerce_list_and_json_passthrough_shape_checked() {
    // list/json are stored already-encoded; coercion validates shape + passes through
    assert_eq!(coerce_effective("[\"a\",\"b\"]", "list").unwrap(), "[\"a\",\"b\"]");
    assert_eq!(coerce_effective("{\"k\":1}", "json").unwrap(), "{\"k\":1}");
    // a "list" whose value isn't a JSON array is a refusal, not a silent string
    assert!(coerce_effective("notalist", "list").is_err());
    assert!(coerce_effective("[1,2", "json").is_err());
}

#[test]
fn coerce_unknown_type_is_error() {
    // an unmodeled propertyValueType must fail loud, never default-to-string
    assert!(coerce_effective("x", "frobnicate").is_err());
}

// ── scope-chain row parse: the I/O half's pure seam ─────────────────────────
// The effective-config fetch CONCATs each Property as "iri|key|valueType|value"
// into ?v (owl-api's single-var ethos). value is LAST so an arbitrary config
// value may itself contain '|' — splitn(4) gives it the remainder.
use owl_api::parse_property_row;

#[test]
fn parse_row_splits_iri_key_type_value() {
    let p = parse_property_row("https://jeffbridwell.com/chorus#p1|alert.threshold|int|3000").unwrap();
    assert_eq!(p.iri, "https://jeffbridwell.com/chorus#p1");
    assert_eq!(p.key, "alert.threshold");
    assert_eq!(p.value_type, "int");
    assert_eq!(p.value, "3000");
}

#[test]
fn parse_row_value_may_contain_pipe() {
    // value takes the remainder — a piped value survives intact
    let p = parse_property_row("https://x#p|cmd|string|a|b|c").unwrap();
    assert_eq!(p.value, "a|b|c");
    assert_eq!(p.value_type, "string");
}

#[test]
fn parse_row_explicit_empty_value_ok() {
    // an explicit empty override is meaningful to the resolver, not malformed
    let p = parse_property_row("https://x#p|k|string|").unwrap();
    assert_eq!(p.value, "");
}

#[test]
fn parse_row_malformed_is_error() {
    assert!(parse_property_row("just-an-iri").is_err());
    assert!(parse_property_row("iri|key|valuetype").is_err()); // value field absent
    assert!(parse_property_row("|key|string|v").is_err());     // empty iri
}

// ── build_scope_node + the full PURE pipeline ───────────────────────────────
// build_scope_node assembles a node's fetched rows into a ScopeNode (kind is a
// parameter — the handler chooses it). The proof node is a TestCoverage instance
// carrying testType; for the 1-element chain kind only labels the winner (nothing
// to rank against), so the handler passes Service (leaf = most specific). The
// leaf-kind taxonomy is deferred to the ownership-walk follow-on (Kade's steer).
use owl_api::{build_scope_node, decide_effective_value, ScopeKind};

#[test]
fn build_scope_node_assembles_properties() {
    let rows = vec![
        "https://x#p1|testType|string|integration".to_string(),
        "https://x#p2|alert.threshold|int|3000".to_string(),
    ];
    let node = build_scope_node("https://x#node1", ScopeKind::Service, &rows).unwrap();
    assert_eq!(node.iri, "https://x#node1");
    assert_eq!(node.kind, ScopeKind::Service);
    assert_eq!(node.properties.len(), 2);
    assert_eq!(node.properties[0].key, "testType");
    assert_eq!(node.properties[0].value, "integration");
}

#[test]
fn build_scope_node_empty_rows_is_empty_node() {
    let node = build_scope_node("https://x#n", ScopeKind::Service, &[]).unwrap();
    assert!(node.properties.is_empty());
}

#[test]
fn build_scope_node_propagates_malformed_row() {
    // a malformed row fails loud — never a silently-dropped property
    let rows = vec!["garbage-no-delimiters".to_string()];
    assert!(build_scope_node("https://x#n", ScopeKind::Service, &rows).is_err());
}

#[test]
fn pure_pipeline_resolves_and_coerces_testtype() {
    // #3435 proof, end-to-end PURE: declared testType on a 1-element chain →
    // resolve → coerce. This is the whole substrate minus the Fuseki fetch.
    let rows = vec!["https://x#p|testType|string|integration".to_string()];
    let node = build_scope_node("https://x#testnode", ScopeKind::Service, &rows).unwrap();
    let res = decide_effective_value(&[node], "testType").unwrap().unwrap();
    assert_eq!(res.value, "integration");
    assert_eq!(res.value_type, "string");
    assert_eq!(coerce_effective(&res.value, &res.value_type).unwrap(), "\"integration\"");
}

#[test]
fn pure_pipeline_unset_key_is_none() {
    let rows = vec!["https://x#p|testType|string|unit".to_string()];
    let node = build_scope_node("https://x#n", ScopeKind::Service, &rows).unwrap();
    assert!(decide_effective_value(&[node], "missing.key").unwrap().is_none());
}

// ── query-builder: the no-projection guard, encoded as a test ───────────────
// These assertions ARE Kade's INVARIANT guard: the read must hit the live
// instances graph by traversing hasProperty→Property. If anyone later swaps in a
// projection/mirror/sqlite table, the graph + predicates change and this goes red.
use owl_api::{effective_fetch_query, NS};

#[test]
fn fetch_query_reads_instances_via_hasproperty_no_projection() {
    let q = effective_fetch_query("https://jeffbridwell.com/chorus#node1");
    // reads the LIVE instances graph — not a projection/mirror store
    assert!(q.contains("urn:chorus:instances"), "must read urn:chorus:instances live");
    // traverses hasProperty → Property{propertyKey, propertyValue, propertyValueType}
    assert!(q.contains(&format!("{NS}hasProperty")));
    assert!(q.contains(&format!("{NS}propertyKey")));
    assert!(q.contains(&format!("{NS}propertyValue")));
    assert!(q.contains(&format!("{NS}propertyValueType")));
}

#[test]
fn fetch_query_is_node_scoped_one_round_trip() {
    let q = effective_fetch_query("https://jeffbridwell.com/chorus#node1");
    // anchored on the node IRI — fetches THAT node's properties
    assert!(q.contains("https://jeffbridwell.com/chorus#node1"));
    // fetches the FULL property set — key-selection is pure code, NOT a SPARQL filter
    assert!(!q.contains("FILTER"), "key must be resolved in pure code, not filtered in SPARQL");
}

#[test]
fn fetch_query_row_contract_matches_parser() {
    let q = effective_fetch_query("https://jeffbridwell.com/chorus#node1");
    // CONCAT "iri|key|valueType|value" into the single ?v that select_v/parse_property_row consume
    assert!(q.contains("CONCAT"));
    assert!(q.contains("AS ?v"));
}

// ── effective_response: the handler's pure core (everything but sparql_json) ──
use owl_api::effective_response;

#[test]
fn effective_response_200_coerces_string() {
    let rows = vec!["https://x#p|testType|string|integration".to_string()];
    let (code, body) = effective_response("testnode", "testType", &rows);
    assert_eq!(code, 200);
    assert!(body.contains("\"value\":\"integration\""), "got: {body}");
    assert!(body.contains("\"key\":\"testType\""));
    assert!(body.contains("\"valueType\":\"string\""));
}

#[test]
fn effective_response_int_is_bare_number() {
    // dotted key flows through — it is NOT interpolated into SPARQL, only compared in code
    let rows = vec!["https://x#p|alert.threshold|int|3000".to_string()];
    let (code, body) = effective_response("n", "alert.threshold", &rows);
    assert_eq!(code, 200);
    assert!(body.contains("\"value\":3000"), "int must be a bare JSON number, got: {body}");
}

#[test]
fn effective_response_404_on_unset_key() {
    let rows = vec!["https://x#p|testType|string|unit".to_string()];
    let (code, body) = effective_response("n", "missing.key", &rows);
    assert_eq!(code, 404);
    assert!(body.contains("error"));
}

#[test]
fn effective_response_500_on_malformed_row() {
    let (code, _) = effective_response("n", "k", &["garbage".to_string()]);
    assert_eq!(code, 500);
}

// ── status-line serialization (live-verify caught 400 → 502 on the wire) ─────
// The handler returns 400 for bad input, but serve()'s inline status map had no
// 400 case → it serialized as "502 Bad Gateway". Hermetic handler tests assert the
// returned tuple code (correct); only a live request exposed the wire mismatch.
use owl_api::status_line;

#[test]
fn status_line_serializes_400_as_bad_request() {
    assert_eq!(status_line(400), "400 Bad Request");
    assert_eq!(status_line(200), "200 OK");
    assert_eq!(status_line(404), "404 Not Found");
    assert_eq!(status_line(500), "500 Internal Server Error");
    assert_eq!(status_line(502), "502 Bad Gateway");
    assert_eq!(status_line(999), "502 Bad Gateway"); // unknown → safe default
}
