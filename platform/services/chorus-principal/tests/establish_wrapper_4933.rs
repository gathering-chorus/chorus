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
use chorus_principal::own_account_inner_cmd;
use std::{fs, process::Command};

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
    let (d, cmd) = world(0);
    let out = Command::new("bash").args(["-c", &cmd]).output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    assert_eq!(fs::read_to_string(d.path("est.args")).unwrap().trim(), "establish fx --run 18f2a");
    assert_eq!(fs::read_to_string(d.path("started")).unwrap().trim(), "fx-run-1");
}

#[test]
fn the_login_trace_crosses_into_the_role_shell_and_quotes_cannot_escape() {
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
    std::env::set_var("CHORUS_STARTED_BY", "jeff");
    let (d, cmd) = world(0);
    std::env::remove_var("CHORUS_STARTED_BY");
    let probe = cmd.replace("&& export CHORUS_RUN &&", &format!("&& export CHORUS_RUN && echo \"$CHORUS_STARTED_BY\" > '{}' &&", d.path("by")));
    assert!(Command::new("bash").args(["-c", &probe]).status().unwrap().success());
    assert_eq!(fs::read_to_string(d.path("by")).unwrap().trim(), "jeff");
}

#[test]
fn negative_proof_establish_refused_means_the_runtime_never_starts() {
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
