//! #4227 — the demo's own chorus-hooks. Until this, a demo had never run one:
//! every hook in a session talked to the single prod daemon, so a card that
//! changed a guard, a gate or the write scrubber could not be shown working
//! before it landed.
use werk_deploy::demo_env::*;

const WERK: &str = "/Users/x/CascadeProjects/chorus-werk/silas-4227";

fn hooks() -> EnvService {
    env_services()
        .into_iter()
        .find(|s| s.name == "chorus-hooks")
        .expect("chorus-hooks must be an env service")
}

#[test]
fn a_demo_now_lists_its_own_hooks_daemon() {
    let names: Vec<String> = env_services().into_iter().map(|s| s.name).collect();
    assert!(names.contains(&"chorus-hooks".to_string()), "got {names:?}");
}

#[test]
fn the_socket_and_the_lock_live_under_the_werk_so_teardown_takes_them() {
    let dir = hooks_run_dir(WERK);
    assert!(dir.starts_with(WERK), "{dir} must be inside the werk");
    assert_eq!(hooks_socket_path(WERK), format!("{dir}/chorus-hooks.sock"));
}

#[test]
fn the_socket_name_matches_what_the_daemon_itself_writes() {
    // chorus-hooks' state_paths puts the socket at <run dir>/chorus-hooks.sock.
    // If these two ever disagree the smoke waits on a path nothing is serving,
    // and the demo reports a dead daemon as a timeout instead of a mismatch.
    assert!(hooks_socket_path(WERK).ends_with("/chorus-hooks.sock"));
}

#[test]
fn the_plist_carries_the_run_dir_and_no_port() {
    let svc = hooks();
    let plist = generate_plist(&svc, "silas", WERK, 0, &[("CHORUS_HOOKS_RUN_DIR", "/w/run")]);
    assert!(plist.contains("CHORUS_HOOKS_RUN_DIR"));
    assert!(plist.contains("/w/run"));
    // NEGATIVE PROOF, part one: chorus-hooks exits on an unknown argument, so
    // a --port appended the way the other werk-bin services get it would stop
    // the daemon from starting at all.
    assert!(!plist.contains("--port"), "chorus-hooks takes no --port:\n{plist}");
    // NEGATIVE PROOF, part two: an empty port_env must produce NO env entry,
    // not a blank key. launchd accepts <key></key> and nothing can read it.
    assert!(!plist.contains("<key></key>"), "blank env key in:\n{plist}");
}

#[test]
fn a_port_service_still_gets_its_port_env() {
    // The same code path must not have stopped writing ports for everyone else.
    let api = env_services().into_iter().find(|s| s.name == "chorus-api").unwrap();
    let plist = generate_plist(&api, "silas", WERK, 3343, &[]);
    assert!(plist.contains("CHORUS_API_PORT"));
    assert!(plist.contains("3343"));
}

#[test]
fn each_role_gets_its_own_hooks_dir() {
    let a = hooks_run_dir("/w/silas-4227");
    let b = hooks_run_dir("/w/kade-4222");
    assert_ne!(a, b, "two cards sharing a run dir is the lock collision again");
}

#[test]
fn a_portless_service_is_not_a_port_collision() {
    // chorus-hooks carries three zeros because it binds no port. Counting them
    // made the collision gate fire on a healthy env — a gate that always fires
    // is a gate nobody reads.
    assert_eq!(env_ports_collide(&env_services()), None);
}

#[test]
fn negative_proof_the_collision_gate_still_catches_a_real_one() {
    // The skip above must not have switched the gate off. Two real services on
    // one port must still be named.
    let mut svcs = env_services();
    let api_wren = svcs.iter().find(|s| s.name == "chorus-api").unwrap().wren_port;
    let c = svcs.iter_mut().find(|s| s.name == "clearing").unwrap();
    c.silas_port = api_wren;
    assert_eq!(env_ports_collide(&svcs).map(|(_, p)| p), Some(api_wren));
}
