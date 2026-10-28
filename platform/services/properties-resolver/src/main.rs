//! properties-resolver bin — a thin self-check/smoke over the lib (#3437).
//!
//! The crate's primary artifact is the LIBRARY (linked by athena-make's
//! /properties/effective projection, werk-test, tagging-lift). This binary
//! exists because the platform/services build contract signs one binary per
//! crate; rather than a no-op, it runs the cascade over a documented fixture so
//! a deployed copy is provably runnable. `--selfcheck` exercises the resolver;
//! any other invocation prints usage.

use properties_resolver::{
    decide_effective_value, extract_agent_roles_json, extract_effective_value_json, http_get_door,
    target_node_for_scope, PropertyDatum, ScopeKind, ScopeNode,
};

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

fn door_url() -> String {
    std::env::var("ATHENA_MAKE_URL")
        .or_else(|_| std::env::var("CHORUS_DOOR"))
        .unwrap_or_else(|_| "http://127.0.0.1:3360".to_string())
}

fn run_resolve(args: &[String]) -> Result<String, String> {
    let mut scope: Option<String> = None;
    let mut name: Option<String> = None;
    let mut key: Option<String> = None;
    let mut pos = Vec::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--scope" => {
                i += 1;
                scope = args.get(i).cloned();
            }
            "--name" | "--node" => {
                i += 1;
                name = args.get(i).cloned();
            }
            "--key" => {
                i += 1;
                key = args.get(i).cloned();
            }
            other if !other.starts_with('-') => pos.push(other.to_string()),
            other => return Err(format!("unknown flag: {other}")),
        }
        i += 1;
    }

    let (target_node, target_key) = match (scope, name, key) {
        (Some(s), Some(n), Some(k)) => (target_node_for_scope(&s, &n), k),
        (None, Some(n), Some(k)) => (n, k),
        _ if pos.len() == 2 => (pos[0].clone(), pos[1].clone()),
        _ if pos.len() == 3 => (target_node_for_scope(&pos[0], &pos[1]), pos[2].clone()),
        _ => {
            return Err(
                "usage: properties-resolver resolve <node> <key> | --scope <scope> --name <name> --key <key>"
                    .to_string(),
            )
        }
    };

    let path = format!("/effective/{target_node}/{target_key}");
    let (code, body) = http_get_door(&door_url(), &path)?;
    if code != 200 {
        return Err(format!("HTTP {code} for {path}: {body}"));
    }
    extract_effective_value_json(&body)
        .ok_or_else(|| format!("malformed effective payload for {path}: {body}"))
}

fn run_roles() -> Result<Vec<String>, String> {
    let path = "/v1/roles/roles?limit=500";
    let (code, body) = http_get_door(&door_url(), path)?;
    if code != 200 {
        return Err(format!("HTTP {code} for {path}: {body}"));
    }
    extract_agent_roles_json(&body)
        .ok_or_else(|| format!("no agent roles returned from {path}"))
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
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
        Some("resolve") => match run_resolve(&args[2..]) {
            Ok(val) => println!("{val}"),
            Err(e) => {
                eprintln!("properties-resolver resolve: {e}");
                std::process::exit(1);
            }
        },
        Some("roles") => match run_roles() {
            Ok(roles) => {
                for r in roles {
                    println!("{r}");
                }
            }
            Err(e) => {
                eprintln!("properties-resolver roles: {e}");
                std::process::exit(1);
            }
        },
        _ => {
            println!(
                "properties-resolver — cascade precedence core (#3437 / #4774). \
                 Subcommands: --selfcheck | resolve <node> <key> | roles"
            );
        }
    }
}
