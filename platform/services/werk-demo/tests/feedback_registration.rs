//! #3479 — feedback registration: a peer's gather reply must WRITE
//! demo.gather.replied (the missing wire). The read side (gathers_missing) is
//! already tested in lib; this pins the WRITER produces a record with the fields
//! the read side keys on (event + peer + verdict + note), so the round-trip closes.

use std::path::PathBuf;
use werk_demo::record_gather_replied;

#[test]
fn record_gather_replied_writes_the_record_the_gate_reads() {
    let tmp: PathBuf = std::env::temp_dir().join(format!("werkdemo-3479-{}", std::process::id()));
    std::fs::create_dir_all(tmp.join("ops/logs")).unwrap();

    record_gather_replied(&tmp, "wren", 999999, "kade", "pass", "looks good");

    let w = std::fs::read_to_string(tmp.join("ops/logs/werk-demo.jsonl")).unwrap();
    assert!(w.contains("\"event\":\"demo.gather.replied\""), "writes the replied event the gate counts");
    assert!(w.contains("\"peer\":\"kade\""), "records which peer replied (gathers_missing keys on peer)");
    assert!(w.contains("\"verdict\":\"pass\""), "records the verdict");
    assert!(w.contains("\"note\":\"looks good\""), "records the substance");
    // keyed for line_keyed round/patch matching (#3461)
    assert!(w.contains("\"round\":") && w.contains("\"patch_id\":"), "keyed for rebase-survival match");

    let _ = std::fs::remove_dir_all(&tmp);
}
