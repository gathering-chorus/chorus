// @domain: domains
// @test-type: integration — runs athena-make generate-{mcp,openapi,page} against an in-test SPARQL stub; no Fuseki
//! #4995 (AC5) — parity for principles: athena-gen generates the MCP tool binding, the
//! OpenAPI document and the page athena-make generates, and each is compared with
//! athena-make's, field by field. Every difference is NAMED below and reviewed; one that
//! is not named fails, and so does a named one that went away (the list stays true).
//! NEGATIVE PROOF: a missing output fails naming it; a changed field type fails as an
//! unnamed difference.
//! Covers src/surfaces against platform/services/athena-make (mcp_binding, openapi_json, page_html).
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

mod stub_4995;
use stub_4995::{repo, serve, store};

/// The reviewed differences (Kade + Borg Queen, 2026-11-19). Each is why, not just what.
const NAMED: &[&str] = &[
    // athena-make's MCP binding still names v0 routes; its own door serves /principles/principles
    "mcp route list: athena-make GET /principles, athena-gen GET /principles/principles",
    "mcp route get: athena-make GET /principles/:name, athena-gen GET /principles/principles/:name",
    "mcp route add: athena-make POST /principles, athena-gen POST /principles/principles",
    // the add tool writes through a different door
    "mcp delegatesTo add: athena-make DAL (athena-model), athena-gen the generated door",
    // generated rows serve name (#5021: status only where declared, and Principle declares none); athena-make's read adds audit fields
    "openapi read field only in athena-make: contributor",
    "openapi read field only in athena-make: created",
    "openapi read field only in athena-make: creator",
    "openapi read field only in athena-make: iri",
    "openapi read field only in athena-make: modified",
    "openapi read field only in athena-make: type",
    "openapi read field only in athena-gen: name",
    // #5002 made ownedBy minCount 1; athena-make's floor counts sh:severity Violation only
    "openapi required only in athena-gen: ownedBy",
    // routes the generated door does not serve yet (route parity is a follow-on, not AC5)
    "openapi path only in athena-make: /principles/principles/batch",
    "openapi path only in athena-make: /principles/principles/delete-batch",
    "openapi path only in athena-make: /principles/principles/{name}/completeness",
    "openapi path only in athena-make: /principles/principles/{name}/consumes-event",
    "openapi path only in athena-make: /principles/principles/{name}/contains",
    "openapi path only in athena-make: /principles/principles/{name}/has-child",
    "openapi path only in athena-make: /principles/principles/{name}/partof",
    "openapi path only in athena-make: /schema/principle",
    // the page points at the API that serves it
    "page OWL_PORT: athena-make 3360, athena-gen 3393",
    "page OWL_ROUTE: athena-make (none), athena-gen /principles/principles",
];

