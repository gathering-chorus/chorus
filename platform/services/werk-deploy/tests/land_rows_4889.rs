// @test-type: unit — a fixture canonical tree and a stand-in rows script; no door.
// @domain: builds
// @card: #4889
//! Kade 20:07: the card's store writes run in the land after Jeff's go, one touch.
use std::path::PathBuf;
use werk_deploy::{land_rows_script, run_land_rows};

fn tree(tag: &str, body: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("wd-rows-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(d.join("platform/scripts")).unwrap();
    std::fs::write(d.join("platform/scripts/land-4889-rows"), body).unwrap();
    d
}

#[test]
fn the_cards_own_script_runs_with_apply_when_the_land_ships_it() {
    let d = tree("ok", "import sys\nprint('plan: applied' if '--apply' in sys.argv else 'dry')\n");
    let s = land_rows_script(&d, 4889, "a.rs\nplatform/scripts/land-4889-rows\n").expect("shipped");
    assert!(run_land_rows(&s).unwrap().contains("plan: applied"));
}

#[test]
fn negative_proof_a_script_this_land_does_not_ship_never_runs() {
    let d = tree("old", "print('x')\n");
    assert!(land_rows_script(&d, 4889, "a.rs\n").is_none(), "an old card's script on main stays put");
    assert!(land_rows_script(&d, 4890, "platform/scripts/land-4889-rows\n").is_none(), "another card's script");
}

#[test]
fn negative_proof_a_refused_write_stops_the_land_naming_why() {
    let d = tree("refused", "print('  REFUSED stepskills/x')\nprint('plan: REFUSED 1 of 3')\nraise SystemExit(1)\n");
    let s = land_rows_script(&d, 4889, "platform/scripts/land-4889-rows").unwrap();
    let e = run_land_rows(&s).unwrap_err().to_string();
    assert!(e.contains("refused (exit Some(1))") && e.contains("REFUSED 1 of 3"), "{e}");
}
