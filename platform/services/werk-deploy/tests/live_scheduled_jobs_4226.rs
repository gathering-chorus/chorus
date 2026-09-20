//! #4226 — the same check, against this machine's real launchd jobs.
//!
//! The fixtures beside this file were captured by hand and can drift from what
//! launchctl actually prints. This asks launchd directly: find the scheduled
//! jobs that are loaded right now, and confirm the up-check answers correctly
//! for the ones whose state we can read off their own output. If nothing is
//! loaded — another machine, a fresh account — the test says UNMEASURED and
//! does not claim a green it did not earn.
use std::process::Command;
use werk_deploy::{deploy_took, job_kind, JobKind};

fn print_job(svc: &str) -> Option<String> {
    let uid = Command::new("id").arg("-u").output().ok()?;
    let uid = String::from_utf8_lossy(&uid.stdout).trim().to_string();
    let out = Command::new("launchctl")
        .args(["print", &format!("gui/{}/{}", uid, svc)])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&out.stdout).to_string())
}

fn last_exit(print_out: &str) -> Option<String> {
    print_out
        .lines()
        .find(|l| l.trim_start().starts_with("last exit code ="))
        .and_then(|l| l.split('=').nth(1))
        .map(|v| v.trim().to_string())
}

#[test]
fn the_up_check_agrees_with_this_machines_scheduled_jobs() {
    // Every com.chorus.* LaunchAgent that is a calendar/interval job.
    let candidates = [
        "com.chorus.tmp-reaper",
        "com.chorus.launchagent-metrics",
        "com.chorus.seed-probe",
        "com.chorus.cruft-scan",
        "com.chorus.heartbeat",
        "com.chorus.athena-validate",
        "com.chorus.crawl-nightly",
        "com.chorus.nightly-suites",
    ];
    let mut measured = 0;
    let mut saw_pass = false;
    let mut saw_fail = false;
    for svc in candidates {
        let Some(out) = print_job(svc) else { continue };
        if job_kind(&out) != JobKind::Scheduled {
            continue;
        }
        measured += 1;
        let exit = last_exit(&out).unwrap_or_else(|| "none".into());
        let running = out.contains("state = running");
        let took = deploy_took(&out);
        println!("{svc}: running={running} last_exit={exit} deploy_took={took}");
        // The rule, restated against live output: a scheduled job passes when
        // it is running, or when its last run exited 0, or when it has not run.
        let want = running || exit == "0" || exit == "(never exited)" || exit == "none";
        assert_eq!(took, want, "{svc}: up-check disagrees with its own launchctl output");
        if took { saw_pass = true } else { saw_fail = true }
    }
    if measured == 0 {
        println!("UNMEASURED: no scheduled com.chorus.* job is loaded on this machine");
        return;
    }
    // A run that only ever saw one answer proves nothing about the other.
    println!("measured {measured} live scheduled job(s); pass_seen={saw_pass} fail_seen={saw_fail}");
    assert!(saw_pass, "no live scheduled job passed — the check may refuse everything");
}
