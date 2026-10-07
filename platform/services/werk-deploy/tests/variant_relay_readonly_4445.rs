//! #4445 — Jeff, 2026-10-07 16:49: "u test in demo". The variant Clearing had
//! no BUZZ_RELAY_URL, so its room never read the relay and could not show
//! Abby's replies: the demo proved nothing. A variant now reads the live relay
//! READ-ONLY, and env-up refuses a variant that could publish to it.
use werk_deploy::demo_env::{clearing_env_prod_leak, clearing_extra_env, relay_url_in_plist};

const RELAY: &str = "ws://192.0.2.10:3000";

fn get(env: &[(String, String)], k: &str) -> Option<String> {
    env.iter().find(|(kk, _)| kk == k).map(|(_, v)| v.clone())
}

#[test]
fn a_variant_reads_the_prod_relay_read_only() {
    for role in ["silas", "kade", "wren"] {
        let env = clearing_extra_env(role, "/w/.chorus-demo", "https://id.example", "/bin", Some(RELAY)).unwrap();
        assert_eq!(get(&env, "BUZZ_RELAY_URL").as_deref(), Some(RELAY), "{role}");
        assert_eq!(get(&env, "CLEARING_ROOM_READONLY").as_deref(), Some("1"), "{role}");
        assert_eq!(clearing_env_prod_leak(&env), None, "{role}");
    }
}

#[test]
fn with_no_prod_relay_the_variant_gets_neither() {
    let env = clearing_extra_env("silas", "/w/.chorus-demo", "https://id.example", "/bin", None).unwrap();
    assert_eq!(get(&env, "BUZZ_RELAY_URL"), None);
    assert_eq!(get(&env, "CLEARING_ROOM_READONLY"), None);
}

#[test]
fn negative_proof_a_variant_that_could_publish_to_the_relay_is_refused() {
    let base = clearing_extra_env("wren", "/w/.chorus-demo", "https://id.example", "/bin", Some(RELAY)).unwrap();
    // read-only dropped
    let no_ro: Vec<(String, String)> = base.iter().filter(|(k, _)| k != "CLEARING_ROOM_READONLY").cloned().collect();
    let hit = clearing_env_prod_leak(&no_ro).expect("a publishing variant was NOT caught");
    assert!(hit.starts_with("BUZZ_RELAY_URL="), "the refusal names the key: {hit}");
    // read-only set to anything but 1
    let ro0: Vec<(String, String)> = base.iter()
        .map(|(k, v)| if k == "CLEARING_ROOM_READONLY" { (k.clone(), "0".into()) } else { (k.clone(), v.clone()) }).collect();
    assert!(clearing_env_prod_leak(&ro0).is_some(), "CLEARING_ROOM_READONLY=0 was NOT caught");
}

#[test]
fn the_relay_is_read_from_the_prod_clearing_plist() {
    let plist = "<dict><key>EnvironmentVariables</key><dict><key>PATH</key><string>/bin</string>\
                 <key>BUZZ_RELAY_URL</key><string>ws://192.0.2.10:3000</string></dict></dict>";
    assert_eq!(relay_url_in_plist(plist).as_deref(), Some(RELAY));
    assert_eq!(relay_url_in_plist("<dict><key>PATH</key><string>/bin</string></dict>"), None);
    assert_eq!(relay_url_in_plist("<key>BUZZ_RELAY_URL</key><string></string>"), None);
}

#[test]
fn a_read_only_variant_clearing_starts_through_the_room_launcher_and_its_plist_holds_no_secret() {
    let clearing = werk_deploy::demo_env::env_services().into_iter().find(|s| s.name == "clearing").unwrap();
    let ro = werk_deploy::demo_env::generate_plist(&clearing, "silas", "/werk/silas-4445", 3481,
        &[("BUZZ_RELAY_URL", RELAY), ("CLEARING_ROOM_READONLY", "1")]);
    let a = ro.find("<string>/bin/bash</string>").expect("no bash launcher");
    let b = ro.find("/werk/silas-4445/platform/scripts/clearing-room-env").expect("no room launcher");
    let c = ro.find("/werk/silas-4445/directing/clearing/dist/server.js").expect("no server entry");
    assert!(a < b && b < c, "launcher must wrap the server: {ro}");
    assert!(!ro.contains("BUZZ_ROOM_SECRET"), "the secret must never be in the plist");
}

#[test]
fn negative_proof_a_variant_without_the_relay_starts_node_directly() {
    let clearing = werk_deploy::demo_env::env_services().into_iter().find(|s| s.name == "clearing").unwrap();
    let plain = werk_deploy::demo_env::generate_plist(&clearing, "silas", "/werk/silas-4445", 3481, &[]);
    assert!(!plain.contains("clearing-room-env"), "{plain}");
    let other = werk_deploy::demo_env::env_services().into_iter().find(|s| s.name != "clearing").unwrap();
    let not_clearing = werk_deploy::demo_env::generate_plist(&other, "silas", "/werk/silas-4445", 3343,
        &[("CLEARING_ROOM_READONLY", "1")]);
    assert!(!not_clearing.contains("clearing-room-env"), "only the Clearing is wrapped");
}
