// @domain: domains
// @test-type: unit — the spine template compiled directly against fixture logs, and the generated pulse main; no service
//! #4766 (AC1) — the streams route serves a role's tool calls from the spine log, where they
//! already live: only the log's end is read, an unreadable log is unmeasured (never an empty
//! list), and the generated main sends every spine class to the spine store.
//! Covers templates/spine_log.rs (tail), templates/main.rs (Routed), src/emit.rs.
use std::io::Write;
use std::path::PathBuf;

#[path = "../templates/spine_log.rs"]
#[allow(dead_code)] // each test file uses part of the template
mod spine_log;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athena-gen-4766-route-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn line(event: &str, role: &str, extra: &str) -> String {
    format!(r#"{{"timestamp":"2026-11-16T20:00:{extra}","event":"{event}","role":"{role}"}}"#)
}

fn action(ts: &str, role: &str, phase: &str, id: &str, session: &str, outcome: &str) -> String {
    format!(r#"{{"timestamp":"{ts}","event":"agent.action","role":"{role}","phase":"{phase}","call_id":"{id}","tool":"Bash","digest":"run {id}","session_id":"{session}","outcome":"{outcome}"}}"#)
}

/// The committed pulse main sends spine classes to the spine store and refuses their writes.
/// NEGATIVE PROOF: the same check on a main without the routing fails.
#[test]
fn the_generated_pulse_main_reads_spine_classes_from_the_spine_store() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let main = std::fs::read_to_string(root.join("directing/products/pulse/api/src/main.rs")).unwrap();
    let routes = |m: &str| m.contains("spine::Routed { graph, spine: spine::SpineRows") && m.contains("if class.store == \"spine\" {");
    assert!(routes(&main), "pulse main does not route spine classes");
    assert!(!routes(&main.replace("spine::Routed", "store::Sparql")), "the check must fail without the routing");
    let model = std::fs::read_to_string(root.join("directing/products/pulse/api/src/model.rs")).unwrap();
    assert!(model.contains("name: \"StreamEvent\"") && model.contains("store: \"spine\""), "StreamEvent is not spine-stored");
}

/// AC1 NEGATIVE PROOF: the read takes only the log's end. A call written before the last
/// `max` bytes is not read, so a whole-file read would show it and fail here.
#[test]
fn negative_proof_only_the_end_of_the_log_is_read() {
    let f = dir("tail").join("chorus.log");
    let mut w = std::fs::File::create(&f).unwrap();
    writeln!(w, "{}", action("2026-11-16T19:00:00", "wren", "started", "old", "s0", "")).unwrap();
    for _ in 0..2000 { writeln!(w, "{}", line("hook.decision", "wren", "00")).unwrap(); }
    writeln!(w, "{}", action("2026-11-16T20:00:00", "wren", "started", "new", "s1", "")).unwrap();
    drop(w);
    let ids: Vec<String> = spine_log::calls(&spine_log::tail(f.to_str().unwrap(), 64 * 1024).unwrap()).into_iter().map(|c| c.id).collect();
    assert_eq!(ids, ["new"], "only the end is read");
    let all: Vec<String> = spine_log::calls(&spine_log::tail(f.to_str().unwrap(), u64::MAX).unwrap()).into_iter().map(|c| c.id).collect();
    assert_eq!(all, ["new", "old"], "the check must see the old call when the window covers it");
}

/// AC1 NEGATIVE PROOF: an unreadable log is an error (unmeasured), never an empty list.
#[test]
fn negative_proof_a_missing_log_is_unmeasured_not_empty() {
    let err = spine_log::tail("/nonexistent/4766/chorus.log", 1024).unwrap_err();
    assert!(err.contains("unreadable"), "{err}");
}

/// The route itself: runs the generated pulse API's own route tests, which call `route()` on
/// the StreamEvent collection — 200 with the log's calls newest first, and 503 (unmeasured)
/// when the log is unreadable. Both must be reported as passing by name; a filter that
/// matched nothing (0 run) fails here rather than passing vacuously.
#[test]
fn the_pulse_streams_route_answers_200_with_calls_and_503_on_an_unreadable_log() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let manifest = root.join("directing/products/pulse/api/Cargo.toml");
    let out = std::process::Command::new(env!("CARGO"))
        .args(["test", "--manifest-path"]).arg(&manifest)
        .args(["--bin", "pulse-api", "spine_tests::", "--", "--test-threads=1"])
        .output().expect("cargo runs");
    let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
    assert!(out.status.success(), "pulse-api route tests failed:\n{text}");
    let ran = |name: &str| text.lines().any(|l| l.contains(name) && l.trim_end().ends_with("ok"));
    for name in ["the_streams_route_serves_the_logs_calls", "negative_proof_an_unreadable_log_is_unmeasured_not_empty"] {
        assert!(ran(name), "{name} did not run and pass:\n{text}");
    }
}

/// #4925 round 6 (Wren, measured 11-23 04:50: 5.9 MB of other events after the last call).
/// NEGATIVE PROOF: one TAIL_BYTES read of such a log holds no call (the old route); the
/// widening read finds it, and never reads past its cap.
#[test]
fn a_busy_log_still_yields_its_last_call_and_the_read_stops_at_the_cap() {
    let d = dir("busy");
    let p = d.join("chorus.log");
    let mut f = std::fs::File::create(&p).unwrap();
    writeln!(f, "{}", action("2026-11-23T04:00:03.000-0500", "kade", "started", "c1", "s1", "")).unwrap();
    writeln!(f, "{}", action("2026-11-23T04:00:04.000-0500", "kade", "ended", "c1", "s1", "ok")).unwrap();
    let filler = line("context.inject.request", "kade", "00.000-0500");
    let small: u64 = 64 * 1024;
    let mut written = 0u64;
    while written < 5 * small { writeln!(f, "{filler}").unwrap(); written += filler.len() as u64 + 1; }
    drop(f);
    let path = p.to_str().unwrap();
    // the old route: one read of `small` bytes holds only filler
    assert!(spine_log::calls(&spine_log::tail(path, small).unwrap()).is_empty(), "control: the old read finds nothing");
    // the widening read finds the call
    let got = spine_log::tail_calls(path, small, 200, 16 * small).unwrap();
    assert_eq!(got.len(), 1, "{got:?}");
    // NEGATIVE: a cap below the call's distance from the end finds nothing, and does not read the whole log
    assert!(spine_log::tail_calls(path, small, 200, 2 * small).unwrap().is_empty());
    // an unreadable log stays an error (unmeasured), never an empty list
    assert!(spine_log::tail_calls("/nonexistent/chorus.log", small, 200, 2 * small).is_err());
}
