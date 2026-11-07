// @test-type: unit — builds the gate command; one real `sudo -n` to an account that does not exist.
// @domain: cicd
// @card: #4921
//! #4921 (Jeff 12:53: "run as one of u / why am i needed"): a gate reviewer's claude
//! runs as the card's role account, under that role's login, never as the runner.
//! At 12:48 the runner's (jeffbridwell's) claude login had expired and every demo crashed.
use werk_demo::{gate_command, review_once};

const BIN: &str = "/Users/jeffbridwell/.local/bin/claude";

#[test]
fn from_the_runner_the_gate_hops_to_the_role_account() {
    let (prog, args, who) = gate_command("jeffbridwell", "kade", BIN, "m", "it's the prompt");
    assert_eq!(prog, "sudo");
    assert_eq!(&args[..6], &["-n", "-u", "chorus-kade", "-H", "bash", "-c"]);
    assert_eq!(who, "claude as chorus-kade");
    let inner = &args[6];
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
        let (prog, args, _) = gate_command("jeffbridwell", role, BIN, "m", "s");
        assert_ne!(prog, BIN, "{role}: claude would run as the runner");
        assert!(!args.iter().any(|a| a == "-p"), "{role}: claude args outside the hop");
    }
}

#[test]
fn already_the_role_account_runs_claude_directly() {
    let (prog, args, who) = gate_command("chorus-kade", "kade", BIN, "m", "s");
    assert_eq!(prog, BIN);
    assert_eq!(args[0], "-p");
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
