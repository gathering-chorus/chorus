//! #4485 — the infrastructure crawl: machines and their disks, measured on the
//! machines themselves. The df fixtures are real output from Library and
//! Bedroom (2026-10-11), so what is proven is what a pass reads.
//! Fixtures by repo path:
//!   platform/services/chorus-crawl/tests/fixtures/infra-4485/df-library.txt
//!   platform/services/chorus-crawl/tests/fixtures/infra-4485/df-bedroom.txt
//!
//! The two negative proofs Wren asked for come first: a disk over 90% is a
//! finding, and a machine the crawl could not walk reads UNMEASURED for its
//! disks, never "disk ok".
//!
//! Which disks can fail (Wren, 14:04): the system volume, and any disk a store
//! sits on (`onDisk`). A full disk with neither is an archive drive, filled on
//! purpose: reported, never a finding. Worked out from the links, no name list.

use chorus_crawl::infra::{disk_findings, parse_df, Finding, Walk};

fn fixture(name: &str) -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/fixtures/infra-4485/{name}")).expect("fixture")
}

#[test]
fn negative_proof_a_disk_over_90_is_a_finding() {
    // a Library Data volume at 95%: the same df line as the fixture, used bumped
    let df = "Filesystem 1024-blocks Used Available Capacity Mounted on\n\
              /dev/disk3s5 1000000 950000 50000 95% /System/Volumes/Data\n";
    let walk = Walk::Walked(parse_df(df));
    let found = disk_findings("library", &walk, &[]);
    assert_eq!(found, vec![Finding::DiskFull { machine: "library".into(), mount: "/System/Volumes/Data".into(), percent: 95 }]);
}

#[test]
fn negative_proof_an_unwalkable_machine_is_unmeasured_never_ok() {
    let walk = Walk::Unwalkable("ssh: connect to host bedroom port 22: Operation timed out".into());
    let found = disk_findings("bedroom", &walk, &[]);
    assert_eq!(found.len(), 1);
    assert!(matches!(&found[0], Finding::Unmeasured { machine, .. } if machine == "bedroom"));
}

#[test]
fn negative_proof_a_full_disk_a_store_sits_on_is_a_finding() {
    let walk = Walk::Walked(parse_df(&fixture("df-bedroom.txt")));
    let found = disk_findings("bedroom", &walk, &["/Volumes/VideosRiley-Te"]);
    assert_eq!(found, vec![Finding::DiskFull { machine: "bedroom".into(), mount: "/Volumes/VideosRiley-Te".into(), percent: 100 }]);
}

#[test]
fn full_archive_disks_with_no_store_are_not_findings() {
    // Bedroom today: 18 volumes over 90%, every one an archive drive
    let disks = parse_df(&fixture("df-bedroom.txt"));
    assert_eq!(disks.iter().filter(|d| d.percent() > 90).count(), 18);
    assert_eq!(disk_findings("bedroom", &Walk::Walked(disks), &[]), vec![]);
}

#[test]
fn library_today_has_no_finding() {
    let walk = Walk::Walked(parse_df(&fixture("df-library.txt")));
    assert_eq!(disk_findings("library", &walk, &[]), vec![]);
}

#[test]
fn df_keeps_real_volumes_only() {
    let mounts: Vec<String> = parse_df(&fixture("df-library.txt")).into_iter().map(|d| d.mount).collect();
    // devfs, map auto_home and the NFS mount of Bedroom's drive are not Library's disks;
    // the sealed system snapshots (VM, Preboot, Update, xarts, iSCPreboot, Hardware) are not either.
    assert_eq!(mounts, vec!["/", "/System/Volumes/Data", "/Volumes/Buzz"]);
}

#[test]
fn df_reads_bytes_not_blocks() {
    let disks = parse_df(&fixture("df-library.txt"));
    let data = disks.iter().find(|d| d.mount == "/System/Volumes/Data").unwrap();
    assert_eq!(data.capacity_bytes, 1_948_455_240 * 1024);
    assert_eq!(data.used_bytes, 1_275_696_880 * 1024);
}

#[test]
fn bedroom_lists_every_external_volume() {
    let disks = parse_df(&fixture("df-bedroom.txt"));
    assert_eq!(disks.iter().filter(|d| d.mount.starts_with("/Volumes/")).count(), 25);
}

use chorus_crawl::infra::{disk_name, disk_row, machine_row, parse_walk, plan, MachineFacts, Row};

