// @domain: code — placed by chorus-crawl's domain rules (#4538)
//! #4422 reopen — the machine walks the crawls run, judged at their boundary:
//! the stores walk's shell line is RUN (sh, a temp HOME, no network, no store)
//! and its output parsed; Bedroom's ssh walk is parsed from text shaped as
//! Bedroom printed it on 2026-10-11; the write order is checked per kind.

use chorus_crawl::infra::{creates_by_kind, Row};
use chorus_crawl::services::parse_bedroom_walk;
use chorus_crawl::stores::{create_groups, datasets_from, parse_map, parse_seen, present_count, walk_cmd};

fn tmp(tag: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("crawl-walk-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn map_for(machine: &str, path: &str, prefix: &str) -> chorus_crawl::stores::StoreMap {
    parse_map(&format!(
        r#"{{"stores":[
          {{"name":"a","machine":"{machine}","storeType":"file","match":{{"kind":"path","value":"{path}"}},"role":"systemOfRecord","basis":"measured"}},
          {{"name":"gone","machine":"{machine}","storeType":"file","match":{{"kind":"path","value":"{path}-not-there"}},"role":"systemOfRecord","basis":"measured"}},
          {{"name":"drives","machine":"{machine}","storeType":"dir","match":{{"kind":"path-prefix","prefix":"{prefix}"}},"role":"systemOfRecord","basis":"measured","writeMode":"closed"}},
          {{"name":"elsewhere","machine":"other","storeType":"file","match":{{"kind":"path","value":"/elsewhere"}},"role":"systemOfRecord","basis":"measured"}}
        ]}}"#
    ))
    .expect("map parses")
}

#[test]
fn the_stores_walk_runs_and_reads_back_what_exists_on_the_machine() {
    let home = tmp("home");
    let store = home.join("store.db");
    std::fs::write(&store, "x").unwrap();
    std::fs::create_dir_all(home.join("archive-1")).unwrap();
    std::fs::create_dir_all(home.join(".chorus")).unwrap();
    std::fs::write(home.join(".chorus/index.db"), "x").unwrap();
    let map = map_for("library", store.to_str().unwrap(), home.join("archive-").to_str().unwrap());
    let cmd = walk_cmd(&map, "library");
    assert!(!cmd.contains("/elsewhere"), "another machine's path is not walked here");
    let out = std::process::Command::new("sh").args(["-c", &cmd]).env("HOME", &home).output().unwrap();
    let seen = parse_seen(&String::from_utf8_lossy(&out.stdout)).expect("the walk finished");
    let s = store.to_str().unwrap().to_string();
    assert!(seen.paths.contains(&s), "the mapped path that exists is seen: {:?}", seen.paths);
    assert!(!seen.paths.iter().any(|p| p.ends_with("-not-there")), "a mapped path that is gone is not seen");
    assert!(seen.paths.iter().any(|p| p.ends_with("/archive-1")), "each path under a prefix is seen");
    assert!(seen.mounts.iter().any(|(p, m)| p == &s && m.starts_with('/')), "each seen path carries its mount");
    assert!(seen.mounts.iter().any(|(p, m)| p.is_empty() && m.starts_with('/')), "the home volume's mount is read");
    assert!(seen.scanned.iter().any(|f| f.ends_with(".chorus/index.db")), "the scanned directory is listed");
}

#[test]
fn negative_proof_a_walk_cut_short_is_an_error_never_an_empty_machine() {
    assert!(parse_seen("MOUNT:\t/\nPATH:/x\n").is_err());
    assert!(parse_seen("").is_err());
    assert!(parse_seen("PATH:/x\nENDING\n").is_err(), "only a line that is END ends the walk");
    let seen = parse_seen("MOUNT:\t/\nPORT:3030\nPORT:junk\nEND\n").unwrap();
    assert_eq!(seen.ports, vec![3030], "a port line that is not a number is dropped");
    assert!(!seen.ports_unmeasured);
    assert!(parse_seen("PORTS-UNMEASURED\nEND\n").unwrap().ports_unmeasured, "no lsof reads unmeasured, not no ports");
}

#[test]
fn fuseki_datasets_are_read_by_name() {
    let text = r#"{ "datasets" : [ { "ds.name" : "/pods", "ds.state" : true }, { "ds.name" : "/staging" } ] }"#;
    assert_eq!(datasets_from(text), vec!["/pods", "/staging"]);
    assert!(datasets_from("{}").is_empty());
}

