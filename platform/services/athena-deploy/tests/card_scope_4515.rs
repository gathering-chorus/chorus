// @test-type: unit
// @domain: athena
//! #4515 — `athena-deploy --card <n>` (Land: prod) and `athena-deploy staging
//! --card <n>` (Deploy: staging). One card's model change, two targets, then the
//! served check: every class the card declares must be in a route of
//! athena-make's discovery document, and that route must answer.
//!
//! Nothing here reaches a store or a real athena-make. The served check runs
//! against a fake athena-make on a loopback port this test opens; the binary
//! tests stop at a refusal or at the #4080 target check (ATHENA_DEPLOY_TARGET_CHECK_ONLY),
//! both before any write.

use athena_deploy::card::{discovery_routes, plan, served_check, served_check_live};
use athena_deploy::DomainSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;

const DISCO: &str = r#"{ "apiVersion": "v1", "service": "athena-make", "kind": "Discovery", "count": 2, "primitives": [{ "kind": "Widget", "collection": "/v1/things/widgets", "openapi": "/v1/things/widgets/openapi.json", "deprecatedCollection": "/v1/widgets" }, { "kind": "Gadget", "collection": "/v1/things/gadgets", "openapi": "/v1/things/gadgets/openapi.json" }] }"#;

fn s(v: &[&str]) -> Vec<String> {
    v.iter().map(|x| x.to_string()).collect()
}

// ---- the plan: which files load where ----

fn security_set() -> DomainSet {
    DomainSet {
        name: "security".into(),
        graph: "urn:chorus:domains:security".into(),
        files: s(&["roles/silas/ontology/identity-principals-3613.ttl", "roles/silas/ontology/permissions-4183.ttl"]),
        replace: false,
    }
}

#[test]
fn a_card_file_outside_every_set_merges_into_the_ontology_graph() {
    let p = plan(&s(&["roles/wren/ontology/widgets.ttl"]), "/w/wren-1", &[security_set()]);
    assert_eq!(p.ontology, s(&["/w/wren-1/roles/wren/ontology/widgets.ttl"]));
    assert!(p.sets.is_empty());
}

#[test]
fn negative_proof_a_card_file_in_a_domain_set_never_loads_into_the_ontology_graph() {
    let p = plan(
        &s(&["roles/silas/ontology/permissions-4183.ttl", "roles/silas/ontology/identity-principals-3613.ttl", "roles/wren/ontology/w.ttl"]),
        "/w/wren-1",
        &[security_set()],
    );
    assert!(
        !p.ontology.iter().any(|f| f.contains("permissions-4183") || f.contains("identity-principals")),
        "a security file would merge into urn:chorus:ontology: {:?}", p.ontology
    );
    assert_eq!(p.sets, vec![security_set()], "its whole set deploys, once");
    assert_eq!(p.ontology, s(&["/w/wren-1/roles/wren/ontology/w.ttl"]));
}

// ---- the served check ----

#[test]
fn discovery_routes_reads_kind_and_collection_and_skips_the_document_itself() {
    assert_eq!(
        discovery_routes(DISCO),
        vec![("Widget".to_string(), "/v1/things/widgets".to_string()), ("Gadget".to_string(), "/v1/things/gadgets".to_string())]
    );
}

#[test]
fn every_declared_class_served_and_answering_passes() {
    let r = served_check(DISCO, &s(&["Widget", "Gadget"]), &|_| true).unwrap();
    assert_eq!(r, s(&["served: Widget at /v1/things/widgets answers", "served: Gadget at /v1/things/gadgets answers"]));
}

#[test]
fn negative_proof_a_class_not_in_discovery_is_deployed_but_not_served() {
    let e = served_check(DISCO, &s(&["Widget", "Sprocket"]), &|_| true).unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("Sprocket DEPLOYED BUT NOT SERVED"), "{e:?}");
}

