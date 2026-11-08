// @domain: identity
// #4929 review (run 3) — sudo/launchctl/shell wrappers are never the session, for any runtime.
use chorus_principal::rows::is_launch_wrapper;
#[test]
fn wrappers_are_not_sessions() {
    assert!(is_launch_wrapper("sudo -n /bin/launchctl asuser 507 /usr/bin/sudo -n -u chorus-borg-queen -H bash -c agy"));
    assert!(is_launch_wrapper("/bin/launchctl asuser 507 /usr/bin/sudo -n -u chorus-borg-queen -H bash -c gemini"));
    assert!(is_launch_wrapper("bash -c umask 002 && exec gemini"));
}
#[test]
fn negative_proof_the_runtime_itself_is_not_a_wrapper() {
    assert!(!is_launch_wrapper("agy"));
    assert!(!is_launch_wrapper("/h/.local/bin/claude -c"));
    assert!(!is_launch_wrapper("node /opt/gemini/cli.js"));
}
