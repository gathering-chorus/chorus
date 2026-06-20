//! #3506 / ADR-047 AC2+AC5 — LIVE serve-loop proof. Spawns the real binary on a
//! scratch port against running Fuseki, smokes the serve-loop contract (discovery
//! root, enveloped collections across primitives, ETag→304, served-OpenAPI-
//! everywhere), then kills the child (Rust child.kill(), not a shell kill).
//! `#[ignore]`d: needs Fuseki on localhost:3030/pods. Run:
//!   cargo test --test live_serve -- --ignored --nocapture

use std::process::{Child, Command, Stdio};
use std::time::Duration;

/// curl -s -i → (headers, body)
fn curl(url: &str, extra: &[&str]) -> (String, String) {
    let out = Command::new("curl")
        .arg("-s")
        .arg("-i")
        .args(extra)
        .arg(url)
        .output()
        .expect("curl runs");
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    match s.split_once("\r\n\r\n") {
        Some((h, b)) => (h.to_string(), b.to_string()),
        None => (s, String::new()),
    }
}

struct Killer(Child);
impl Drop for Killer {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[test]
#[ignore]
fn live_serve_loop_contract() {
    let port = 3897u16;
    let base = format!("http://127.0.0.1:{}", port);
    let bin = env!("CARGO_BIN_EXE_owl-api");
    // OWL_API_MODEL_COMMIT set so the ETag activates (else it's suppressed as "unknown").
    let child = Command::new(bin)
        .args(["serve", "--port", &port.to_string()])
        .env("OWL_API_MODEL_COMMIT", "testcommit9")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn owl-api serve");
    let _killer = Killer(child); // kills on scope exit, even on panic

    // wait for ready
    let mut ready = false;
    for _ in 0..60 {
        std::thread::sleep(Duration::from_millis(100));
        let (_, b) = curl(&format!("{}/health", base), &[]);
        if b.contains("\"ok\": true") {
            ready = true;
            break;
        }
    }
    assert!(ready, "owl-api did not come up on :{}", port);

    // 1) discovery root
    let (_, disc) = curl(&format!("{}/", base), &[]);
    assert!(disc.contains("\"kind\": \"Discovery\""), "discovery root kind: {}", disc);
    assert!(disc.contains("\"apiVersion\": \"v1\""), "discovery apiVersion: {}", disc);
    assert!(disc.contains("\"collection\""), "discovery lists collections: {}", disc);

    // 2) the SAME envelope across every served primitive (≥3 = uniformity proof)
    let mut enveloped_primitives = 0;
    for plural in ["domains", "products", "value-streams", "services"] {
        let (h, body) = curl(&format!("{}/{}", base, plural), &[]);
        if h.contains("404") {
            continue; // primitive not served (no shape) — skip, not a failure
        }
        assert!(body.contains("\"apiVersion\": \"v1\""), "/{} enveloped: {}", plural, body);
        assert!(body.contains("\"count\":"), "/{} carries count: {}", plural, body);
        assert!(body.contains("\"data\":"), "/{} has data slot: {}", plural, body);
        enveloped_primitives += 1;
    }
    assert!(enveloped_primitives >= 3, "uniform envelope proven on ≥3 primitives; got {}", enveloped_primitives);

    // 3) entity read on the cards domain — real data, enveloped
    let (eh, eb) = curl(&format!("{}/domains/cards", base), &[]);
    assert!(eb.contains("\"kind\": \"Domain\""), "cards entity kind: {}", eb);
    assert!(eb.contains("\"creator\""), "cards entity real data: {}", eb);

    // 4) ETag → 304 conditional GET (commit env makes the tag live)
    let etag = eh
        .lines()
        .find(|l| l.to_ascii_lowercase().starts_with("etag:"))
        .and_then(|l| l.split('"').nth(1))
        .map(|s| s.to_string());
    let etag = etag.expect("GET 200 read carries an ETag when commit is set");
    let (h304, _) = curl(
        &format!("{}/domains/cards", base),
        &["-H", &format!("If-None-Match: \"{}\"", etag)],
    );
    assert!(h304.contains("304 Not Modified"), "conditional GET → 304: {}", h304);

    // 5) served OpenAPI for a non-properties surface (everywhere, not just /borg/properties)
    let (oh, ob) = curl(&format!("{}/domains/openapi.json", base), &[]);
    assert!(!oh.contains("404"), "/domains/openapi.json is served: {}", oh);
    assert!(ob.contains("openapi") || ob.contains("paths"), "openapi doc body: {}", ob);

    // 6) pagination: limit slices, links.next appears when more remain
    let (_, page) = curl(&format!("{}/domains?limit=5", base), &[]);
    assert!(page.contains("\"count\":"), "paginated list still enveloped: {}", page);
    // (cards domain set has 35 domains → a limit of 5 must yield a next cursor)
    assert!(page.contains("\"next\""), "links.next present when more remain: {}", page);
}
