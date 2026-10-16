// @test-type: integration — needs-stack: spawns athena-make against the live Fuseki
// @domain: domains — placed by chorus-crawl's domain rules (#4538)
//! #3506 / ADR-047 AC2+AC5 — LIVE serve-loop proof. Spawns the real binary on a
//! scratch port against running Fuseki, smokes the serve-loop contract (discovery
//! root, enveloped collections across primitives, ETag→304, served-OpenAPI-
//! everywhere), then kills the child (Rust child.kill(), not a shell kill).
//! Needs Fuseki on localhost:3030/pods. #4292 — no longer `#[ignore]`d: the
//! nightly runs it with the stack up and skips it, typed, when the stack is
//! down (#3919 needs-stack binaries).

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

/// The HTTP status from a response's header block (its first line), 0 if none.
fn status_of(headers: &str) -> u16 {
    headers.lines().next().and_then(|l| l.split_whitespace().nth(1)).and_then(|c| c.parse().ok()).unwrap_or(0)
}

/// NEGATIVE PROOF (#3734) for the check above: a 200 whose ETag holds "404"
/// is served, and only a 404 status line is not.
#[test]
fn a_404_inside_a_header_value_is_not_a_404() {
    assert_eq!(status_of("HTTP/1.1 200 OK\r\nETag: \"bf0404450e21629c\""), 200);
    assert_eq!(status_of("HTTP/1.1 404 Not Found\r\n"), 404);
}

#[test]
fn live_serve_loop_contract() {
    // #4553 — a port the OS hands out, not a fixed one. Two runs (#4553 run 2,
    // #4530 run 20) both started a server on :3897 at 20:18 and both read "did
    // not come up"; and a fixed port could answer ready from ANOTHER run's server.
    let port = std::net::TcpListener::bind("127.0.0.1:0").and_then(|l| l.local_addr()).expect("a free port").port();
    let base = format!("http://127.0.0.1:{}", port);
    let bin = env!("CARGO_BIN_EXE_athena-make");
    // OWL_API_MODEL_COMMIT set so the ETag activates (else it's suppressed as "unknown").
    let child = Command::new(bin)
        .args(["serve", "--port", &port.to_string()])
        .env("OWL_API_MODEL_COMMIT", "testcommit9")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn athena-make serve");
    let mut killer = Killer(child); // kills on scope exit, even on panic

    // wait for ready
    let mut ready = false;
    // #4292 — startup now generates ~56 APIs from the live model; measured
    // ready after 6-10s on 2026-09-25, so the old 6s budget failed every time.
    // #4553 — measured 61s on 2026-10-15 20:45 (load 25-29 on 8 cores), past
    // the 30s budget. The wait now tells a dead server from a slow one: a
    // server that exits fails at once with its exit status; one still alive
    // gets up to 120s and then says it was alive but not ready.
    let start = std::time::Instant::now();
    while start.elapsed() < Duration::from_secs(120) {
        std::thread::sleep(Duration::from_millis(200));
        if let Ok(Some(status)) = killer.0.try_wait() {
            panic!("athena-make serve exited before it was ready: {}", status);
        }
        let (_, b) = curl(&format!("{}/health", base), &[]);
        if b.contains("\"ok\": true") {
            ready = true;
            break;
        }
    }
    assert!(ready, "athena-make alive but not ready on :{} after {:?}", port, start.elapsed());

    // 1) discovery root
    let (_, disc) = curl(&format!("{}/", base), &[]);
    assert!(disc.contains("\"kind\": \"Discovery\""), "discovery root kind: {}", disc);
    assert!(disc.contains("\"apiVersion\": \"v1\""), "discovery apiVersion: {}", disc);
    assert!(disc.contains("\"collection\""), "discovery lists collections: {}", disc);

    // 2) the SAME envelope across every served primitive (≥3 = uniformity proof)
    let mut enveloped_primitives = 0;
    for plural in ["domains", "products", "value-streams", "services"] {
        let (h, body) = curl(&format!("{}/{}?count=true", base, plural), &[]); // #4531: the door counts only on request
        // #4514 — the status line only: `h.contains("404")` also matched the
        // ETag "bf0404450e21629c" on a 200 (measured 2026-10-13 19:23), so a
        // served primitive was skipped whenever its data hashed to a "404"
        if status_of(&h) == 404 {
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
    assert!(eb.contains("\"purpose\""), "cards entity real data: {}", eb);

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
    assert_ne!(status_of(&oh), 404, "/domains/openapi.json is served: {}", oh);
    assert!(ob.contains("openapi") || ob.contains("paths"), "openapi doc body: {}", ob);

    // 6) pagination: limit slices, links.next appears when more remain
    let (_, page) = curl(&format!("{}/domains?limit=5&count=true", base), &[]); // #4531: the door counts only on request
    assert!(page.contains("\"count\":"), "paginated list still enveloped: {}", page);
    // (cards domain set has 35 domains → a limit of 5 must yield a next cursor)
    assert!(page.contains("\"next\""), "links.next present when more remain: {}", page);
}
