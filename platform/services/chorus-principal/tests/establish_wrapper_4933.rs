// @test-type: integration — runs the generated wrapper in bash with a stub establish
// @domain: identity
// #4933 AC3 — one wrapper for every runtime (Silas 16:42, Abby's A1/A3): the role's
// own shell runs `chorus-principal establish <role> --run` first, as the role, and
// only then execs the runtime with the run it established (CHORUS_RUN). Fail
// closed: a refused or failed establish exits with its reason and the runtime
// never starts (Jeff's no-fallback rule).
//
// The wrapper string is executed for real here with a stub establish and a stub
// runtime; fixture role `fx` only.
use chorus_principal::lifecycle::establish_refusal;
use chorus_principal::own_account_inner_cmd;
use std::{fs, process::Command, sync::Mutex};

// cases that set CHORUS_TRACE_ID / CHORUS_STARTED_BY run one at a time: the wrapper
// reads process env, and cargo runs tests in parallel threads (review 19:32, note 8)
static ENV: Mutex<()> = Mutex::new(());
fn env_lock() -> std::sync::MutexGuard<'static, ()> { ENV.lock().unwrap_or_else(|p| p.into_inner()) }

fn world(establish_exit: i32) -> (tempdir::Dir, String) {
    let d = tempdir::Dir::new();
    let root = d.path("root");
    fs::create_dir_all(format!("{root}/platform/scripts")).unwrap();
    fs::write(format!("{root}/platform/scripts/chorus-env-setup.sh"), "").unwrap();
    fs::write(format!("{root}/platform/scripts/role-post"), "exit 0\n").unwrap();
    let est = d.path("establish");
    fs::write(&est, format!("#!/bin/bash\necho \"$*\" > '{}'\n[ {establish_exit} -eq 0 ] && echo fx-run-1 && exit 0\necho 'REFUSED — establish fx runs as chorus-fx' >&2\nexit {establish_exit}\n", d.path("est.args"))).unwrap();
    Command::new("chmod").args(["+x", &est]).status().unwrap();
    let runtime = format!("echo \"$CHORUS_RUN\" > '{}'", d.path("started"));
    let role_dir = d.path("roles/fx");
    fs::create_dir_all(&role_dir).unwrap();
    let cmd = own_account_inner_cmd(&role_dir, &root, &d.path("id"), &d.path("id/fx/token.cache"), &d.path("sessions"), "fx", &est, "18f2a", &runtime);
    (d, cmd)
}

