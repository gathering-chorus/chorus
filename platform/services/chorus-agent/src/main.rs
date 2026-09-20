use chorus_agent::{config, contract::*, execution, Result};
use serde_json::{json, Value};
use std::io::Read;

async fn request(method: &str, path: &str, body: Option<Value>) -> Result<Value> {
    let client = reqwest::Client::builder()
        .unix_socket(config::socket())
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;
    let mut req = client.request(method.parse().unwrap(), format!("http://localhost{path}"));
    if let Some(body) = body {
        req = req.json(&body);
    }
    let response = req
        .send()
        .await
        .map_err(|_| "agent supervisor unavailable")?;
    let status = response.status();
    let body: Value = response
        .json()
        .await
        .map_err(|_| "invalid supervisor response")?;
    if !status.is_success() {
        return Err(body["error"]["message"]
            .as_str()
            .unwrap_or("supervisor refused request")
            .into());
    }
    Ok(body)
}

async fn launch(args: &[String]) -> Result<Value> {
    let role = args.get(1).ok_or("launch requires role")?;
    let c = config::read(&config::config_path())?;
    let mut options = std::collections::BTreeMap::new();
    for pair in args[2..].chunks(2) {
        if pair.len() != 2
            || !["--profile", "--cwd", "--native-session-id"].contains(&pair[0].as_str())
        {
            return Err("launch accepts --profile, --cwd, --native-session-id with values".into());
        }
        options.insert(pair[0].as_str(), pair[1].as_str());
    }
    let profile = options
        .get("--profile")
        .map(|s| s.to_string())
        .or_else(|| std::env::var("CHORUS_AGENT_PROFILE").ok())
        .or_else(|| c.roles.get(role).cloned())
        .ok_or("no role runtime profile configured")?;
    let p = c.profiles.get(&profile).ok_or("unknown role profile")?;
    let cwd = options
        .get("--cwd")
        .map(|s| s.to_string())
        .or_else(|| c.role_workspaces.get(role).cloned())
        .ok_or("launch requires --cwd or explicit role_workspaces binding")?;
    let token = std::env::var("CHORUS_SESSION_TOKEN_FILE")
        .map_err(|_| "CHORUS_SESSION_TOKEN_FILE is required for runtime launch")?;
    let native_id = options.get("--native-session-id");
    if native_id.is_some()
        && p.mode == Mode::Native
        && !matches!(p.runtime, Runtime::Claude | Runtime::Codex)
    {
        return Err("native resume for this adapter must use its documented client UI then register the explicit conversation id".into());
    }
    let session=request("POST","/v1/sessions",Some(json!({"version":VERSION,"profile":profile,"role":role,"cwd":cwd,"credential_file":token,"primary":true,"native_session_id":native_id}))).await?;
    if p.mode == Mode::Managed {
        return Ok(session);
    }
    native_client(p, session, &token).await
}
async fn native_client(p: &Profile, session: Value, token: &str) -> Result<Value> {
    let id = session["session_id"]
        .as_str()
        .ok_or("invalid session response")?;
    let role = session["role"].as_str().ok_or("invalid session role")?;
    let cwd = session["cwd"].as_str().ok_or("invalid session cwd")?;
    let mut command = std::process::Command::new(execution::executable(p));
    if let Some(native) = session["native_session_id"].as_str() {
        match p.runtime {
            Runtime::Claude => {
                command.args(["--resume", native]);
            }
            Runtime::Codex => {
                command.args(["resume", native]);
            }
            _ => {
                return Err("resume this native runtime in its UI; then register its explicit conversation id".into());
            }
        }
    }
    if let Some(model) = session["model"].as_str() {
        command.args(["--model", model]);
    }
    let status = command
        .current_dir(cwd)
        .env("CHORUS_SESSION_ID", id)
        .env("CHORUS_ROLE", role)
        .env("DEPLOY_ROLE", role)
        .env("CHORUS_SESSION_TOKEN_FILE", token)
        .env("CHORUS_MCP_IDENTITY_MODE", "strict")
        .env_remove("CHORUS_IDENTITY_TOKEN")
        .env_remove("CLAUDECODE")
        .status();
    let event = chorus_agent::store::event(
        id,
        "session.disconnected",
        json!({"reason":"native_process_exit","exit_code":status.as_ref().ok().and_then(|s|s.code())}),
    );
    let _ = request(
        "POST",
        "/v1/events",
        Some(serde_json::to_value(event).unwrap()),
    )
    .await;
    match status {
        Ok(status) if status.success() => Ok(session),
        Ok(status) => Err(format!(
            "native runtime exited: {status}; resume session {id} explicitly"
        )),
        Err(reason) => Err(format!(
            "native runtime unavailable: {reason}; release session {id} with stop"
        )),
    }
}

