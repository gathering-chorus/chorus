// @test-type: unit
// @domain: pipelines
//! #4599 — a pipeline run on a werk another user owns gets past git, because the
//! run's env trusts the card's own werk. Abby's werks are uid 505 and dagu runs
//! as 501, so every git call stopped with "dubious ownership" until Jeff added
//! the werk to his own config by hand. The env trusts that one werk and no other.
//!
//! git's own test switch GIT_TEST_ASSUME_DIFFERENT_OWNER stands in for the
//! other owner, so the proof runs without a second user.
use chorus_make::{generate, generate_straight, parse_pipe, parse_rows};
use std::process::Command;

fn fixture(name: &str) -> String {
    let dir = std::env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR");
    std::fs::read_to_string(format!("{dir}/tests/fixtures/{name}")).expect("fixture")
}

/// The three git lines of a generated pipeline's env, with the run's values in.
fn trust_env(yaml: &str, base: &str, role: &str, card: &str) -> Vec<(String, String)> {
    yaml.lines()
        .filter_map(|l| l.strip_prefix("  - GIT_CONFIG_"))
        .map(|l| {
            let (k, v) = l.split_once(": ").expect("key: value");
            let v = v.replace("${CHORUS_WERK_BASE}", base).replace("${ROLE}", role).replace("${CARD}", card);
            (format!("GIT_CONFIG_{k}"), v)
        })
        .collect()
}

fn generated() -> Vec<(&'static str, String)> {
    let known: Vec<String> = ["cicd", "cicd-test", "nightly", "athena"].iter().map(|s| s.to_string()).collect();
    vec![
        ("cicd", generate("cicd", &parse_rows(&fixture("cicd-rows.tsv")).unwrap()).unwrap()),
        ("cicd-test", generate_straight("cicd-test", &parse_pipe(&fixture("cicd-test-pipe.tsv")).unwrap(),
            &parse_rows(&fixture("cicd-test-rows.tsv")).unwrap(), &known).unwrap()),
    ]
}

/// rev-parse in `repo` as if another user owned it, under `env`.
fn git_opens(repo: &std::path::Path, env: &[(String, String)]) -> Result<(), String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo).args(["rev-parse", "--git-dir"])
        .env("GIT_TEST_ASSUME_DIFFERENT_OWNER", "1")
        .env_remove("GIT_CONFIG_COUNT")
        .env_remove("GIT_CONFIG_KEY_0")
        .env_remove("GIT_CONFIG_VALUE_0")
        .envs(env.iter().map(|(k, v)| (k.as_str(), v.as_str())));
    let out = cmd.output().expect("git runs");
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).into_owned()) }
}

fn werks() -> (tempfile_dir::Dir, std::path::PathBuf, std::path::PathBuf) {
    let d = tempfile_dir::Dir::new();
    let own = d.path.join("abby-normal-4598");
    let other = d.path.join("kade-4579");
    for w in [&own, &other] {
        std::fs::create_dir_all(w).unwrap();
        let o = Command::new("git").arg("-C").arg(w).args(["init", "-q"]).output().unwrap();
        assert!(o.status.success(), "git init {}: {}", w.display(), String::from_utf8_lossy(&o.stderr));
    }
    (d, own, other)
}

#[test]
fn a_run_opens_its_own_werk_when_another_user_owns_it() {
    let (d, own, _) = werks();
    for (name, yaml) in generated() {
        let env = trust_env(&yaml, d.path.to_str().unwrap(), "abby-normal", "4598");
        assert_eq!(env.len(), 3, "{name}: the env names GIT_CONFIG_COUNT, KEY_0 and VALUE_0\n{yaml}");
        git_opens(&own, &env).unwrap_or_else(|e| panic!("{name}: git refused the card's own werk: {e}"));
    }
}

/// NEGATIVE PROOF: with no trust line git refuses the werk (the state this card
/// fixes), and the same env never opens another role's werk.
#[test]
fn negative_proof_without_the_env_or_on_another_werk_git_refuses() {
    let (d, own, other) = werks();
    let e = git_opens(&own, &[]).expect_err("no env: git must refuse a werk another user owns");
    assert!(e.contains("dubious ownership"), "{e}");
    for (name, yaml) in generated() {
        let env = trust_env(&yaml, d.path.to_str().unwrap(), "abby-normal", "4598");
        let e = git_opens(&other, &env).expect_err(&format!("{name}: trusting 4598 must not open kade-4579"));
        assert!(e.contains("dubious ownership"), "{name}: {e}");
    }
}

/// A temp dir under the target dir, removed on drop (no tempfile crate here).
mod tempfile_dir {
    pub struct Dir { pub path: std::path::PathBuf }
    impl Dir {
        pub fn new() -> Self {
            // the two tests run at once and the clock can give both one value
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            let path = std::env::temp_dir().join(format!("git-trust-4599-{}-{n}", std::process::id()));
            std::fs::create_dir_all(&path).unwrap();
            // git compares the resolved path: /var/folders is a link to /private/var
            Dir { path: path.canonicalize().unwrap() }
        }
    }
    impl Drop for Dir {
        fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.path); }
    }
}
