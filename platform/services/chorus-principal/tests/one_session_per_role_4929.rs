// @domain: identity
// #4929 AC3 — the root sudo wrapper is not counted as a second live session.
// Measured 2026-11-08 06:36: silas-42982.json (the asuser wrapper, registered by
// chorus-principal attach) and silas-43168.json (claude itself) were both live,
// so Silas's next sign-in was refused as "2 live sessions" and nudges to him
// came back "logged out". The wrapper matched because its arguments say claude.
use chorus_principal::rows::{pane_process_is, PaneRuntime};

#[test]
fn claude_itself_is_the_session() {
    assert!(pane_process_is("/h/.local/bin/claude -c", PaneRuntime::Claude));
    assert!(pane_process_is("claude", PaneRuntime::Claude));
}

#[test]
fn negative_proof_the_asuser_wrapper_is_not_the_session() {
    let wrapper = "sudo -n /bin/launchctl asuser 502 /usr/bin/sudo -n -u chorus-silas -H bash -c \"umask 002 && cd '/x/roles/silas' && exec /h/.local/bin/claude -c\"";
    assert!(!pane_process_is(wrapper, PaneRuntime::Claude), "the root wrapper must not be registered as Silas's session");
    assert!(!pane_process_is("bash -c umask 002 && exec claude -c", PaneRuntime::Claude));
    assert!(!pane_process_is("/bin/launchctl asuser 502 /usr/bin/sudo -n -u chorus-silas -H bash -c claude", PaneRuntime::Claude));
}