#[test]
fn negative_proof_a_route_listed_but_not_answering_fails_and_names_it() {
    let e = served_check(DISCO, &s(&["Widget", "Gadget"]), &|r| r != "/v1/things/gadgets").unwrap_err();
    assert_eq!(e.len(), 1, "{e:?}");
    assert!(e[0].contains("Gadget is declared at /v1/things/gadgets but that route does not answer"), "{e:?}");
    // and the bare path answering is named as the advertise-one-serve-another split
    assert!(e[0].contains("/gadgets DOES answer"), "{e:?}");
}

#[test]
fn negative_proof_a_discovery_with_no_routes_is_unmeasured_not_a_pass() {
    for d in ["", "curl: (7) Failed to connect", r#"{"kind":"Discovery","primitives":[]}"#] {
        let e = served_check(d, &s(&["Widget"]), &|_| true).unwrap_err();
        assert!(e[0].contains("UNMEASURED"), "{d}: {e:?}");
    }
}

#[test]
fn a_card_that_declares_no_class_says_so_out_loud() {
    let r = served_check("", &[], &|_| true).unwrap();
    assert!(r[0].contains("declare no class"), "{r:?}");
}

/// A fake athena-make on loopback: `/` is the discovery document, the widgets
/// route answers 200, everything else 404.
fn fake_athena_make() -> String {
    let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = l.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in l.incoming().flatten() {
            let mut st = stream;
            let mut buf = [0u8; 2048];
            let n = st.read(&mut buf).unwrap_or(0);
            let req = String::from_utf8_lossy(&buf[..n]);
            let path = req.split_whitespace().nth(1).unwrap_or("/").to_string();
            let (code, body) = match path.as_str() {
                "/" => ("200 OK", DISCO.to_string()),
                "/v1/things/widgets" => ("200 OK", "[]".to_string()),
                _ => ("404 Not Found", "{}".to_string()),
            };
            let _ = write!(st, "HTTP/1.1 {code}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len());
        }
    });
    format!("http://{addr}")
}

#[test]
fn negative_proof_the_live_check_fails_a_route_that_404s_over_http() {
    let api = fake_athena_make();
    assert!(served_check_live(&api, &s(&["Widget"])).is_ok(), "the answering route passes");
    let e = served_check_live(&api, &s(&["Widget", "Gadget", "Sprocket"])).unwrap_err();
    assert_eq!(e.len(), 2, "{e:?}");
    assert!(e.iter().any(|l| l.contains("Gadget is declared at /v1/things/gadgets but that route does not answer")), "{e:?}");
    assert!(e.iter().any(|l| l.contains("Sprocket DEPLOYED BUT NOT SERVED")), "{e:?}");
}

#[test]
fn negative_proof_an_athena_make_that_is_down_is_unmeasured() {
    let e = served_check_live("http://127.0.0.1:9", &s(&["Widget"])).unwrap_err();
    assert!(e[0].contains("UNMEASURED"), "{e:?}");
}

// ---- the binary: refusals and the prod guard, all before any write ----

fn bin() -> PathBuf {
    // at RUN time, next to this test binary
    let exe = std::env::current_exe().unwrap();
    let b = exe.parent().unwrap().parent().unwrap().join("athena-deploy");
    assert!(b.is_file(), "athena-deploy not built at {}", b.display());
    b
}

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "core.hooksPath=/dev/null", "-c", "commit.gpgsign=false"])
        .arg("-C").arg(dir).args(args).output().unwrap();
    assert!(o.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&o.stderr));
}

fn write(dir: &Path, rel: &str, body: &str) {
    let p = dir.join(rel);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, body).unwrap();
}

/// A werk base named `chorus-werk` (so the #4080 werk guard sees a werk) holding
/// `wren-<card>`, whose card commit writes `changes` on top of main.
fn werk(name: &str, card: &str, changes: &[(&str, &str)]) -> PathBuf {
    let base = std::env::temp_dir().join(format!("athena-deploy-4515-{}-{name}", std::process::id())).join("chorus-werk");
    let _ = std::fs::remove_dir_all(&base);
    let w = base.join(format!("wren-{card}"));
    std::fs::create_dir_all(&w).unwrap();
    git(&w, &["init", "-q", "-b", "main"]);
    write(&w, "platform/config/domain-set-manifest.txt", "security|urn:chorus:domains:security|roles/silas/ontology/permissions-4183.ttl\n");
    write(&w, "README", "x\n");
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "main"]);
    git(&w, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
    for (rel, body) in changes {
        write(&w, rel, body);
    }
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "--allow-empty", "-m", "card"]);
    base
}