fn row(kind: &'static str, name: &str, fields: &[(&str, &str)]) -> Row {
    Row { kind, name: name.into(), fields: fields.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect() }
}

#[test]
fn stores_are_created_parents_first() {
    let creates = vec![
        row("PersistenceStore", "graph-x", &[("inStore", "pods"), ("roleSource", "")]),
        row("PersistenceStore", "copy-y", &[("roleSource", "git")]),
        row("PersistenceStore", "pods", &[]),
    ];
    let [first, sources, rest] = create_groups(&creates);
    let names = |g: &Vec<&Row>| g.iter().map(|r| r.name.clone()).collect::<Vec<_>>();
    assert_eq!(names(&first), vec!["pods"]);
    assert_eq!(names(&sources), vec!["copy-y"]);
    assert_eq!(names(&rest), vec!["graph-x"]);
}

#[test]
fn the_read_back_counts_only_present_rows() {
    let r = |s: &str| vec![("name".to_string(), "n".to_string()), ("storeStatus".to_string(), s.to_string())];
    assert_eq!(present_count(&[r("present"), r("absent"), r("present"), vec![]]), 2);
}

#[test]
fn negative_proof_a_new_machine_is_never_posted_into_the_disk_batch() {
    let creates = vec![row("Machine", "kitchen", &[]), row("Disk", "kitchen-root", &[]), row("Disk", "library-data", &[])];
    let by = creates_by_kind(&creates);
    assert_eq!(by.iter().map(|(k, v)| (*k, v.len())).collect::<Vec<_>>(), vec![("Machine", 1), ("Disk", 2)], "machine first, then its disks");
}

/// Bedroom's walk as it printed it on 2026-10-11 (two units; one LaunchAgent
/// with a print block and a log, one daemon with no plist read).
const BEDROOM: &str = "PID\tStatus\tLabel\n412\t0\tcom.gathering.navidrome\n-\t78\tcom.chorus.launchagent-metrics\n-\t0\tcom.chorus.no-plist\n\
---PLISTS---\n\
FILE:/Users/jeffbridwell/Library/LaunchAgents/com.chorus.launchagent-metrics.plist\n\
{\"Label\":\"com.chorus.launchagent-metrics\",\"ProgramArguments\":[\"/bin/sh\",\"m.sh\"],\"StartInterval\":15,\"StandardOutPath\":\"/tmp/launchagent-metrics.log\"}\n\
FILE:/Users/jeffbridwell/Library/LaunchAgents/com.gathering.navidrome.plist\n\
{\"Label\":\"com.gathering.navidrome\",\"ProgramArguments\":[\"/opt/navidrome\"],\"KeepAlive\":true}\n\
---PRINT---\n\
UNIT:com.chorus.launchagent-metrics\n\truns = 92086\n\tlast exit code = 0\nlogmtime = 1791684012\n\
UNIT:com.gathering.navidrome\n\truns = 1\n\tlast exit code = (never exited)\n\
---DISABLED---\n\tdisabled services = {\n\t\t\"com.chorus.launchagent-metrics\" => enabled\n\t\t\"com.gathering.navidrome\" => disabled\n\t}\n\
---LSOF---\np412\nn*:4533\n---PS---\n  412     1\n";

#[test]
fn bedroom_units_carry_runs_exit_enabled_log_and_ports_from_one_walk() {
    let units = parse_bedroom_walk(BEDROOM);
    let u = |l: &str| units.iter().find(|u| u.label == l).unwrap_or_else(|| panic!("{l} missing"));
    let m = u("com.chorus.launchagent-metrics");
    assert_eq!((m.meta.runs, m.last_exit_code, m.meta.disabled, m.meta.log_written), (Some(92086), Some(0), Some(false), Some(1_791_684_012)));
    assert!(!m.evidence_unavailable);
    let n = u("com.gathering.navidrome");
    assert!(n.meta.never_exited, "(never exited) is read as never ran");
    assert_eq!(n.meta.disabled, Some(true));
    assert_eq!(n.ports, vec![4533]);
    assert_eq!(n.meta.log_written, None, "no log, no time");
}

