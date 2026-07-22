//! #3660 — the generated /tree read primitive. A shape declares its recursive
//! edge(s) (chorus:treeEdge) + sibling rank (chorus:treeOrder); the generator
//! emits `GET /<kind>/<id>/tree?depth=N` returning the nested, ordered JSON
//! tree in one call. Red-first: route emission, ordered recursion, depth cap,
//! cycle guard (node-REUSE across branches is legal — Borg serves two steps).

use owl_api::{build_tree, openapi_json, tree_routes, RouteTable};

fn edges(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(a, b)| (a.to_string(), b.to_string())).collect()
}

fn ranks(pairs: &[(&str, i64)]) -> Vec<(String, i64)> {
    pairs.iter().map(|(a, r)| (a.to_string(), *r)).collect()
}

fn fixture_with_tree() -> RouteTable {
    RouteTable {
        class: "https://jeffbridwell.com/chorus#ValueStream".into(),
        fields: vec!["label|datatype:string".into(), "contains|edge:ValueStreamStep".into()],
        routes: {
            let mut r = vec![
                "GET /valuestreams".into(),
                "GET /valuestreams/:name".into(),
                "GET /schema/valuestream".into(),
            ];
            r.extend(tree_routes("valuestreams", &["contains".into(), "hasValueStream".into()]));
            r
        },
        secured: vec![],
        mandatory: vec![],
        repo_target: "generated/valuestream".into(),
        exposure: vec![],
        instances_graph: "urn:chorus:instances".into(),
        tree_edges: vec!["contains".into(), "hasValueStream".into()],
        tree_order: Some("stageOrder".into()),
    }
}

// AC1 — a shape that declares recursive-descent edges gets a /tree read emitted;
// a shape with no declared tree edge emits NOTHING (no phantom route).
#[test]
fn tree_route_emitted_only_when_edges_declared() {
    let with = tree_routes("valuestreams", &["contains".into()]);
    assert_eq!(with, vec!["GET /valuestreams/:name/tree".to_string()]);
    let without = tree_routes("valuestreams", &[]);
    assert!(without.is_empty(), "no declared tree edge → no /tree route");
}

// AC2 — full recursion in one call: root → children → grandchildren, nested JSON.
#[test]
fn tree_returns_nested_recursion() {
    let e = edges(&[
        ("chorus", "building"),
        ("building", "pull"),
        ("building", "commit"),
        ("pull", "worktree-add"),
    ]);
    let t = build_tree("chorus", &e, &[], 32).expect("tree builds");
    // nested: worktree-add sits INSIDE pull's children, which sits inside building's
    let pull_idx = t.find("\"pull\"").expect("pull present");
    let wt_idx = t.find("\"worktree-add\"").expect("grandchild present");
    assert!(wt_idx > pull_idx, "grandchild nests under child");
    assert!(t.contains("\"children\""), "nested children key");
}

// AC3 — children ordered by the declared rank at every level; nodes WITHOUT
// rank fall to the bottom, visible (never dropped).
#[test]
fn children_ordered_by_rank_unranked_visible_at_bottom() {
    let e = edges(&[
        ("root", "gamma"),
        ("root", "alpha"),
        ("root", "beta"),
        ("root", "unranked"),
    ]);
    let r = ranks(&[("alpha", 1), ("beta", 2), ("gamma", 3)]);
    let t = build_tree("root", &e, &r, 32).expect("tree builds");
    let pos = |n: &str| t.find(&format!("\"{}\"", n)).unwrap_or_else(|| panic!("{} missing", n));
    assert!(pos("alpha") < pos("beta"), "rank 1 before rank 2");
    assert!(pos("beta") < pos("gamma"), "rank 2 before rank 3");
    assert!(pos("gamma") < pos("unranked"), "unranked falls to the bottom");
}

// AC4 — depth bounds descent: depth=1 returns root + children, no grandchildren.
#[test]
fn depth_param_bounds_descent() {
    let e = edges(&[("a", "b"), ("b", "c"), ("c", "d")]);
    let t = build_tree("a", &e, &[], 1).expect("tree builds");
    assert!(t.contains("\"b\""), "depth 1 keeps direct children");
    assert!(!t.contains("\"c\""), "depth 1 cuts grandchildren");
    let t0 = build_tree("a", &e, &[], 0).expect("tree builds");
    assert!(!t0.contains("\"b\""), "depth 0 is root only");
}

// AC4 — a REAL cycle (a node that is its own ancestor) is refused with a NAMED error.
#[test]
fn cycle_refused_with_named_error() {
    let e = edges(&[("a", "b"), ("b", "c"), ("c", "a")]);
    let err = build_tree("a", &e, &[], 32).expect_err("cycle must refuse");
    assert!(err.contains("cycle"), "error names the cycle: {}", err);
}

// AC4 — node REUSE is NOT a cycle: a diamond (two branches converge on one
// node) legally repeats the node in both branches. Borg serves two steps.
#[test]
fn node_reuse_across_branches_is_not_a_cycle() {
    let e = edges(&[("root", "left"), ("root", "right"), ("left", "borg"), ("right", "borg")]);
    let t = build_tree("root", &e, &[], 32).expect("diamond is legal");
    assert_eq!(t.matches("\"borg\"").count(), 2, "reused node appears under BOTH parents");
}

// AC5 — the generated OpenAPI documents the tree read.
#[test]
fn openapi_documents_the_tree_read() {
    let spec = openapi_json(&fixture_with_tree());
    assert!(spec.contains("/valuestreams/{name}/tree"), "tree path documented");
    assert!(spec.contains("depth"), "depth parameter documented");
}
