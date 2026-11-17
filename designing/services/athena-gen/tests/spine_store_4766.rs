// @domain: domains
// @test-type: unit — the spine template compiled directly against fixture logs; no service
//! #4766 — Streams read the spine log where the calls already live (Jeff 2026-11-16:
//! "loki is not system of record"; the spine is). A class marked
//! `chorus:instancesStore "spine"` is read from the log's end, one row per tool call.
//! Covers src/model (Class.store), src/emit (ClassDef store, spine files),
//! templates/spine_log.rs (tail, calls).
use athena_gen::model::{load, product};
use std::io::Write;
use std::path::PathBuf;

#[path = "../templates/spine_log.rs"]
mod spine_log;

const MODEL: &str = r#"
@prefix c: <https://jeffbridwell.com/chorus#> .
@prefix sh: <http://www.w3.org/ns/shacl#> .
@prefix xsd: <http://www.w3.org/2001/XMLSchema#> .
c:pulse a c:Product ; c:hasDomain c:streams ; c:hasProperty c:gp, c:ns .
c:gp c:propertyKey "store.graph_prefix" ; c:propertyValue "urn:chorus:domains:" .
c:ns c:propertyKey "model.namespace" ; c:propertyValue "https://jeffbridwell.com/chorus#" .
c:streams c:definesVocabulary c:StreamEvent, c:Channel .
c:StreamEventShape a sh:NodeShape ; sh:targetClass c:StreamEvent ; c:instancesStore "spine" ;
  sh:property [ sh:path c:streamRole ; sh:datatype xsd:string ] .
c:ChannelShape a sh:NodeShape ; sh:targetClass c:Channel ;
  sh:property [ sh:path c:channelKind ; sh:datatype xsd:string ] .
"#;

fn dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("athena-gen-4766-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn model_file(name: &str, ttl: &str) -> PathBuf {
    let f = dir(name).join("m.ttl");
    std::fs::write(&f, ttl).unwrap();
    f
}

fn line(event: &str, role: &str, extra: &str) -> String {
    format!(r#"{{"timestamp":"2026-11-16T20:00:{extra}","event":"{event}","role":"{role}"{}}}"#, "")
}

fn action(ts: &str, role: &str, phase: &str, id: &str, session: &str, outcome: &str) -> String {
    format!(r#"{{"timestamp":"{ts}","event":"agent.action","role":"{role}","phase":"{phase}","call_id":"{id}","tool":"Bash","digest":"run {id}","session_id":"{session}","outcome":"{outcome}"}}"#)
}

#[test]
fn a_class_marked_spine_reads_the_log_and_the_rest_stay_in_the_graph() {
    let (store, _) = load(&[model_file("ok", MODEL)]).unwrap();
    let p = product(&store, "pulse").unwrap();
    let mut got: Vec<(String, String)> = p.domains.iter().flat_map(|d| d.classes.iter().map(|c| (c.name.clone(), c.store.clone()))).collect();
    got.sort();
    assert_eq!(got, [("Channel".into(), "graph".into()), ("StreamEvent".into(), "spine".into())]);
    let rs = athena_gen::emit::model_rs(&p, "1.0.0", &[]);
    assert!(rs.contains("store: \"spine\"") && rs.contains("store: \"graph\""), "each class is emitted with its store");
}

/// NEGATIVE PROOF: a store the generator doesn't know is refused by name.
#[test]
fn negative_proof_an_unknown_store_is_refused() {
    let (store, _) = load(&[model_file("bad", &MODEL.replace("\"spine\"", "\"loki\""))]).unwrap();
    let err = product(&store, "pulse").unwrap_err();
    assert!(err.0.contains("instancesStore \"loki\" is not one of graph, spine"), "{err:?}");
}

#[test]
fn each_call_is_one_row_joined_on_its_id_newest_first() {
    let log = [
        action("2026-11-16T20:00:01", "wren", "started", "a", "s1", ""),
        action("2026-11-16T20:00:02", "wren", "ended", "a", "s1", "ok"),
        action("2026-11-16T20:00:03", "kade", "started", "b", "s2", ""),
        action("2026-11-16T20:00:04", "kade", "ended", "b", "s2", "error"),
        action("2026-11-16T20:00:05", "silas", "started", "c", "s3", ""),
        action("2026-11-16T20:00:06", "wren", "started", "d", "s1", ""),
        r#"{"timestamp":"2026-11-16T20:00:07","event":"agent.turn.ended","role":"wren","session_id":"s1"}"#.to_string(),
    ].join("\n");
    let got: Vec<(String, String)> = spine_log::calls(&log).into_iter().map(|c| (c.id, c.phase)).collect();
    assert_eq!(got, [("d".into(), "stopped".into()), ("c".into(), "running".into()), ("b".into(), "failed".into()), ("a".into(), "done".into())]);
}

/// AC4 NEGATIVE PROOF: a reply-only turn (turn line, nudge, reply) makes no rows, so it
/// never reads as work. If anything but agent.action made a row, this turn would show.
#[test]
fn negative_proof_a_reply_only_turn_is_not_work() {
    let log = [
        line("session_turn", "wren", "01"),
        line("nudge.emitted", "wren", "02"),
        line("reply.published", "wren", "03"),
        r#"{"timestamp":"2026-11-16T20:00:04","event":"agent.turn.ended","role":"wren","session_id":"s1"}"#.to_string(),
    ].join("\n");
    assert!(spine_log::calls(&log).is_empty(), "a reply-only turn made rows");
    let with_call = format!("{log}\n{}", action("2026-11-16T20:00:05", "wren", "started", "x", "s1", ""));
    assert_eq!(spine_log::calls(&with_call).len(), 1, "the check must still see a real call");
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
