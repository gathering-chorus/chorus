//! #3437 demo — what the effective-config endpoint returns, driven through the
//! REAL cascade resolver. This is the response body Properties A (#3435) will
//! serve at `GET /effective/:node/:key`; here the bindings are constructed in
//! code (no chorus:Property instances exist in the graph yet — landing the first
//! real one is #3438) so you can see the resolver respond with data NOW.
//!
//! Run:  cargo run --example effective_response

use owl_api::cascade::{resolve, Binding, Effective, Level};

/// The JSON body A serves: the effective value + WHERE it resolved (provenance)
/// + the node/key asked. coerce() shows the ONE coercion A applies on the winner.
fn response_json(node: &str, key: &str, eff: &Option<Effective>) -> String {
    match eff {
        Some(e) => format!(
            "{{ \"node\": \"{}\", \"key\": \"{}\", \"value\": {}, \"valueType\": \"{}\", \"resolvedAt\": \"{:?}\", \"found\": true }}",
            node, key, coerce(&e.value, &e.value_type), e.value_type, e.level
        ),
        None => format!(
            "{{ \"node\": \"{}\", \"key\": \"{}\", \"found\": false, \"note\": \"unset on the ancestry — caller uses the code default\" }}",
            node, key
        ),
    }
}

/// The ONE coercion A applies on the resolved winner (per propertyValueType).
/// Shown here so the response is realistic; the resolver itself stays type-blind.
fn coerce(value: &str, value_type: &str) -> String {
    match value_type {
        "int" | "bool" | "json" => value.to_string(), // emit bare (number/bool/object)
        "list" => format!("[{}]", value.split(',').map(|s| format!("\"{}\"", s.trim())).collect::<Vec<_>>().join(", ")),
        _ => format!("\"{}\"", value), // string: quoted
    }
}

fn request(node: &str, key: &str, bindings: &[Binding]) {
    let eff = resolve(key, bindings);
    println!("GET /effective/{}/{}", node, key);
    println!("200 OK");
    println!("{}\n", response_json(node, key, &eff));
}

fn main() {
    println!("=== owl-api effective-config — cascade resolver (#3437) ===\n");

    // Scenario: the `crawler-index` SERVICE lives in the `chorus` DOMAIN, inside
    // the `borg` PRODUCT, on the `reflecting` VALUE-STREAM. These are the Property
    // bindings A would gather by walking that service's containment ancestry.
    let crawler_index = vec![
        // alert.threshold: set broadly at the Product, overridden tighter at the Service.
        Binding::new("alert.threshold", "0.90", "int", Level::Product),
        Binding::new("alert.threshold", "0.80", "int", Level::Service),
        // deploy.timeout: set ONLY at the Product — the service inherits it.
        Binding::new("deploy.timeout", "30", "int", Level::Product),
        // crawl.stale_sec: a Domain-wide value, no closer override.
        Binding::new("crawl.stale_sec", "3000", "int", Level::Domain),
        // feature.experimental: a list, set at the ValueStream (the least specific rung).
        Binding::new("feature.experimental", "embed-rerank, graph-hydrate", "list", Level::ValueStream),
    ];

    println!("--- service: crawler-index  (Service ⊂ chorus Domain ⊂ borg Product ⊂ reflecting ValueStream) ---\n");
    request("crawler-index", "alert.threshold", &crawler_index);     // OVERRIDE: Service 0.80 wins over Product 0.90
    request("crawler-index", "deploy.timeout", &crawler_index);      // INHERIT: Product's 30, no closer override
    request("crawler-index", "crawl.stale_sec", &crawler_index);     // Domain-level value
    request("crawler-index", "feature.experimental", &crawler_index);// list coercion from the ValueStream rung
    request("crawler-index", "nonexistent.key", &crawler_index);     // NO-MATCH: found:false → code default

    // A DIFFERENT service in the SAME product sees a DIFFERENT effective value for
    // the same key — because it carries its own Service override (or doesn't).
    println!("--- service: graph-hydrate  (same product, NO own alert.threshold override) ---\n");
    let graph_hydrate = vec![
        Binding::new("alert.threshold", "0.90", "int", Level::Product), // only the Product value on its chain
    ];
    request("graph-hydrate", "alert.threshold", &graph_hydrate);     // INHERIT: gets the Product 0.90, not crawler-index's 0.80
}
