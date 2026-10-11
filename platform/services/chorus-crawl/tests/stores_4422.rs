//! #4422 — the stores crawl, judged against the real map
//! (platform/config/store-map.json) and a picture of Library and Bedroom as
//! measured on 2026-10-11. Fixtures by repo path:
//!   platform/config/store-map.json

use chorus_crawl::infra::{plan, Row};
use chorus_crawl::stores::{absent, desired, parse_map, Finding, MachineSeen, Seen, StoreMap};

fn map() -> StoreMap {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    let text = std::fs::read_to_string(format!("{dir}/../../config/store-map.json")).expect("map");
    parse_map(&text).expect("the map parses")
}

const HOME: &str = "/Users/jeffbridwell";
const DATA: &str = "/System/Volumes/Data";

fn paths_of(map: &StoreMap, machine: &str) -> Vec<String> {
    map.entries
        .iter()
        .filter(|e| e.machine == machine)
        .filter_map(|e| match &e.matcher { chorus_crawl::stores::Match::Path(p) => Some(p.clone()), _ => None })
        .collect()
}

/// Library and Bedroom as measured on 2026-10-11.
fn today(map: &StoreMap) -> Seen {
    let lib_paths = paths_of(map, "library");
    let mut lib_mounts: Vec<(String, String)> = lib_paths.iter().map(|p| (p.clone(), DATA.to_string())).collect();
    lib_mounts.push((String::new(), DATA.into())); // port and dataset stores read the home volume
    let bed_paths = paths_of(map, "bedroom");
    let bed_mounts = bed_paths
        .iter()
        .map(|p| {
            let m = if p.starts_with("/Volumes/") { p.split('/').take(3).collect::<Vec<_>>().join("/") } else { DATA.to_string() };
            (p.clone(), m)
        })
        .collect();
    Seen {
        machines: vec![
            ("library".into(), Ok(MachineSeen { paths: lib_paths, ports: vec![3102, 3100, 9090, 3306], mounts: lib_mounts, scanned: vec![
                format!("{HOME}/.chorus/index.db"),
                format!("{HOME}/.chorus/messages.db"),
                format!("{HOME}/.chorus/vikunja/db/vikunja.db.bak-chunk-final"),
            ] })),
            ("bedroom".into(), Ok(MachineSeen { paths: bed_paths, ports: vec![], mounts: bed_mounts, scanned: vec![] })),
        ],
        datasets: Some(["/pods", "/staging", "/chorus-test", "/chorus-test-proto", "/werk-silas", "/werk-kade-4224", "/werk-kade-4154"].map(String::from).to_vec()),
        graphs: Some(
            [
                "urn:chorus:ontology", "urn:chorus:documents", "urn:chorus:domains:security", "urn:chorus:domains:cards",
                "urn:chorus:domains:logs", "urn:chorus:domains:infrastructure", "urn:borg:instances", "urn:chorus:previous:urn:borg:instances",
                "urn:gathering:photos", "http://jeffbridwell.com/gathering/icd/current", "urn:jb:photo:1", "urn:jb:photo:2", "urn:jb:note:3",
                "urn:chorus:instances-bt5", "urn:chorus:scopeproof",
            ]
            .map(String::from)
            .to_vec(),
        ),
    }
}

fn names(rows: &[Row]) -> Vec<&str> {
    rows.iter().map(|r| r.name.as_str()).collect()
}

#[test]
fn the_map_ships_with_zero_unmapped_today() {
    let m = map();
    let d = desired(&m, &today(&m), "2026-10-11T20:00:00Z");
    let unmapped: Vec<&Finding> = d.findings.iter().filter(|f| matches!(f, Finding::Unmapped { .. })).collect();
    assert!(unmapped.is_empty(), "unmapped: {unmapped:?}");
    assert!(d.findings.is_empty(), "findings: {:?}", d.findings);
}

#[test]
fn negative_proof_an_unknown_dataset_is_unmapped() {
    let m = map();
    let mut s = today(&m);
    s.datasets.as_mut().unwrap().push("/mystery".into());
    let d = desired(&m, &s, "t");
    assert!(d.findings.contains(&Finding::Unmapped { what: "Fuseki dataset /mystery".into() }));
}

