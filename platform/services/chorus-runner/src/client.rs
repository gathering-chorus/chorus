//! #4514 AC4 — `chorus-runner call <tool> [key=value…]`: one tool call, run
//! in-process by the runner's own code (Silas 19:11: no server, so a card that
//! changes the runner runs its own build from the werk bin slot, and no run
//! depends on a runner that is not deployed yet). The caller is $ROLE; the
//! verb's output streams to the step's log and its exit code is the step's.

use serde_json::{json, Value};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// `key=value` pairs as tool arguments, as text; the runner turns each into
/// the type its schema declares. A key given twice becomes a list, in order.
/// A pair without `=` is refused.
pub fn args_from_pairs(pairs: &[String]) -> Result<Value, String> {
    let mut m = serde_json::Map::new();
    for p in pairs {
        let (k, v) = p.split_once('=').ok_or_else(|| format!("{p}: arguments are key=value"))?;
        match m.get_mut(k) {
            Some(Value::Array(a)) => a.push(json!(v)),
            Some(prev) => *prev = json!([prev.clone(), v]),
            None => {
                m.insert(k.to_string(), json!(v));
            }
        }
    }
    Ok(Value::Object(m))
}

/// The whole call; returns the process exit code.
pub fn run(tool: &str, args: &Value) -> i32 {
    let role = std::env::var("ROLE").or_else(|_| std::env::var("CHORUS_ROLE")).unwrap_or_default();
    if role.is_empty() {
        eprintln!("chorus-runner call: no ROLE to run as");
        return 2;
    }
    // #4515 — athena-get/-list/-add/-set: the same check and the same door
    // call as the server, as $ROLE with a credential minted for $ROLE
    if crate::is_athena_tool(tool) {
        let cols = std::sync::Arc::new(std::sync::RwLock::new(Vec::new()));
        crate::server::refresh_collections(&cols);
        let cols = cols.read().map(|c| c.clone()).unwrap_or_default();
        let map = args.as_object().cloned().unwrap_or_default();
        let (ok, text, _) = crate::server::run_athena(&crate::server::door_base(), tool, &map, &cols, &role, None, &crate::new_trace_id());
        if ok { println!("{text}") } else { eprintln!("chorus-runner call: {text}") }
        return if ok { 0 } else { 1 };
    }
    let tools = match crate::server::load_skill_tools() {
        Ok(t) => t,
        Err(e) => {
            eprintln!("chorus-runner call: {e}");
            return 2;
        }
    };
    let Some(t) = tools.iter().find(|t| t.name == tool) else {
        eprintln!("chorus-runner call: no tool {tool} (a Skill row needs an input shape and runsAs to be one)");
        return 2;
    };
    let map = args.as_object().cloned().unwrap_or_default();
    let trace = crate::new_trace_id();
    let p = match crate::server::prepare(t, &map, &role, &trace) {
        Ok(p) => p,
        Err((e, _)) => {
            eprintln!("chorus-runner call: {e}");
            return 1;
        }
    };
    if let Some((method, url, body, token)) = &p.http {
        let (ok, text, _) = crate::server::run_http(method, url, body, token, &trace, t.timeout_s);
        println!("{text}");
        return if ok { 0 } else { 1 };
    }
    eprintln!("chorus-runner call: {tool} as {} → {} {}", p.who, p.binary, p.argv.join(" "));
    let mut child = match Command::new(&p.binary).args(&p.argv).envs(p.env.iter().map(|(k, v)| (*k, v.as_str()))).stdin(Stdio::null()).spawn() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("chorus-runner call: {} did not start: {e}", p.binary);
            return 1;
        }
    };
    // a long verb keeps its own deadlines (Kade's review); a short one gets the row's
    let deadline = (!t.long_running).then(|| Instant::now() + Duration::from_secs(t.timeout_s));
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.code().unwrap_or(1),
            Ok(None) => {}
            Err(e) => {
                eprintln!("chorus-runner call: {e}");
                return 1;
            }
        }
        if deadline.is_some_and(|d| Instant::now() > d) {
            let _ = child.kill();
            eprintln!("chorus-runner call: {tool} stopped after {}s (the row's timeout)", t.timeout_s);
            return 124;
        }
        std::thread::sleep(Duration::from_millis(200));
    }
}