fn input() -> Result<Value> {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take((MAX_INPUT_BYTES + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > MAX_INPUT_BYTES {
        return Err("input exceeds size limit".into());
    }
    serde_json::from_slice(&bytes).map_err(|e| format!("invalid request JSON: {e}"))
}
#[tokio::main]
async fn main() {
    match run().await {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("chorus-agent: {error}");
            std::process::exit(1);
        }
    }
}
async fn run() -> Result<Value> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let verb = args.first().map(String::as_str).unwrap_or("help");
    if verb == "launch" {
        return launch(&args).await;
    }
    if matches!(verb, "help" | "--help" | "-h") {
        return Ok(
            json!({"usage":"chorus-agent launch <role> [--profile name] [--cwd path]; start|register|run (JSON stdin); status [session]; resume|cancel|stop|disconnect|events <session>; send|context|handoff|switch|approve <session> (JSON stdin); reload; profiles; doctor [profile]; serve via chorus-agentd","version":VERSION}),
        );
    }
    if verb == "doctor" {
        let c = config::read(&config::config_path())?;
        if let Some(name) = args.get(1) {
            if !c.profiles.contains_key(name) {
                return Err(format!("unknown operator profile: {name}"));
            }
        }
        let mut reports = serde_json::Map::new();
        for (name, profile) in &c.profiles {
            if args.get(1).is_some_and(|id| id != name) {
                continue;
            }
            let report = match execution::probe(profile).await {
                Ok((version, caps)) => {
                    json!({"available":true,"runtime_version":version,"capabilities":caps,"enforcement":profile.enforcement})
                }
                Err(reason) => json!({"available":false,"reason":reason}),
            };
            reports.insert(name.clone(), report);
        }
        return Ok(json!({"version":VERSION,"profiles":reports}));
    }
    // Standalone jobs preserve werk verbs' subprocess independence from the daemon.
    if verb == "run" {
        let req: JobRequest = serde_json::from_value(input()?).map_err(|e| e.to_string())?;
        let c = config::read(&config::config_path())?;
        let profile = c.profiles.get(&req.profile).ok_or("unknown job profile")?;
        let result = execution::run_job(profile, req).await?;
        if args.iter().any(|a| a == "--text") {
            print!("{}", result.text);
            std::process::exit(0);
        }
        return serde_json::to_value(result).map_err(|e| e.to_string());
    }
    let id = args.get(1).cloned().unwrap_or_default();
    if !id.is_empty() && !chorus_agent::store::safe_id(&id) {
        return Err("invalid session id".into());
    }
    if verb == "resume" && !id.is_empty() {
        let previous = request("GET", &format!("/v1/sessions/{id}"), None).await?;
        if previous["mode"] == "native" {
            let c = config::read(&config::config_path())?;
            let profile = c
                .profiles
                .get(
                    previous["profile"]
                        .as_str()
                        .ok_or("invalid session profile")?,
                )
                .ok_or("unknown profile")?;
            if !matches!(profile.runtime, Runtime::Claude | Runtime::Codex) {
                return Err("native resume for this runtime requires its UI; use the documented reconnect procedure".into());
            }
            let token = std::env::var("CHORUS_SESSION_TOKEN_FILE")
                .map_err(|_| "CHORUS_SESSION_TOKEN_FILE is required")?;
            let session = request(
                "POST",
                &format!("/v1/sessions/{id}/resume"),
                Some(json!({})),
            )
            .await?;
            return native_client(profile, session, &token).await;
        }
    }
    let (method, path, body) = match verb {
        "reload" => ("POST", "/v1/config/reload".into(), Some(json!({}))),
        "profiles" => ("GET", "/v1/config".into(), None),
        "start" | "register" => ("POST", "/v1/sessions".into(), Some(input()?)),
        "status" if id.is_empty() => ("GET", "/v1/sessions".into(), None),
        "status" => ("GET", format!("/v1/sessions/{id}"), None),
        "events" => ("GET", format!("/v1/sessions/{id}/events"), None),
        "send" | "context" | "handoff" | "switch" | "approve" if !id.is_empty() => {
            ("POST", format!("/v1/sessions/{id}/{verb}"), Some(input()?))
        }
        "resume" | "cancel" | "stop" | "disconnect" if !id.is_empty() => {
            ("POST", format!("/v1/sessions/{id}/{verb}"), Some(json!({})))
        }
        _ => return Err("unknown operation or missing session ID; use --help".into()),
    };
    request(method, &path, body).await
}
