//! athena-serve — the Athena pipeline's SERVE leg (#4186).
//!
//! Jeff, 2026-09-16: "right now athena and werk are conjoined twins we need to
//! separate them". The land used to restart athena-make from inside
//! werk-deploy and seed rows against it before it answered: POST → 0 on six
//! lands across #4175 and #4179 (launchd said "running", the socket said
//! nothing). This verb is the one place that question is asked, and it is
//! asked of the HEALTH endpoint, never of launchd:
//!
//!   athena-serve <launchd label> <health url> [--kickstart] [--timeout <s>]
//!
//! --kickstart: `launchctl kickstart -k gui/<uid>/<label>` first (a model
//! change is only served after a restart; athena-make reads shapes at boot).
//! Then poll <health url> until it answers 200 with a healthy body, or refuse
//! after --timeout seconds (default 60) naming the last answer. Exit 0 only
//! when it answered. Nothing downstream (the seed) may run on any other exit.
//!
//! Test seams (#3528, a test brings its own world): LAUNCHCTL_BIN, CURL_BIN,
//! ATHENA_SERVE_POLL_MS, CHORUS_LOG (spine emitter; absent = no emit).

use std::env;
use std::process::Command;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub label: String,
    pub health_url: String,
    pub kickstart: bool,
    pub timeout_s: u64,
}

pub fn parse_args(args: &[String]) -> Result<Args, String> {
    let usage = "usage: athena-serve <launchd-label> <health-url> [--kickstart] [--timeout <seconds>]";
    let mut pos: Vec<String> = Vec::new();
    let mut kickstart = false;
    let mut timeout_s = 60u64;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--kickstart" => kickstart = true,
            "--timeout" => {
                i += 1;
                timeout_s = args.get(i).and_then(|s| s.parse().ok()).ok_or_else(|| format!("--timeout needs seconds\n{usage}"))?;
            }
            other if other.starts_with("--") => return Err(format!("unknown flag {other}\n{usage}")),
            other => pos.push(other.to_string()),
        }
        i += 1;
    }
    if pos.len() != 2 { return Err(usage.to_string()); }
    Ok(Args { label: pos[0].clone(), health_url: pos[1].clone(), kickstart, timeout_s })
}

/// One health probe's answer: HTTP code + body. Pure verdict: 200 AND a
/// body that says healthy. "ok"/"healthy" buried elsewhere in the body does
/// not count (the 6.401ms substring class, werk-deploy health_body_ok).
pub fn healthy(code: &str, body: &str) -> bool {
    if code.trim() != "200" { return false; }
    let b = body.replace(' ', "").replace('\n', "");
    // chorus-api / pulse / clearing answer {"status":"healthy"|"ok"}; athena-make
    // answers {"ok":true,"service":"athena-make"} (first live run of #4186 refused
    // a healthy variant for 90s over exactly this). Both are a status field at the
    // top of the body, never a word buried in another field.
    b.contains("\"status\":\"healthy\"") || b.contains("\"status\":\"ok\"") || b.starts_with("{\"ok\":true")
}

/// The refusal line: what was polled, how long, and the LAST answer, so the
/// reader knows whether the service is down (000), up-but-sick (503) or
/// answering something this verb does not recognise as healthy (200 + body).
pub fn refusal(url: &str, waited_s: u64, last_code: &str, last_body: &str) -> String {
    let body: String = last_body.chars().take(120).collect();
    format!("athena-serve: REFUSED — {url} did not answer healthy within {waited_s}s (last: HTTP {last_code} {body:?}). The model may be in the store but it is NOT served; nothing downstream may run.")
}

fn envd(k: &str, d: &str) -> String { env::var(k).unwrap_or_else(|_| d.to_string()) }

fn emit(event: &str, kv: &[(&str, &str)]) {
    let Ok(log) = env::var("CHORUS_LOG") else { return };
    if log.is_empty() { return; }
    let role = env::var("DEPLOY_ROLE").or_else(|_| env::var("CHORUS_ROLE")).unwrap_or_else(|_| "system".into());
    let mut a: Vec<String> = vec![event.to_string(), role];
    for (k, v) in kv { a.push(format!("{k}={v}")); }
    let _ = Command::new("bash").arg(&log).args(&a).output();
}

fn probe(curl: &str, url: &str) -> (String, String) {
    let out = Command::new(curl).args(["-s", "-m", "5", "-w", "\n%{http_code}", url]).output();
    match out {
        Ok(o) => {
            let s = String::from_utf8_lossy(&o.stdout).to_string();
            let (body, code) = s.rsplit_once('\n').unwrap_or(("", "000"));
            (code.trim().to_string(), body.to_string())
        }
        Err(e) => ("000".into(), format!("curl failed: {e}")),
    }
}

pub fn run(argv: &[String]) -> i32 {
    let a = match parse_args(argv) { Ok(a) => a, Err(e) => { eprintln!("{e}"); return 2; } };
    let launchctl = envd("LAUNCHCTL_BIN", "launchctl");
    let curl = envd("CURL_BIN", "curl");
    let poll_ms: u64 = envd("ATHENA_SERVE_POLL_MS", "1000").parse().unwrap_or(1000);

    if a.kickstart {
        let uid = Command::new("id").arg("-u").output().map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string()).unwrap_or_else(|_| "501".into());
        let target = format!("gui/{uid}/{}", a.label);
        match Command::new(&launchctl).args(["kickstart", "-k", &target]).output() {
            Ok(o) if o.status.success() => emit("athena.serve.kickstarted", &[("service", &a.label)]),
            Ok(o) => {
                let err = String::from_utf8_lossy(&o.stderr).trim().to_string();
                eprintln!("athena-serve: REFUSED — launchctl kickstart -k {target} failed: {err}");
                emit("athena.serve.refused", &[("service", &a.label), ("reason", "kickstart-failed")]);
                return 1;
            }
            Err(e) => { eprintln!("athena-serve: REFUSED — cannot run {launchctl}: {e}"); return 1; }
        }
    }

    let start = Instant::now();
    let (code, body): (String, String);
    loop {
        let (c, b) = probe(&curl, &a.health_url);
        if healthy(&c, &b) {
            let waited = start.elapsed().as_secs();
            println!("athena-serve: {} answers healthy at {} after {}s", a.label, a.health_url, waited);
            emit("athena.serve.ready", &[("service", &a.label), ("url", &a.health_url), ("waited_s", &waited.to_string())]);
            return 0;
        }
        if start.elapsed() >= Duration::from_secs(a.timeout_s) { code = c; body = b; break; }
        std::thread::sleep(Duration::from_millis(poll_ms));
    }
    let line = refusal(&a.health_url, a.timeout_s, &code, &body);
    eprintln!("{line}");
    emit("athena.serve.refused", &[("service", &a.label), ("url", &a.health_url), ("last_http", &code)]);
    1
}