#[test]
fn negative_proof_an_unknown_graph_and_file_are_unmapped() {
    let m = map();
    let mut s = today(&m);
    s.graphs.as_mut().unwrap().push("urn:elsewhere:x".into());
    if let Ok(ms) = &mut s.machines[0].1 {
        ms.scanned.push(format!("{HOME}/.chorus/new.db"));
    }
    let d = desired(&m, &s, "t");
    assert!(d.findings.contains(&Finding::Unmapped { what: "graph urn:elsewhere:x".into() }));
    assert!(d.findings.contains(&Finding::Unmapped { what: format!("file library:{HOME}/.chorus/new.db") }));
}

#[test]
fn residue_and_strays_are_reported_never_rows() {
    let m = map();
    let d = desired(&m, &today(&m), "t");
    assert!(d.reported.iter().any(|r| r.contains("urn:chorus:instances-bt5")));
    assert!(d.reported.iter().any(|r| r.contains(".chorus/messages.db")));
    assert!(d.reported.iter().any(|r| r.contains("vikunja.db.bak-")));
    assert!(!names(&d.rows).iter().any(|n| n.contains("bt5") || n.contains("bak")));
}

#[test]
fn graph_families_one_row_each_exact_wins_over_prefix() {
    let m = map();
    let d = desired(&m, &today(&m), "t");
    let n = names(&d.rows);
    assert!(n.contains(&"pods-cards") && n.contains(&"pods-security") && n.contains(&"pods-ontology"));
    assert!(n.contains(&"pods-domain-logs") && n.contains(&"pods-domain-infrastructure"));
    assert!(!n.contains(&"pods-domain-cards"), "cards has its own entry");
    let jb = d.rows.iter().find(|r| r.name == "pods-jb").unwrap();
    assert_eq!(jb.get("storeRecordCount"), "3");
    assert_eq!(d.rows.iter().filter(|r| r.name.starts_with("pods-jb")).count(), 1);
}

#[test]
fn every_row_has_exactly_one_parent_and_a_source_unless_record() {
    let m = map();
    let d = desired(&m, &today(&m), "t");
    for r in &d.rows {
        let parents = [r.get("onDisk"), r.get("inStore")].iter().filter(|v| !v.is_empty()).count();
        assert_eq!(parents, 1, "{} parents on {}", parents, r.name);
        assert_eq!(r.get("storeRole") == "systemOfRecord", r.get("roleSource").is_empty(), "role/source on {}", r.name);
    }
    let onto = d.rows.iter().find(|r| r.name == "pods-ontology").unwrap();
    assert_eq!((onto.get("storeRole"), onto.get("roleSource"), onto.get("inStore")), ("copyOf", "git-chorus", "fuseki-pods"));
    let git = d.rows.iter().find(|r| r.name == "git-chorus").unwrap();
    assert_eq!(git.get("onDisk"), "library-system-volumes-data");
    let riley = d.rows.iter().find(|r| r.name == "bedroom-photos").unwrap();
    assert_eq!(riley.get("onDisk"), "bedroom-volumes-photosnew");
}

#[test]
fn test_datasets_are_a_row_each() {
    let m = map();
    let d = desired(&m, &today(&m), "t");
    let n = names(&d.rows);
    for x in ["fuseki-test-werk-silas", "fuseki-test-werk-kade-4154", "fuseki-test-chorus-test", "fuseki-test-chorus-test-proto"] {
        assert!(n.contains(&x), "{x} missing from {n:?}");
    }
}

#[test]
fn negative_proof_an_unwalkable_bedroom_is_unmeasured_never_absent() {
    let m = map();
    let first = desired(&m, &today(&m), "2026-10-11T20:00:00Z");
    let mut s = today(&m);
    s.machines[1].1 = Err("ssh: connect to host bedroom port 22: Operation timed out".into());
    let d = desired(&m, &s, "2026-10-11T20:05:00Z");
    assert!(d.findings.iter().any(|f| matches!(f, Finding::Unmeasured { store, .. } if store == "bedroom-mongodb")));
    let gone = absent(&first.rows, &d);
    assert!(gone.is_empty(), "marked absent while unmeasured: {:?}", names(&gone));
}