#[test]
fn the_runtime_starts_only_after_establish_and_carries_its_run() {
    let _g = env_lock();
    let (d, cmd) = world(0);
    let out = Command::new("bash").args(["-c", &cmd]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    // the pane and tty placeholders are filled by the pane's outer shell (launch_line);
    // run directly here, they arrive as written and establish ignores them
    assert_eq!(fs::read_to_string(d.path("est.args")).unwrap().trim(), "establish fx --run 18f2a --pane __CHORUS_PANE__ --tty __CHORUS_TTY__");
    assert_eq!(fs::read_to_string(d.path("started")).unwrap().trim(), "fx-run-1");
}

#[test]
fn the_login_trace_crosses_into_the_role_shell_and_quotes_cannot_escape() {
    let _g = env_lock();
    std::env::set_var("CHORUS_TRACE_ID", "login-on-fx-17-1';touch /tmp/x;'");
    let (d, cmd) = world(0);
    std::env::remove_var("CHORUS_TRACE_ID");
    let probe = cmd.replace("&& export CHORUS_RUN &&", &format!("&& export CHORUS_RUN && echo \"$CHORUS_TRACE_ID\" > '{}' &&", d.path("trace")));
    let out = Command::new("bash").args(["-c", &probe]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(fs::read_to_string(d.path("trace")).unwrap().trim(), "login-on-fx-17-1touchtmpx");
}

#[test]
fn who_started_it_crosses_into_the_role_shell() {
    let _g = env_lock();
    std::env::set_var("CHORUS_STARTED_BY", "jeff");
    let (d, cmd) = world(0);
    std::env::remove_var("CHORUS_STARTED_BY");
    let probe = cmd.replace("&& export CHORUS_RUN &&", &format!("&& export CHORUS_RUN && echo \"$CHORUS_STARTED_BY\" > '{}' &&", d.path("by")));
    assert!(Command::new("bash").args(["-c", &probe]).status().unwrap().success());
    assert_eq!(fs::read_to_string(d.path("by")).unwrap().trim(), "jeff");
}

#[test]
fn negative_proof_establish_refused_means_the_runtime_never_starts() {
    // the WRAPPER half: whatever establish refuses for, the runtime never starts. The
    // refusal itself (only the role's own account) is establish_refusal, proven below
    // and through the binary in launch_hop_writes_nothing_4933.bats case 3.
    let _g = env_lock();
    let (d, cmd) = world(2);
    let out = Command::new("bash").args(["-c", &cmd]).output().unwrap();
    assert!(!out.status.success());
    assert!(!std::path::Path::new(&d.path("started")).exists(), "the runtime started after a refused establish");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("REFUSED") && err.contains("not starting fx"), "the reason is shown: {err}");
}

mod tempdir {
    pub struct Dir(std::path::PathBuf);
    impl Dir {
        pub fn new() -> Dir {
            let p = std::env::temp_dir().join(format!("est4933-{}-{}", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
            std::fs::create_dir_all(&p).unwrap();
            Dir(p)
        }
        pub fn path(&self, s: &str) -> String { self.0.join(s).to_string_lossy().to_string() }
    }
    impl Drop for Dir { fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.0); } }
}

#[test]
fn a_cold_start_keeps_why_it_is_pending() {
    use chorus_principal::lifecycle::{keep_launch_reason, LoginState};
    let launch = LoginState::Pending { why: "identity not answering".into(), pid: None };
    let back = LoginState::Pending { why: "fx's run fx-run-1 is not established (HTTP 000)".into(), pid: Some(7) };
    match keep_launch_reason(&launch, back) {
        LoginState::Pending { why, pid } => { assert!(why.starts_with("identity not answering"), "{why}"); assert_eq!(pid, Some(7)); }
        other => panic!("{other:?}"),
    }
    // a recorded read-back wins: the role did establish
    let rec = LoginState::Recorded { session: "s".into(), pid: Some(7) };
    assert_eq!(keep_launch_reason(&launch, rec.clone()), rec);
}

// --- review 19:32, item 7: the account gate itself ---

#[test]
fn establish_is_allowed_only_for_the_account_the_principal_row_names() {
    assert_eq!(establish_refusal("fx", "chorus-fx", "chorus-fx"), None);
}

#[test]
fn negative_proof_any_other_account_is_refused_and_named() {
    let r = establish_refusal("fx", "chorus-fx", "jeffbridwell").expect("the launcher's account is refused");
    assert!(r.contains("runs as chorus-fx") && r.contains("jeffbridwell"), "{r}");
    assert!(establish_refusal("fx", "chorus-fx", "chorus-kade").is_some(), "another role's account is refused");
}

#[test]
fn negative_proof_an_unknown_account_is_refused_even_against_an_empty_row() {
    // `!me.is_empty()` matters: an empty id must never match an empty hostAccount
    assert!(establish_refusal("fx", "", "").is_some());
    assert!(establish_refusal("fx", "chorus-fx", "").is_some());
}

#[test]
fn the_wrapper_hands_establish_the_panes_own_id_and_tty_from_outside_sudo() {
    let _g = env_lock();
    let cmd = own_account_inner_cmd("/r/roles/fx", "/r", "/h/id", "/h/id/fx/token.cache", "/s", "fx", "/b/chorus-principal", "1a2b", "true");
    assert!(cmd.contains("--pane '__CHORUS_PANE__' --tty '__CHORUS_TTY__'"), "{cmd}");
}
