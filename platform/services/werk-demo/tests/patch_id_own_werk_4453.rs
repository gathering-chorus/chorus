//! #4453 — the presented patch-id comes from the presenting role's werk.
//! 2026-10-07 12:42 and 13:21: a leftover silas-4446 werk was hashed instead of
//! kade-4446 (a fixed wren, silas, kade order), so for three rounds the
//! presented patch-id matched no land and every go on #4446 refused.
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};
use werk_demo::patch_id_for_card;

fn git(dir: &Path, args: &[&str]) {
    let ok = Command::new("git").args(args).current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t").env("GIT_AUTHOR_EMAIL", "t@t")
        .env("GIT_COMMITTER_NAME", "t").env("GIT_COMMITTER_EMAIL", "t@t")
        .output().unwrap().status.success();
    assert!(ok, "git {:?} in {}", args, dir.display());
}

/// A werk: a clone of `origin` with one card commit writing `content`.
fn werk(base: &Path, origin: &Path, name: &str, content: &str) {
    let w = base.join(name);
    git(base, &["clone", "-q", origin.to_str().unwrap(), name]);
    fs::write(w.join("card.txt"), content).unwrap();
    git(&w, &["add", "."]);
    git(&w, &["commit", "-q", "-m", name]);
}

fn world() -> PathBuf {
    let n = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let root = std::env::temp_dir().join(format!("wd-4453-{}", n));
    let seed = root.join("seed");
    fs::create_dir_all(&seed).unwrap();
    git(&seed, &["init", "-q", "-b", "main"]);
    fs::write(seed.join("README"), "x").unwrap();
    git(&seed, &["add", "."]);
    git(&seed, &["commit", "-q", "-m", "base"]);
    git(&root, &["clone", "-q", "--bare", "seed", "origin.git"]);
    fs::create_dir_all(root.join("werks")).unwrap();
    root
}

#[test]
fn the_presenting_roles_werk_wins_over_a_peers_leftover() {
    let root = world();
    let (origin, base) = (root.join("origin.git"), root.join("werks"));
    werk(&base, &origin, "silas-4446", "a peer's leftover change");
    werk(&base, &origin, "kade-4446", "the change kade presents");
    let b = base.to_str().unwrap();
    let kade = patch_id_for_card(b, "kade", 4446);
    let silas = patch_id_for_card(b, "silas", 4446);
    assert!(!kade.is_empty() && !silas.is_empty(), "both werks hash");
    // NEGATIVE PROOF: the old fixed order (wren, silas, kade) returned
    // silas's hash for kade too, and this goes red on it
    assert_ne!(kade, silas, "kade's present hashed silas's leftover werk");
}

#[test]
fn with_no_own_werk_a_peer_presenting_on_the_card_still_finds_it() {
    let root = world();
    let (origin, base) = (root.join("origin.git"), root.join("werks"));
    werk(&base, &origin, "kade-4447", "kade's change");
    let b = base.to_str().unwrap();
    assert_eq!(patch_id_for_card(b, "wren", 4447), patch_id_for_card(b, "kade", 4447));
    assert!(!patch_id_for_card(b, "wren", 4447).is_empty());
}
