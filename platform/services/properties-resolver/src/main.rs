//! properties-resolver bin — a thin self-check/smoke over the lib (#3437).
//!
//! The crate's primary artifact is the LIBRARY (linked by owl-api's
//! /properties/effective projection, werk-test, tagging-lift). This binary
//! exists because the platform/services build contract signs one binary per
//! crate; rather than a no-op, it runs the cascade over a documented fixture so
//! a deployed copy is provably runnable. `--selfcheck` exercises the resolver;
//! any other invocation prints usage.

use properties_resolver::{decide_effective_value, PropertyDatum, ScopeKind, ScopeNode};

/// The documented example: every scope sets `alert.threshold`; Service (most
/// specific) must win with value "5".
fn fixture() -> Vec<ScopeNode> {
    let d = |iri: &str, v: &str| PropertyDatum {
        iri: iri.into(),
        key: "alert.threshold".into(),
        value: v.into(),
        value_type: "int".into(),
    };
    vec![
        ScopeNode { kind: ScopeKind::Service, iri: "svc:api".into(), properties: vec![d("p:svc", "5")] },
        ScopeNode { kind: ScopeKind::Domain, iri: "dom:obs".into(), properties: vec![d("p:dom", "10")] },
        ScopeNode { kind: ScopeKind::Product, iri: "prod:borg".into(), properties: vec![d("p:prod", "20")] },
        ScopeNode { kind: ScopeKind::ValueStream, iri: "vs:gather".into(), properties: vec![d("p:vs", "100")] },
    ]
}

fn main() {
    match std::env::args().nth(1).as_deref() {
        Some("--selfcheck") => match decide_effective_value(&fixture(), "alert.threshold") {
            Ok(Some(r)) if r.value == "5" && r.winning_scope_kind == ScopeKind::Service => {
                println!(
                    "PASS — alert.threshold resolved to '{}' ({:?}) via property {}",
                    r.value, r.winning_scope_kind, r.winning_property_iri
                );
            }
            other => {
                eprintln!("FAIL — unexpected resolution: {other:?}");
                std::process::exit(1);
            }
        },
        _ => {
            println!(
                "properties-resolver — cascade precedence core (#3437). \
                 Primary artifact is the linked library; run with --selfcheck to smoke-test resolution."
            );
        }
    }
}
