// @domain: builds — the werk-commit verb's first-step refusal (#4553)
//! #4553 (Kade) — a card run refuses at its commit step while another run of the
//! card is live, naming that run. #4536 run 14 started beside a live run and died
//! 3 minutes in on the build lock. Runs the real /cws reader against a file://
//! dagu fixture; nothing live is read.

use std::fs;
use std::path::PathBuf;
use werk_commit::live_run_refusal;

// the chorus tree this crate sits in, read at run time (the #4030 guard)
fn chorus_root() -> PathBuf {
    PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap()).join("../../..")
}

fn dagu_fixture(runs: &str) -> tempfile_dir::Dir {
    let d = tempfile_dir::Dir::new();
    fs::create_dir_all(d.0.join("api/v1")).unwrap();
    fs::write(d.0.join("api/v1/dag-runs"), format!("{{\"dagRuns\":[{}]}}", runs)).unwrap();
    d
}

mod tempfile_dir {
    pub struct Dir(pub std::path::PathBuf);
    impl Dir {
        pub fn new() -> Dir {
            let p = std::env::temp_dir().join(format!("wc-4553-{}-{}", std::process::id(),
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            std::fs::create_dir_all(&p).unwrap();
            Dir(p)
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

const BUSY: &str = r#"{"name":"cicd","dagRunId":"busy","params":"CARD=4536 ROLE=kade","statusLabel":"running","startedAt":"2026-10-15T15:19:04-04:00","nodes":[]}"#;
const ME: &str = r#"{"name":"cicd","dagRunId":"me","params":"CARD=4536 ROLE=kade","statusLabel":"running","startedAt":"2026-10-15T15:32:38-04:00","nodes":[]}"#;
const DONE: &str = r#"{"name":"cicd","dagRunId":"busy","params":"CARD=4536 ROLE=kade","statusLabel":"failed","startedAt":"2026-10-15T15:19:04-04:00","nodes":[]}"#;

// one test: the reader's env (CWS_DAGU) is process-wide
#[test]
fn a_second_live_run_is_refused_and_names_the_first() {
    let live = dagu_fixture(&format!("{},{}", ME, BUSY));
    std::env::set_var("CWS_DAGU", format!("file://{}", live.0.display()));
    let err = live_run_refusal(&chorus_root(), 4536, "me").unwrap_err();
    assert_eq!(err, "refused: cicd run 1 is live (dagu busy)");

    // NEGATIVE PROOF: the same card with the other run finished goes on
    let done = dagu_fixture(&format!("{},{}", ME, DONE));
    std::env::set_var("CWS_DAGU", format!("file://{}", done.0.display()));
    assert!(live_run_refusal(&chorus_root(), 4536, "me").is_ok(), "a finished run must not refuse");

    // dagu unreadable: the run goes on (the build lock still guards), never a false refusal
    std::env::set_var("CWS_DAGU", "http://127.0.0.1:9");
    assert!(live_run_refusal(&chorus_root(), 4536, "me").is_ok());
}