/// Run the binary with stores and athena-make pointed at closed loopback ports,
/// so a run that got past its refusal could not reach anything real.
fn run(base: &Path, args: &[&str], extra: &[(&str, &str)]) -> (i32, String) {
    let mut c = Command::new(bin());
    c.args(args).env("CHORUS_WERK_BASE", base).env("CHORUS_ROLE", "wren").env_remove("ROLE")
        .env("CHORUS_FUSEKI_BASE", "http://127.0.0.1:9").env("ATHENA_MAKE_URL", "http://127.0.0.1:9")
        .env("ATHENA_MAKE_STAGING_URL", "http://127.0.0.1:9").env("ATHENA_SERVE_BIN", "/usr/bin/false")
        .env("CHORUS_LOG", "/usr/bin/true")
        .env_remove("FUSEKI_GSP").env_remove("FUSEKI_QUERY").env_remove("FUSEKI_UPDATE")
        .env_remove("DEPLOY_TARGET").env_remove("DOMAIN_SET_MANIFEST").env_remove("TTL");
    for (k, v) in extra {
        c.env(k, v);
    }
    let o = c.output().unwrap();
    (o.status.code().unwrap_or(-1), format!("{}{}", String::from_utf8_lossy(&o.stdout), String::from_utf8_lossy(&o.stderr)))
}

const WIDGET: &str = "@prefix chorus: <https://jeffbridwell.com/chorus#> .\n@prefix owl: <http://www.w3.org/2002/07/owl#> .\nchorus:Widget a owl:Class .\n";

#[test]
fn negative_proof_a_card_with_no_model_change_is_refused_on_both_targets() {
    let base = werk("nomodel", "9201", &[("platform/scripts/x.sh", "echo\n")]);
    for args in [&["--card", "9201"][..], &["staging", "--card", "9201"][..]] {
        let (code, out) = run(&base, args, &[]);
        assert_eq!(code, 2, "{args:?}: {out}");
        assert!(out.contains("REFUSED — card #9201: no model change"), "{args:?}: {out}");
        assert!(!out.contains("staging copy"), "refused before staging was touched: {out}");
    }
}

#[test]
fn negative_proof_a_card_with_no_werk_is_refused_on_both_targets() {
    let base = werk("nowerk", "9202", &[]);
    for args in [&["--card", "9299"][..], &["staging", "--card", "9299"][..]] {
        let (code, out) = run(&base, args, &[]);
        assert_eq!(code, 2, "{args:?}: {out}");
        assert!(out.contains("REFUSED — card #9299: no werk at"), "{args:?}: {out}");
    }
}

#[test]
fn negative_proof_card_without_a_number_is_refused() {
    let base = werk("noarg", "9203", &[]);
    let (code, out) = run(&base, &["--card"], &[]);
    assert_eq!(code, 2, "{out}");
    assert!(out.contains("--card needs a card number"), "{out}");
}

#[test]
fn negative_proof_the_prod_land_from_a_werk_still_needs_canonical_on_purpose() {
    // #4080 stays: a card's Land writes prod only when the caller says so. The
    // target check stops the run right after the guard either way — no write.
    let base = werk("guard", "9204", &[("roles/wren/ontology/widgets.ttl", WIDGET)]);
    let (code, out) = run(&base, &["--card", "9204"], &[("ATHENA_DEPLOY_TARGET_CHECK_ONLY", "1")]);
    assert_eq!(code, 78, "a bare card land from a werk must refuse: {out}");
    assert!(out.contains("REFUSED — running inside a werk"), "{out}");
    let (code, out) = run(&base, &["--card", "9204"], &[("ATHENA_DEPLOY_TARGET_CHECK_ONLY", "1"), ("DEPLOY_TARGET", "canonical")]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("target-check: ok"), "{out}");
}