#[test]
fn negative_proof_a_bedroom_unit_with_no_plist_read_is_unavailable_and_its_list_zero_is_not_a_clean_exit() {
    let units = parse_bedroom_walk(BEDROOM);
    let x = units.iter().find(|u| u.label == "com.chorus.no-plist").expect("listed");
    assert!(x.evidence_unavailable);
    assert_eq!(x.last_exit_code, None, "list's 0 cannot tell a clean exit from never-run");
    assert_eq!(x.meta.disabled, None, "not looked at is absent, never false");
    // and an ssh answer with no sections still lists what launchctl listed
    assert_eq!(parse_bedroom_walk("PID\tStatus\tLabel\n-\t3\tcom.a\n").iter().map(|u| u.last_exit_code).collect::<Vec<_>>(), vec![Some(3)]);
}

#[test]
fn domain_graphs_are_the_rows_instances_graphs_that_exist_plus_the_maps_own() {
    let map = parse_map(
        r#"{"stores":[{"name":"pods","machine":"library","storeType":"fuseki-dataset","match":{"kind":"fuseki-dataset","value":"/pods"},"role":"systemOfRecord","basis":"measured"},
          {"name":"jb","inStore":"pods","storeType":"graph","match":{"kind":"graph","value":"urn:jb:seeds/"},"role":"systemOfRecord","basis":"measured"}]}"#,
    )
    .unwrap();
    let d = |g: &str| vec![("name".to_string(), "x".to_string()), ("instancesGraph".to_string(), g.to_string())];
    let rows = vec![d("urn:chorus:domains:cards"), d("urn:chorus:domains:empty"), d(""), vec![("name".into(), "no-graph".into())]];
    let exists = |g: &str| Some(g != "urn:chorus:domains:empty");
    assert_eq!(
        chorus_crawl::stores::domain_graphs(&rows, &map, &exists),
        Some(vec!["urn:chorus:domains:cards".to_string(), "urn:jb:seeds/".to_string()]),
        "a domain with no rows yet is not a store; a row with no graph is skipped"
    );
}

#[test]
fn negative_proof_a_graph_store_that_cannot_be_asked_makes_every_graph_unmeasured() {
    let map = parse_map(r#"{"stores":[]}"#).unwrap();
    let rows = vec![vec![("instancesGraph".to_string(), "urn:chorus:domains:cards".to_string())]];
    assert_eq!(chorus_crawl::stores::domain_graphs(&rows, &map, &|_| None), None);
}

fn disk(mount: &str, used: u64, avail: u64) -> chorus_crawl::infra::DiskReading {
    chorus_crawl::infra::DiskReading { mount: mount.into(), capacity_bytes: used + avail, used_bytes: used, available_bytes: avail }
}

#[test]
fn a_full_disk_is_a_finding_only_when_a_live_store_sits_on_it() {
    use chorus_crawl::infra::{disk_name, walk_results, Finding, MachineFacts};
    let facts = MachineFacts { hostname: "bedroom.local".into(), ..Default::default() };
    let disks = vec![disk("/Volumes/Video", 99, 1), disk("/Volumes/New", 97, 3), disk("/System/Volumes/Data", 10, 90)];
    let machines = vec![("bedroom", Ok((facts, disks)))];
    let (rows, found, notes) = walk_results(&machines, &[disk_name("bedroom", "/Volumes/New")], "2026-10-12T00:00:00Z");
    assert_eq!(rows.iter().map(|r| r.kind).collect::<Vec<_>>(), vec!["Machine", "Disk", "Disk", "Disk"], "the machine row comes before its disks");
    assert_eq!(found, vec![Finding::DiskFull { machine: "bedroom".into(), mount: "/Volumes/New".into(), percent: 97 }]);
    assert!(notes.iter().any(|n| n.contains("/Volumes/Video 99% full — archive")), "{notes:?}");
}

#[test]
fn negative_proof_a_machine_not_walked_is_unmeasured_never_a_machine_with_no_disks() {
    use chorus_crawl::infra::{walk_results, Finding};
    let machines = vec![("bedroom", Err("ssh: connect timed out".to_string()))];
    let (rows, found, _) = walk_results(&machines, &[], "t");
    assert!(rows.is_empty());
    assert_eq!(found, vec![Finding::Unmeasured { machine: "bedroom".into(), reason: "ssh: connect timed out".into() }]);
}
