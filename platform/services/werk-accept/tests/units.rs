//! Pure-helper unit tests (RED first). The NOVEL logic of accept is the authority
//! gate (DEC-048: only Wren/Jeff finalize; a builder never self-accepts). That's a
//! pure function — test it exhaustively here. Plus the shared verb-contract helpers.

use werk_accept::{branch_name, can_accept, demo_verdict_pass, jsonl_line, script_path};

#[test]
fn script_path_resolves_absolute_under_home_platform_scripts() {
    // #3183: werk-accept is exec'd by the chorus-mcp daemon, whose PATH lacks
    // platform/scripts — bare-name `cards` died "No such file or directory" (proven
    // LIVE accepting #3211). Resolve absolutely from home, like werk-pull's #3151 fix.
    use std::path::Path;
    assert_eq!(script_path(Path::new("/repo"), "cards"), "/repo/platform/scripts/cards");
    assert_eq!(script_path(Path::new("/x/y"), "chorus-werk"), "/x/y/platform/scripts/chorus-werk");
    assert_eq!(script_path(Path::new("/x/y"), "chorus-log"), "/x/y/platform/scripts/chorus-log");
}

#[test]
fn jeff_can_accept_any_card_including_jeff_owned() {
    assert!(can_accept("jeff", "kade"));
    assert!(can_accept("jeff", "silas"));
    assert!(can_accept("jeff", "wren"));
    // human final-authority is exempt from the self-accept rule (#3057 gate-arch):
    // there is no higher authority to protect against, so jeff accepts jeff-owned.
    assert!(can_accept("jeff", "jeff"));
}

#[test]
fn wren_can_accept_other_builders_cards() {
    assert!(can_accept("wren", "kade"));
    assert!(can_accept("wren", "silas"));
}

#[test]
fn builder_cannot_self_accept() {
    // DEC-048: the builder of a card can never finalize their own card.
    assert!(!can_accept("kade", "kade"));
    assert!(!can_accept("silas", "silas"));
    assert!(!can_accept("wren", "wren")); // even Wren can't self-accept her own card
}

#[test]
fn non_authority_roles_cannot_accept() {
    // only jeff/wren are accept authorities; kade/silas never finalize.
    assert!(!can_accept("kade", "silas"));
    assert!(!can_accept("silas", "kade"));
}

#[test]
fn branch_name_is_role_slash_card() {
    assert_eq!(branch_name("kade", 3057), "kade/3057");
}

#[test]
fn demo_verdict_pass_reads_the_demo_witness() {
    // #3116: accept gates on a demo.verdict=pass in the werk-demo witness.
    use std::path::PathBuf;
    let home: PathBuf = std::env::temp_dir().join(format!(
        "wa-verdict-{}-{}",
        std::process::id(),
        std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
    ));
    let logs = home.join("ops/logs");
    std::fs::create_dir_all(&logs).unwrap();
    let witness = logs.join("werk-demo.jsonl");

    // no witness → no demo ran → false
    assert!(!demo_verdict_pass(&home, 3116));

    // a PASS for #31160 must NOT satisfy #3116 (comma-terminated key guards the prefix)
    std::fs::write(&witness, "{\"event\":\"demo.verdict\",\"card_id\":31160,\"verdict\":\"pass\"}\n").unwrap();
    assert!(!demo_verdict_pass(&home, 3116));

    // a FAIL for #3116 → false
    std::fs::write(&witness, "{\"event\":\"demo.verdict\",\"card_id\":3116,\"verdict\":\"fail\"}\n").unwrap();
    assert!(!demo_verdict_pass(&home, 3116));

    // a PASS for #3116 → true
    std::fs::write(&witness, "{\"event\":\"demo.verdict\",\"card_id\":3116,\"verdict\":\"pass\",\"prover\":\"jeff\"}\n").unwrap();
    assert!(demo_verdict_pass(&home, 3116));

    let _ = std::fs::remove_dir_all(&home);
}

#[test]
fn jsonl_line_is_valid_witness_record() {
    let line = jsonl_line(1, "accept.completed", "kade", 3057, "t", ",\"sha\":\"abc\"");
    assert!(line.ends_with('\n'));
    assert!(line.contains("\"event\":\"accept.completed\""));
    assert!(line.contains("\"card_id\":3057"));
}
