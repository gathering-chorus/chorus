// #4225 — the numbers are fixtures, never this box: a check whose verdict
// depends on what happens to be running cannot be read as red or green.
use demo_fitness::*;

const FULL: &str = "\
830\t0\tcom.chorus.pulse
64992\t0\tcom.chorus.api
3295\t0\tcom.chorus.athena-make
95309\t0\tcom.chorus.api.werk.kade
95510\t0\tcom.chorus.mcp.werk.kade
95583\t0\tcom.chorus.athena-make.werk.kade
1335\t0\tcom.chorus.clearing.werk.kade
-\t0\tcom.chorus.cruft-scan
46419\t-9\tcom.gathering.fuseki
";

/// No verb binaries built — the state before a werk builds its own copy.
const NO_VERBS: [String; 0] = [];

/// The werk has built its own athena-model.
fn own_model() -> Vec<String> {
    vec!["athena-model".to_string()]
}

#[test]
fn a_demo_today_owns_four_of_the_six_and_the_two_it_lacks_are_named() {
    let f = measure(FULL, "kade", &NO_VERBS);
    assert_eq!(f.own, vec!["chorus-api", "chorus-mcp", "athena-make", "clearing"]);
    assert_eq!(f.missing, vec!["athena-model", "chorus-hooks"]);
    assert_eq!(f.target_total(), 6);
}

#[test]
fn a_scheduled_service_is_not_a_running_one() {
    // com.chorus.cruft-scan has "-" for a pid: configured, not up. Counting it
    // would report a demo as more complete than it is.
    let running = running_labels(FULL);
    assert!(!running.iter().any(|l| l == "com.chorus.cruft-scan"));
    assert!(running.iter().any(|l| l == "com.chorus.pulse"));
}

#[test]
fn the_shared_list_is_prod_services_only_never_the_variant_copies() {
    let f = measure(FULL, "kade", &NO_VERBS);
    assert!(f.shared.iter().all(|l| !l.contains(".werk.")));
    assert!(f.shared.iter().any(|l| l == "com.gathering.fuseki"));
}

#[test]
fn negative_proof_stop_one_variant_service_and_the_count_drops_and_names_it() {
    // The state this number exists to catch: the same world minus one variant.
    // If this ever matches the full reading, the measure is not measuring.
    let degraded: String = FULL
        .lines()
        .filter(|l| !l.contains("clearing.werk.kade"))
        .collect::<Vec<_>>()
        .join("\n");
    let full = measure(FULL, "kade", &NO_VERBS);
    let less = measure(&degraded, "kade", &NO_VERBS);
    assert_eq!(less.own.len(), 3);
    assert!(less.missing.contains(&"clearing".to_string()));
    assert_ne!(full.own.len(), less.own.len());
}

#[test]
fn another_roles_variant_does_not_count_as_mine() {
    // kade's copies are up; silas has none. Reading a peer's variant as your
    // own would report a demo that does not exist.
    let f = measure(FULL, "silas", &NO_VERBS);
    assert_eq!(f.own.len(), 0);
    assert_eq!(f.missing.len(), 6);
}

#[test]
fn the_series_reports_the_previous_run_back() {
    let a = series_line(&measure(FULL, "kade", &NO_VERBS), "2026-09-20T12:00:00Z");
    let b = series_line(&measure(FULL, "silas", &NO_VERBS), "2026-09-20T12:05:00Z");
    let series = format!("{}\n{}\n", a, b);
    assert_eq!(previous_own(&series), Some(0)); // silas ran last, owning none
    assert_eq!(previous_own(""), None);
    assert!(report(&measure(FULL, "kade", &NO_VERBS), Some(3)).contains("(prev 3)"));
}

#[test]
fn a_verb_is_counted_by_its_binary_never_by_a_running_process() {
    // athena-model runs, writes once and exits. Nothing is ever running, so
    // the launchctl output below is identical in both readings — the only
    // difference is whether the werk built its own copy of the verb.
    let without = measure(FULL, "kade", &NO_VERBS);
    let with = measure(FULL, "kade", &own_model());
    assert!(without.missing.contains(&"athena-model".to_string()));
    assert!(with.own.contains(&"athena-model".to_string()));
    assert_eq!(with.own.len(), without.own.len() + 1);
}

#[test]
fn negative_proof_a_verb_the_werk_never_built_is_still_missing() {
    // The failure this replaces cut the other way: counting athena-model as
    // present because it exists somewhere. An unrelated verb in the list must
    // not satisfy it, or the check passes for every demo forever.
    let f = measure(FULL, "kade", &["chorus-awake".to_string()]);
    assert!(f.missing.contains(&"athena-model".to_string()));
    assert_eq!(f.own.len(), 4);
}

#[test]
fn the_two_kinds_are_not_confused() {
    assert_eq!(piece_of("athena-model"), Some(Piece::Verb));
    assert_eq!(piece_of("chorus-hooks"), Some(Piece::Daemon));
    assert_eq!(piece_of("fuseki"), None);
    assert!(verb_bin_path("athena-model", "silas", "/w").ends_with("/silas-bin/athena-model"));
}