#[test]
fn ac4_second_run_writes_nothing_and_a_dropped_dataset_is_marked_absent() {
    let iso = |s: &str| chorus_crawl::services::secs_from_iso(s);
    let m = map();
    let run1 = desired(&m, &today(&m), "2026-10-11T20:00:00Z");
    let p1 = plan(run1.rows.clone(), &[], 0, &iso);
    assert_eq!(p1.create.len(), run1.rows.len());
    let held = p1.create;
    let now = iso("2026-10-11T20:01:00Z").unwrap();
    let run2 = desired(&m, &today(&m), "2026-10-11T20:01:00Z");
    let p2 = plan(run2.rows.clone(), &held, now, &iso);
    assert_eq!((p2.create.len(), p2.update.len()), (0, 0));
    assert!(absent(&held, &run2).is_empty());
    // drop one werk dataset: run 3 marks exactly that row absent
    let mut s = today(&m);
    s.datasets.as_mut().unwrap().retain(|d| d != "/werk-kade-4154");
    let run3 = desired(&m, &s, "2026-10-11T20:02:00Z");
    let gone = absent(&held, &run3);
    assert_eq!(names(&gone), vec!["fuseki-test-werk-kade-4154"]);
    assert_eq!(gone[0].get("storeStatus"), "absent");
}

#[test]
fn negative_proof_the_map_refuses_a_copy_without_a_source_and_a_dangling_source() {
    let bad = r#"{"stores":[{"name":"a","machine":"library","storeType":"x","match":{"kind":"path","value":"/a"},"role":"copyOf","basis":"measured"}]}"#;
    assert!(parse_map(bad).unwrap_err().contains("must name a source"));
    let dangling = r#"{"stores":[{"name":"a","machine":"library","storeType":"x","match":{"kind":"path","value":"/a"},"role":"copyOf","source":"nowhere","basis":"measured"}]}"#;
    assert!(parse_map(dangling).unwrap_err().contains("names no store"));
    let sor = r#"{"stores":[{"name":"a","machine":"library","storeType":"x","match":{"kind":"path","value":"/a"},"role":"systemOfRecord","source":"a","basis":"measured"}]}"#;
    assert!(parse_map(sor).unwrap_err().contains("cannot name"));
    // a role with no word on how it is known is refused: a guess is never unlabelled
    let unlabelled = r#"{"stores":[{"name":"a","machine":"library","storeType":"x","match":{"kind":"path","value":"/a"},"role":"systemOfRecord"}]}"#;
    assert!(parse_map(unlabelled).unwrap_err().contains("say how the role is known"));
}

#[test]
fn the_video_archive_is_a_closed_row_per_drive_and_videosnew_is_live() {
    let m = map();
    let mut s = today(&m);
    if let Ok(ms) = &mut s.machines[1].1 {
        for p in ["/Volumes/VideosRiley-Te", "/Volumes/VideosNew"] {
            ms.paths.push(p.into());
            ms.mounts.push((p.into(), p.into()));
        }
    }
    let d = desired(&m, &s, "t");
    let riley = d.rows.iter().find(|r| r.name == "bedroom-video-videosriley-te").expect("a row per archive drive");
    assert_eq!((riley.get("writeMode"), riley.get("onDisk")), ("closed", "bedroom-volumes-videosriley-te"));
    let new = d.rows.iter().find(|r| r.name == "bedroom-video-new").unwrap();
    assert_eq!(new.get("writeMode"), "live");
    assert!(!d.rows.iter().any(|r| r.name == "bedroom-video-videosnew"), "VideosNew has its own entry");
}