fn athena_make(kind: &str, endpoint: &str) -> String {
    let bin = std::env::var("ATHENA_MAKE_BIN").unwrap_or_else(|_| "/Users/jeffbridwell/.chorus/bin/athena-make".into());
    assert!(Path::new(&bin).is_file(), "athena-make binary not found at {bin} (set ATHENA_MAKE_BIN)");
    let o = std::process::Command::new(&bin).args([&format!("generate-{kind}"), "--class", "Principle"]).env("CHORUS_FUSEKI", endpoint).output().unwrap();
    assert!(o.status.success(), "athena-make generate-{kind}: {}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap()
}

/// athena-gen's three outputs from a crate folder; a missing one is refused by name.
fn generated(dir: &Path) -> Result<[String; 3], String> {
    let read = |f: &str| std::fs::read_to_string(dir.join("surfaces").join(f)).map_err(|_| format!("athena-gen output missing: surfaces/{f}"));
    Ok([read("Principle.mcp.json")?, read("Principle.openapi.json")?, read("Principle.html")?])
}

fn mcp_diffs(make: &Value, gen: &Value, out: &mut Vec<String>) {
    let tools = |v: &Value| v["tools"].as_array().unwrap().iter().map(|t| (t["verb"].as_str().unwrap().to_string(), t.clone())).collect::<std::collections::BTreeMap<_, _>>();
    let (m, g) = (tools(make), tools(gen));
    for verb in m.keys().chain(g.keys()).collect::<BTreeSet<_>>() {
        match (m.get(verb), g.get(verb)) {
            (Some(a), Some(b)) => {
                for k in ["name", "route", "delegatesTo"] {
                    if a.get(k) != b.get(k) {
                        let s = |v: &Value| v.get(k).and_then(Value::as_str).unwrap_or("(none)").to_string();
                        let label = if k == "name" { format!("mcp tool name {verb}") } else if k == "route" { format!("mcp route {verb}") } else { format!("mcp {k} {verb}") };
                        out.push(format!("{label}: athena-make {}, athena-gen {}", s(a), s(b)));
                    }
                }
            }
            (Some(_), None) => out.push(format!("mcp tool only in athena-make: {verb}")),
            (None, Some(_)) => out.push(format!("mcp tool only in athena-gen: {verb}")),
            _ => {}
        }
    }
    if make["class"] != gen["class"] {
        out.push(format!("mcp class: athena-make {}, athena-gen {}", make["class"], gen["class"]));
    }
}

fn keys(v: &Value) -> BTreeSet<String> {
    v.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default()
}

fn strings(v: &Value) -> BTreeSet<String> {
    v.as_array().map(|a| a.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}

fn openapi_diffs(make: &Value, gen: &Value, out: &mut Vec<String>) {
    let (ms, gs) = (&make["components"]["schemas"], &gen["components"]["schemas"]);
    let (mp, gp) = (&ms["Principle"]["properties"], &gs["Principle"]["properties"]);
    for k in keys(mp).difference(&keys(gp)) { out.push(format!("openapi read field only in athena-make: {k}")); }
    for k in keys(gp).difference(&keys(mp)) { out.push(format!("openapi read field only in athena-gen: {k}")); }
    for k in keys(mp).intersection(&keys(gp)) {
        if mp[k] != gp[k] { out.push(format!("openapi read field {k}: athena-make {}, athena-gen {}", mp[k], gp[k])); }
    }
    let (mr, gr) = (strings(&ms["Principle"]["required"]), strings(&gs["Principle"]["required"]));
    for k in mr.difference(&gr) { out.push(format!("openapi required only in athena-make: {k}")); }
    for k in gr.difference(&mr) { out.push(format!("openapi required only in athena-gen: {k}")); }
    for body in ["PrincipleCreate", "PrincipleReplace"] {
        if ms[body] != gs[body] { out.push(format!("openapi {body}: athena-make {}, athena-gen {}", ms[body], gs[body])); }
    }
    let (mpaths, gpaths) = (keys(&make["paths"]), keys(&gen["paths"]));
    for p in mpaths.difference(&gpaths) { out.push(format!("openapi path only in athena-make: {p}")); }
    for p in gpaths.difference(&mpaths) { out.push(format!("openapi path only in athena-gen: {p}")); }
    for p in mpaths.intersection(&gpaths) {
        let (a, b) = (keys(&make["paths"][p]), keys(&gen["paths"][p]));
        if a != b { out.push(format!("openapi methods {p}: athena-make {a:?}, athena-gen {b:?}")); }
    }
}

/// The page's facts: its title, its blocks, its scripts, and the OWL_* globals.
fn page_facts(html: &str) -> std::collections::BTreeMap<String, String> {
    let mut f = std::collections::BTreeMap::new();
    let between = |s: &str, a: &str, b: &str| s.split_once(a).and_then(|(_, r)| r.split_once(b)).map(|(x, _)| x.to_string());
    f.insert("title".into(), between(html, "<title>", "</title>").unwrap_or_default());
    let ids: Vec<&str> = html.match_indices("id=\"").map(|(i, _)| { let r = &html[i + 4..]; &r[..r.find('"').unwrap_or(0)] }).collect();
    f.insert("ids".into(), ids.join(","));
    let srcs: Vec<&str> = html.match_indices("<script src=\"").map(|(i, _)| { let r = &html[i + 13..]; &r[..r.find('"').unwrap_or(0)] }).collect();
    f.insert("scripts".into(), srcs.join(","));
    for g in ["OWL_PORT", "OWL_CLASS", "OWL_COLLECTION", "OWL_ROUTE"] {
        let v = between(html, &format!("window.{g} = "), ";").map(|v| v.trim_matches('"').to_string()).unwrap_or_else(|| "(none)".into());
        f.insert(g.into(), v);
    }
    f
}

fn page_diffs(make: &str, gen: &str, out: &mut Vec<String>) {
    let (m, g) = (page_facts(make), page_facts(gen));
    for (k, a) in &m {
        if g.get(k) != Some(a) {
            out.push(format!("page {k}: athena-make {a}, athena-gen {}", g.get(k).cloned().unwrap_or_default()));
        }
    }
}

fn differences(make: &[String; 3], gen: &[String; 3]) -> Vec<String> {
    let json = |s: &str| serde_json::from_str::<Value>(s).unwrap_or_else(|e| panic!("not JSON ({e}): {s}"));
    let mut out = Vec::new();
    mcp_diffs(&json(&make[0]), &json(&gen[0]), &mut out);
    openapi_diffs(&json(&make[1]), &json(&gen[1]), &mut out);
    page_diffs(&make[2], &gen[2], &mut out);
    out.sort();
    out
}

fn crate_dir() -> PathBuf {
    repo().join("shaping/domains/principles/api")
}

#[test]
fn principles_surfaces_match_athena_make_but_for_the_named_differences() {
    let ep = serve(store(None));
    let make = [athena_make("mcp", &ep), athena_make("openapi", &ep), athena_make("page", &ep)];
    let gen = generated(&crate_dir()).unwrap_or_else(|e| panic!("{e}"));
    let got = differences(&make, &gen);
    let mut want: Vec<String> = NAMED.iter().map(|s| s.to_string()).collect();
    want.sort();
    let unnamed: Vec<&String> = got.iter().filter(|d| !want.contains(d)).collect();
    let gone: Vec<&String> = want.iter().filter(|d| !got.contains(d)).collect();
    assert!(unnamed.is_empty() && gone.is_empty(), "unnamed differences: {unnamed:#?}\nnamed but gone: {gone:#?}");

    // NEGATIVE PROOF: a field type that changes is a difference no one named
    let mut o: Value = serde_json::from_str(&gen[1]).unwrap();
    o["components"]["schemas"]["Principle"]["properties"]["order"] = serde_json::json!({ "type": "integer" });
    let changed = [gen[0].clone(), o.to_string(), gen[2].clone()];
    assert!(differences(&make, &changed).iter().any(|d| d.starts_with("openapi read field order:") && !want.contains(d)),
        "a changed field type must surface as an unnamed difference");
}

/// NEGATIVE PROOF: each missing output is refused by name.
#[test]
fn a_missing_output_fails_naming_it() {
    for f in ["Principle.mcp.json", "Principle.openapi.json", "Principle.html"] {
        let dir = std::env::temp_dir().join(format!("parity-4995-{}-{f}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("surfaces")).unwrap();
        for g in ["Principle.mcp.json", "Principle.openapi.json", "Principle.html"].iter().filter(|g| **g != f) {
            std::fs::copy(crate_dir().join("surfaces").join(g), dir.join("surfaces").join(g)).unwrap();
        }
        let e = generated(&dir).expect_err("a missing output must fail");
        assert_eq!(e, format!("athena-gen output missing: surfaces/{f}"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// The committed surfaces are what the generator writes today.
#[test]
fn the_committed_surfaces_are_what_the_generator_writes() {
    let (st, _) = athena_gen::model::load(&athena_gen::model::model_files(&repo())).unwrap();
    let p = athena_gen::model::domain(&st, "principles").unwrap_or_else(|e| panic!("{}", e.0));
    for (path, text) in athena_gen::surfaces::files(&p, &athena_gen::model::settings(&st, "principles").unwrap()) {
        let committed = std::fs::read_to_string(crate_dir().join(&path)).unwrap_or_else(|_| panic!("{path} not committed"));
        assert_eq!(committed, text, "{path} differs from a fresh generate");
    }
}
