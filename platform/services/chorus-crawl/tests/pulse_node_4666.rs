// @test-type: unit — hermetic: the choice of node binary from given inputs; no process, no plist read.
// @domain: messages
// @card: #4666
// @owner: silas
//! #4666 — the messages sensor runs pulse's reader (buzz-day-counts.js), which
//! loads pulse's better-sqlite3. That module is built for the node pulse runs
//! (nvm v20), so the reader must run on that node, never on whatever `node` the
//! runner's PATH finds first (homebrew v23 at 04:45 on 10-20: NODE_MODULE_VERSION
//! 115 vs 131, messages-sent and messages-kept unmeasured).
use chorus_crawl::readings::pulse_node;

const PULSE_NODE: &str = "/Users/jeffbridwell/.nvm/versions/node/v20.11.1/bin/node";

#[test]
fn the_reader_runs_on_the_node_pulse_runs() {
    assert_eq!(pulse_node(None, Some(PULSE_NODE.into())), PULSE_NODE);
}

#[test]
fn an_explicit_override_wins() {
    assert_eq!(pulse_node(Some("/opt/x/node".into()), Some(PULSE_NODE.into())), "/opt/x/node");
}

// NEGATIVE PROOF: the 10-20 failure — no answer from pulse's plist must not mean
// "whatever node is first on PATH"; it is named as unknown so the reading says so.
#[test]
fn without_pulse_node_there_is_no_silent_fallback_to_path() {
    assert_eq!(pulse_node(None, None), "");
    assert_eq!(pulse_node(Some(String::new()), None), "");
}

// pulse's ProgramArguments[0] could one day be a wrapper script; only a node binary counts.
#[test]
fn a_program_that_is_not_node_is_not_used() {
    assert_eq!(pulse_node(None, Some("/Users/x/platform/scripts/service-run".into())), "");
}