#[test]
fn negative_proof_only_a_live_store_makes_a_full_disk_a_finding() {
    use chorus_crawl::infra::{disk_findings, DiskReading, Walk};
    use chorus_crawl::stores::live_store_disks;
    let full = DiskReading { mount: "/Volumes/VideosRiley-Te".into(), capacity_bytes: 100, used_bytes: 99, available_bytes: 1 };
    let store = |mode: &str| Row {
        kind: "PersistenceStore",
        name: "bedroom-video-videosriley-te".into(),
        fields: vec![("onDisk".into(), "bedroom-volumes-videosriley-te".into()), ("writeMode".into(), mode.into()), ("storeStatus".into(), "present".into())],
    };
    let mounts_for = |rows: &[Row]| -> Vec<String> {
        let disks = live_store_disks(rows);
        [&full].iter().filter(|d| disks.contains(&chorus_crawl::infra::disk_name("bedroom", &d.mount))).map(|d| d.mount.clone()).collect()
    };
    let closed = mounts_for(&[store("closed")]);
    let refs: Vec<&str> = closed.iter().map(String::as_str).collect();
    assert!(disk_findings("bedroom", &Walk::Walked(vec![full.clone()]), &refs).is_empty());
    let live = mounts_for(&[store("live")]);
    let refs: Vec<&str> = live.iter().map(String::as_str).collect();
    assert_eq!(disk_findings("bedroom", &Walk::Walked(vec![full.clone()]), &refs).len(), 1);
}

#[test]
fn a_row_as_the_door_lists_it_compares_unchanged() {
    // the door's listing of pods-ontology on staging, 2026-10-11 17:00
    let listed: Vec<(String, String)> = [
        ("name", "pods-ontology"), ("label", "/pods ontology graph"), ("status", ""), ("inStore", "persistence-store-fuseki-pods"),
        ("lastObserved", "2026-10-11T21:00:54Z"), ("onDisk", ""), ("roleSource", "persistence-store-git-chorus"),
        ("storeNamespace", "urn:chorus:ontology"), ("storeRole", "copyOf"), ("storeServedBy", ""), ("storeStatus", "present"),
        ("storeType", "graph-family"), ("writeMode", "live"), ("roleBasis", "inventory"),
    ]
    .iter()
    .map(|(k, v)| (k.to_string(), v.to_string()))
    .collect();
    let held = vec![Row { kind: "PersistenceStore", name: "pods-ontology".into(), fields: chorus_crawl::stores::door_form(listed.clone()) }];
    let m = map();
    let want: Vec<Row> = desired(&m, &today(&m), "2026-10-11T21:01:00Z").rows.into_iter().filter(|r| r.name == "pods-ontology").collect();
    let iso = |s: &str| chorus_crawl::services::secs_from_iso(s);
    let p = plan(want.clone(), &held, iso("2026-10-11T21:01:00Z").unwrap(), &iso);
    assert_eq!((p.create.len(), p.update.len(), p.unchanged), (0, 0, 1));
    // NEGATIVE PROOF: compared without the door form, the prefixed links read as a change
    let raw = vec![Row { kind: "PersistenceStore", name: "pods-ontology".into(), fields: listed }];
    let p = plan(want, &raw, iso("2026-10-11T21:01:00Z").unwrap(), &iso);
    assert_eq!(p.update.len(), 1);
}

#[test]
fn every_row_says_how_its_role_is_known() {
    let m = map();
    let d = desired(&m, &today(&m), "t");
    for r in &d.rows {
        assert!(["measured", "inventory", "inferred"].contains(&r.get("roleBasis")), "{} has roleBasis {:?}", r.name, r.get("roleBasis"));
    }
    let lance = d.rows.iter().find(|r| r.name == "pods-jb").unwrap();
    assert_eq!(lance.get("roleBasis"), "inferred");
}

#[test]
fn negative_proof_a_store_on_a_disk_with_no_row_is_held_and_named() {
    let m = map();
    let mut d = desired(&m, &today(&m), "t");
    let before = d.rows.len();
    // every disk written except Bedroom's photo volume
    let mut disks: Vec<String> = d.rows.iter().map(|r| r.get("onDisk").to_string()).filter(|x| !x.is_empty()).collect();
    disks.retain(|x| x != "bedroom-volumes-photosnew");
    chorus_crawl::stores::hold_unwritten_disks(&mut d, &disks);
    assert_eq!(d.rows.len(), before - 1);
    assert!(d.findings.iter().any(|f| matches!(f, Finding::Unmeasured { store, reason } if store == "bedroom-photos" && reason.contains("chorus-crawl infra"))));
}