fn iso(s: &str) -> Option<u64> {
    chorus_crawl::services::secs_from_iso(s)
}

#[test]
fn a_walk_with_no_df_is_an_error_never_no_disks() {
    assert!(parse_walk("HOST:Jeffs-Mac-mini\nMODEL:Mac14,12\n").is_err());
}

#[test]
fn a_walk_reads_the_machine_and_its_disks() {
    let text = format!("HOST:Jeffs-Mac-mini\nMODEL:Mac14,12\nOS:26.6.2\n---DF---\n{}", fixture("df-bedroom.txt"));
    let (facts, disks) = parse_walk(&text).unwrap();
    assert_eq!(facts, MachineFacts { hostname: "Jeffs-Mac-mini".into(), hardware_model: "Mac14,12".into(), os_version: "26.6.2".into() });
    assert_eq!(disks.len(), 27); // / + Data + 25 external
}

#[test]
fn disk_names_are_door_slugs() {
    assert_eq!(disk_name("library", "/"), "library-root");
    assert_eq!(disk_name("library", "/System/Volumes/Data"), "library-system-volumes-data");
    assert_eq!(disk_name("bedroom", "/Volumes/VideosRiley-Te"), "bedroom-volumes-videosriley-te");
}

fn data_disk(used: u64) -> chorus_crawl::infra::DiskReading {
    chorus_crawl::infra::DiskReading { mount: "/System/Volumes/Data".into(), capacity_bytes: 1000, used_bytes: used, available_bytes: 1000 - used }
}

#[test]
fn second_run_writes_nothing() {
    // AC2: run 1 creates, run 2 a minute later (used bytes moved, same percent) writes 0
    let first = plan(vec![disk_row("library", &data_disk(671), "2026-10-11T18:00:00Z")], &[], 0, &iso);
    assert_eq!(first.create.len(), 1);
    let held = first.create;
    let now = iso("2026-10-11T18:01:00Z").unwrap();
    let second = plan(vec![disk_row("library", &data_disk(674), "2026-10-11T18:01:00Z")], &held, now, &iso);
    assert_eq!((second.create.len(), second.update.len(), second.unchanged), (0, 0, 1));
}

#[test]
fn negative_proof_a_percent_move_is_written() {
    let held = vec![disk_row("library", &data_disk(670), "2026-10-11T18:00:00Z")];
    let now = iso("2026-10-11T18:01:00Z").unwrap();
    let p = plan(vec![disk_row("library", &data_disk(690), "2026-10-11T18:01:00Z")], &held, now, &iso);
    assert_eq!(p.update.len(), 1);
}

#[test]
fn machine_prose_goes_and_other_writers_fields_stay() {
    let held = vec![Row {
        kind: "Machine",
        name: "library".into(),
        fields: vec![
            ("label".into(), "Library".into()),
            ("comment".into(), "M1 MacBook Air. Primary development machine.".into()),
            ("ownedBy".into(), "principal-silas".into()),
        ],
    }];
    let facts = MachineFacts { hostname: "jeffs-mac-mini-m1-3".into(), hardware_model: "Macmini9,1".into(), os_version: "26.6.2".into() };
    let p = plan(vec![machine_row("library", &facts, "2026-10-11T18:00:00Z")], &held, 0, &iso);
    let r = &p.update[0];
    assert_eq!(r.get("comment"), "");
    assert_eq!(r.get("label"), "Library");
    assert_eq!(r.get("ownedBy"), "principal-silas");
    assert_eq!(r.get("hostname"), "jeffs-mac-mini-m1-3");
}

#[test]
fn a_machine_the_door_does_not_hold_is_never_created() {
    let p = plan(vec![machine_row("attic", &MachineFacts::default(), "2026-10-11T18:00:00Z")], &[], 0, &iso);
    assert_eq!(p, Default::default());
}

#[test]
fn percent_matches_df_on_an_apfs_shared_container() {
    // Library 2026-10-11: df prints 2% for / and 67% for Data. Both volumes report
    // the whole container as capacity, so used / capacity would say 0% for /.
    let disks = parse_df(&fixture("df-library.txt"));
    let pct = |m: &str| disks.iter().find(|d| d.mount == m).unwrap().percent();
    assert_eq!((pct("/"), pct("/System/Volumes/Data")), (2, 67));
}
