// @test-type: unit — builds the gate command; one real `sudo -n` to an account that does not exist.
// @domain: cicd
// @card: #4921, #4924
//! #4921 (Jeff 12:53: "run as one of u / why am i needed"): a gate reviewer's claude
//! runs as the card's role account, under that role's login, never as the runner.
//! At 12:48 the runner's (jeffbridwell's) claude login had expired and every demo crashed.
//! #4924 AC4 (Wren 20:08, Jeff 20:26): when the owner doesn't run Claude, gate reviews hop to the
//! card's reviewer (wren per Jeff decision, or CHORUS_REVIEWER).
use werk_demo::{review_once, role_hop};

const BIN: &str = "/Users/jeffbridwell/.local/bin/claude";

fn args(v: &[&str]) -> Vec<String> { v.iter().map(|s| s.to_string()).collect() }

#[test]
fn from_the_runner_the_gate_hops_to_the_role_account() {
    let (prog, a, who) = role_hop("jeffbridwell", "kade", BIN, &args(&["-p", "it's the prompt"]), Some("/w/kade-1"));
    assert_eq!(prog, "sudo");
    assert_eq!(&a[..6], &["-n", "-u", "chorus-kade", "-H", "bash", "-c"]);
    assert_eq!(who, "claude as chorus-kade");
    let inner = &a[6];
    assert!(inner.starts_with("cd '/w/kade-1' && "), "the breaks question and /code-review read the werk: {inner}");
    assert!(inner.contains(&format!("exec '{BIN}' '-p'")), "{inner}");
    assert!(inner.contains("'it'\\''s the prompt'"), "a quote in the prompt must survive the shell: {inner}");
    assert!(inner.contains("CHORUS_HEADLESS=1"), "sudo drops the env, so headless is set inside: {inner}");
    assert!(!inner.contains("claude.key") && !inner.contains("CLAUDE_CODE_OAUTH_TOKEN"), "the role's own sign-in, never the revoked key file (13:02): {inner}");
    let ok = std::process::Command::new("bash").args(["-n", "-c", inner]).status().unwrap();
    assert!(ok.success(), "the hop's shell line must parse");
}

#[test]
fn negative_proof_the_runner_never_runs_claude_bare() {
    for role in ["kade", "silas", "wren", "abby-normal"] {
        let (prog, a, _) = role_hop("jeffbridwell", role, BIN, &args(&["-p", "x"]), None);
        assert_ne!(prog, BIN, "{role}: claude would run as the runner");
        assert!(!a.iter().any(|x| x == "-p"), "{role}: claude args outside the hop");
    }
}

#[test]
fn already_the_role_account_runs_claude_directly() {
    let (prog, a, who) = role_hop("chorus-kade", "kade", BIN, &args(&["-p"]), Some("/w"));
    assert_eq!(prog, BIN);
    assert_eq!(a, args(&["-p"]));
    assert_eq!(who, "claude as chorus-kade");
}

#[test]
fn a_refused_hop_is_named_never_a_silent_fall_back() {
    std::env::remove_var("CHORUS_GATE_PROFILE");
    std::env::set_var("CHORUS_CLAUDE_BIN", "/bin/false");
    std::env::set_var("CHORUS_GATE_TIMEOUT_SECS", "20");
    let (verdict, finding) = review_once("sys", "ctx", "round-1", &[], "nosuch-4921");
    assert_eq!(verdict, "error");
    assert!(finding.starts_with("claude as chorus-nosuch-4921 exit"), "{finding}");
    assert!(finding.contains("sudo"), "the refusal names sudo's own reason: {finding}");
}

#[test]
fn non_claude_role_hops_to_reviewer() {
    std::env::remove_var("CHORUS_REVIEWER");
    let (prog, a, who) = role_hop("jeffbridwell", "borg-queen", BIN, &args(&["-p", "prompt"]), None);
    assert_eq!(prog, "sudo");
    assert_eq!(&a[..6], &["-n", "-u", "chorus-wren", "-H", "bash", "-c"]);
    assert_eq!(who, "claude as chorus-wren");
}

#[test]
fn non_claude_role_hops_to_reviewer_override() {
    std::env::set_var("CHORUS_REVIEWER", "silas");
    let (prog, a, who) = role_hop("jeffbridwell", "borg-queen", BIN, &args(&["-p", "prompt"]), None);
    std::env::remove_var("CHORUS_REVIEWER");
    assert_eq!(prog, "sudo");
    assert_eq!(&a[..6], &["-n", "-u", "chorus-silas", "-H", "bash", "-c"]);
    assert_eq!(who, "claude as chorus-silas");
}

#[test]
fn negative_proof_non_claude_role_never_hops_to_own_account() {
    // NEGATIVE PROOF (#3734): non-Claude roles (borg-queen, abby-normal) must never hop
    // to their own account (which lacks a Claude login and crashes the demo).
    for role in ["borg-queen", "abby-normal"] {
        let (_, a, who) = role_hop("jeffbridwell", role, BIN, &args(&["-p", "x"]), None);
        assert_ne!(who, format!("claude as chorus-{role}"), "{role} must not hop to own account");
        assert!(!a.contains(&format!("chorus-{role}")), "{role} account must not be sudo target");
    }
}

#[test]
fn safe_directory_is_scoped_to_werk_path_never_wildcard() {
    let werk_path = "/Users/jeffbridwell/CascadeProjects/chorus-werk/test-123";
    let (_, a, _) = role_hop("jeffbridwell", "borg-queen", BIN, &args(&["-p"]), Some(werk_path));
    let inner = &a[6];
    assert!(inner.contains(&format!("GIT_CONFIG_VALUE_0='{werk_path}'")), "must scope safe.directory to werk: {inner}");
    assert!(!inner.contains("GIT_CONFIG_VALUE_0='*'"), "must never use wildcard * for safe.directory: {inner}");

    // Negative proof: when no cwd is given, safe.directory is not set at all
    let (_, a_none, _) = role_hop("jeffbridwell", "borg-queen", BIN, &args(&["-p"]), None);
    let inner_none = &a_none[6];
    assert!(!inner_none.contains("safe.directory"), "without cwd, safe.directory must not be set: {inner_none}");
}

#[test]
fn negative_proof_other_repo_path_remains_refused_under_scoped_safe_directory() {
    let target = "/Users/jeffbridwell/CascadeProjects/chorus";
    // With safe.directory scoped strictly to a different path (/tmp)
    let output = std::process::Command::new("git")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_SYSTEM", "/dev/null")
        .env("GIT_CONFIG_COUNT", "1")
        .env("GIT_CONFIG_KEY_0", "safe.directory")
        .env("GIT_CONFIG_VALUE_0", "/tmp")
        .args(["-C", target, "status"])
        .output()
        .expect("git status execution");

    assert!(!output.status.success(), "git must fail on dubious ownership for un-scoped path");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("fatal: detected dubious ownership"), "must refuse dubious ownership: {stderr}");
}
