//! Adapter entrypoint. Native hook JSON is untrusted observation, never runtime
//! enrollment or a capability grant. The supervisor supplies CHORUS_SESSION_ID.
use crate::runtime_tools::normalize_tools;
use serde_json::{json, Value};
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::process::{Command, ExitCode, Stdio};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
enum Event { Start, Before, After, Prompt, Stop, End, Compact }

fn event(name: &str) -> Result<Event, String> {
    match name {
        "SessionStart" | "session-start" | "session.created" => Ok(Event::Start),
        "PreToolUse" | "BeforeTool" | "pre-tool-use" | "tool.execute.before" => Ok(Event::Before),
        "PostToolUse" | "PostToolUseFailure" | "AfterTool" | "post-tool-use" | "tool.execute.after" => Ok(Event::After),
        "UserPromptSubmit" | "BeforeAgent" | "user-prompt-submit" => Ok(Event::Prompt),
        "Stop" | "AfterAgent" | "stop" | "session.idle" => Ok(Event::Stop),
        "SessionEnd" | "session-end" | "session.deleted" => Ok(Event::End),
        "PreCompact" | "PreCompress" | "pre-compact" => Ok(Event::Compact),
        _ => Err(format!("unsupported native hook event {name}")),
    }
}

#[derive(Debug, Default, PartialEq)]
pub struct Decision { pub denied: bool, pub reason: String, pub context: String }

#[derive(Debug, Default, PartialEq)]
struct DeliveryReceipt { ids: Vec<u64>, context_token: Option<String> }

fn append_boundary(boundary: &Value, decision: &mut Decision) -> DeliveryReceipt {
    let mut receipt = DeliveryReceipt::default();
    let mut handoff_delivered = false;
    if let Some(context) = boundary.get("context") {
        let texts: Vec<&str> = match context {
            Value::String(s) => vec![s.as_str()],
            Value::Array(items) => items.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        for text in texts.into_iter().filter(|s|!s.is_empty()) {
            decision.context.push('\n'); decision.context.push_str(text);
            handoff_delivered = true;
        }
    }
    if handoff_delivered {
        receipt.context_token = boundary.get("context_token").and_then(Value::as_str).map(str::to_string);
    }
    if let Some(messages) = boundary.get("messages").and_then(Value::as_array) {
        for message in messages {
            let id = message["id"].as_u64().filter(|id|*id>0 && *id<=9_007_199_254_740_991);
            let header = |key:&str|message[key].as_str().filter(|s|!s.is_empty() && s.len()<=200 && !s.contains(['\r','\n']));
            let valid = (id, header("message_id"), header("from"), message["content"].as_str().filter(|s|!s.is_empty()));
            if let (Some(id),Some(message_id),Some(from),Some(content)) = valid {
                if !matches!(message["kind"].as_str(),Some("peer_message" | "human_input")) {
                    eprintln!("chorus boundary message has invalid kind; left unacknowledged"); continue;
                }
                decision.context.push_str(&format!("\n\n[Chorus message {message_id} from {from}]\n{content}"));
                if !receipt.ids.contains(&id) {receipt.ids.push(id);}
            } else {eprintln!("chorus boundary message is malformed; left unacknowledged");}
        }
    }
    receipt
}

/// A policy ask must never disappear into a runtime with no approval callback.
/// These command hooks return deny; a managed supervisor may request approval
/// and retry after its native approval protocol has completed.
pub fn parse_policy_reply(raw: &Value) -> Result<Decision, String> {
    let code = raw.get("exit_code").and_then(Value::as_i64).ok_or("policy response missing exit_code")?;
    let mut decision = Decision::default();
    if code != 0 {
        decision.denied = true;
        decision.reason = raw.get("stderr").and_then(Value::as_str).unwrap_or("policy blocked operation").into();
    }
    if let Some(output) = raw.get("stdout").and_then(Value::as_str).filter(|s| !s.is_empty()) {
        if let Ok(v) = serde_json::from_str::<Value>(output) {
            let specific = v.get("hookSpecificOutput").unwrap_or(&v);
            let permission = specific.get("permissionDecision").or_else(|| v.get("decision")).and_then(Value::as_str);
            if matches!(permission, Some("deny" | "block" | "ask")) {
                decision.denied = true;
                decision.reason = specific.get("permissionDecisionReason").or_else(|| v.get("reason"))
                    .and_then(Value::as_str).unwrap_or("policy requires approval or denied the operation").into();
            }
            decision.context = specific.get("additionalContext").and_then(Value::as_str).unwrap_or("").into();
        } else { decision.context = output.into(); }
    }
    Ok(decision)
}

pub fn native_reply(runtime: &str, name: &str, d: &Decision) -> Value {
    if matches!(runtime, "claude-code" | "claude" | "codex") {
        if name == "PreToolUse" || name == "pre-tool-use" {
            let mut specific = json!({"hookEventName":"PreToolUse"});
            if d.denied {
                specific["permissionDecision"] = json!("deny");
                specific["permissionDecisionReason"] = json!(d.reason);
            }
            if !d.context.is_empty() { specific["additionalContext"] = json!(d.context); }
            return json!({"hookSpecificOutput":specific});
        }
        if d.denied { return json!({"decision":"block","reason":d.reason}); }
        return json!({"hookSpecificOutput":{"hookEventName":name,"additionalContext":d.context}});
    }
    // Gemini command hooks use decision/reason plus an optional context envelope.
    // The OpenCode plugin and supervisor invoke this same explicit result shape.
    let mut reply = json!({"decision":if d.denied {"deny"} else {"allow"},"reason":d.reason});
    if !d.context.is_empty() { reply["hookSpecificOutput"] = json!({"additionalContext":d.context}); }
    reply
}

fn policy(endpoint: &str, input: &Value) -> Result<Decision, String> {
    let bin = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut child = Command::new(bin).arg(endpoint).env("CHORUS_HOOK_RAW", "1")
        .stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped())
        .spawn().map_err(|e| format!("policy shim unavailable: {e}"))?;
    if let Some(mut stdin) = child.stdin.take() { stdin.write_all(input.to_string().as_bytes()).map_err(|e| e.to_string())?; }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let raw: Value = serde_json::from_slice(&output.stdout).map_err(|_| "policy response missing or malformed".to_string())?;
    // The legacy shim's local denial runs before CHORUS_HOOK_RAW encoding.
    if raw.get("hookSpecificOutput").is_some() {
        return parse_policy_reply(&json!({"exit_code":0,"stdout":raw.to_string()}));
    }
    parse_policy_reply(&raw)
}

fn daemon_post(path: &str, body: &Value) -> Result<Value, String> {
    let socket = std::env::var("CHORUS_AGENT_SOCKET").unwrap_or_else(|_| {
        format!("{}/.chorus/run/chorus-agent.sock", std::env::var("HOME").unwrap_or_default())
    });
    let mut stream = UnixStream::connect(socket).map_err(|e| e.to_string())?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
    stream.set_write_timeout(Some(Duration::from_secs(2))).map_err(|e| e.to_string())?;
    let body = body.to_string();
    write!(stream, "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).map_err(|e| e.to_string())?;
    let mut reply = String::new(); stream.read_to_string(&mut reply).map_err(|e| e.to_string())?;
    let (headers, payload) = reply.split_once("\r\n\r\n").ok_or("malformed agent response")?;
    if !headers.lines().next().unwrap_or("").contains(" 2") { return Err("agent daemon rejected hook observation".into()); }
    serde_json::from_str(payload).map_err(|e| e.to_string())
}

fn redact_metadata(value: &mut Value) {
    match value {
        Value::Object(object) => {
            object.retain(|key, _| {
                let key = key.to_ascii_lowercase();
                !matches!(key.as_str(), "trusted" | "enforcement" | "capabilities" | "token" | "access_token" | "api_key" | "authorization" | "password" | "credential_file" | "session_token")
            });
            for child in object.values_mut() { redact_metadata(child); }
        }
        Value::Array(values) => { for child in values { redact_metadata(child); } }
        _ => {}
    }
}

fn observation_data(runtime: &str, ev: Event, raw: &Value, decision: &Decision) -> Value {
    let runtime = match runtime { "claude-code" => "claude", "openai-compatible" => "external", other => other };
    let mut data = json!({"runtime":runtime,
        "decision":if decision.denied {"deny"} else {"allow"},"reason":decision.reason});
    if matches!(ev, Event::Before | Event::After) {
        let cwd = raw.get("cwd").and_then(Value::as_str).map(str::to_string)
            .or_else(||std::env::current_dir().ok().map(|p|p.to_string_lossy().into_owned())).unwrap_or_default();
        let tool = raw.get("tool_name").or_else(||raw.get("tool")).and_then(Value::as_str).unwrap_or("");
        let input = raw.get("tool_input").or_else(||raw.get("input")).or_else(||raw.get("args")).unwrap_or(&Value::Null);
        match normalize_tools(runtime, tool, input, &cwd) {
            Ok(ops) => {
                data["operations"] = json!(ops.into_iter().map(|op|json!({"tool_name":op.tool,"tool_input":op.input,"cwd":op.cwd})).collect::<Vec<_>>());
                if ev == Event::After {
                    data["tool_response"] = raw.get("tool_response").or_else(||raw.get("tool_output")).or_else(||raw.get("output")).cloned().unwrap_or(Value::Null);
                    let response = &data["tool_response"];
                    data["tool_output_is_error"] = json!(raw["tool_output_is_error"] == true || response["is_error"] == true
                        || response.get("error").is_some_and(|v|!v.is_null())
                        || response.get("exit_code").or_else(||response.get("exitCode")).and_then(Value::as_i64).is_some_and(|n|n!=0));
                }
            }
            Err(reason) => { data["evidence_unavailable"] = json!(true); data["evidence_reason"] = json!(reason); }
        }
    } else if ev == Event::Prompt { data["prompt"] = raw.get("prompt").cloned().unwrap_or(Value::Null); }
    redact_metadata(&mut data);
    // Avoid unbounded private journals. Truncated arguments/output are not
    // treated as complete evidence for gates.
    if data.to_string().len() > 256 * 1024 {
        return json!({"runtime":runtime,"decision":if decision.denied {"deny"} else {"allow"},"evidence_unavailable":true,"evidence_reason":"native observation exceeds 256 KiB"});
    }
    data
}

fn publish(runtime: &str, ev: Event, raw: &Value, decision: &Decision) -> Option<Value> {
    let sid = std::env::var("CHORUS_SESSION_ID").ok()?;
    // IDs enter a URL below; never permit a payload-supplied path or identity.
    if sid.is_empty() || !sid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') { return None; }
    let kind = match ev {
        Event::Start => "session.started", Event::Before => "tool.requested",
        Event::After => "tool.completed", Event::Prompt => "turn.started",
        Event::Stop => "turn.completed", Event::End => "session.ended", Event::Compact => "context.compacted",
    };
    let event_id = uuid::Uuid::now_v7().to_string();
    let native_id = raw.get("session_id").or_else(|| raw.get("sessionID")).and_then(Value::as_str);
    // Never forward native arbitrary metadata (credentials/trust flags) wholesale.
    let data = observation_data(runtime, ev, raw, decision);
    let body = json!({"version":1,"session_id":sid,"event_id":event_id,"type":kind,
        "native_session_id":native_id,"tool_call_id":raw.get("tool_use_id").or_else(|| raw.get("callID")),"data":data});
    if let Err(e) = daemon_post("/v1/events", &body) { eprintln!("chorus runtime observation unavailable: {e}"); }
    if !decision.denied && matches!(ev, Event::Start | Event::Prompt) {
        let boundary = json!({"event_id":uuid::Uuid::now_v7().to_string(),"native_session_id":native_id,
            "state":if ev == Event::Prompt {"running"} else {"idle"}});
        return daemon_post(&format!("/v1/sessions/{sid}/boundary"), &boundary).ok();
    }
    None
}

fn evaluate(runtime: &str, ev: Event, raw: &Value) -> Result<Decision, String> {
    if !matches!(runtime, "claude-code" | "claude" | "codex" | "gemini" | "opencode" | "openai-compatible" | "external") {
        return Err(format!("unknown runtime {runtime}"));
    }
    if matches!(ev, Event::Start | Event::End | Event::Compact) { return Ok(Decision::default()); }
    let cwd = raw.get("cwd").and_then(Value::as_str).map(str::to_string)
        .or_else(|| std::env::current_dir().ok().map(|p| p.to_string_lossy().into_owned())).ok_or("missing cwd")?;
    let role = std::env::var("CHORUS_ROLE").or_else(|_| std::env::var("DEPLOY_ROLE")).map_err(|_| "runtime hook requires launcher-owned role environment")?;
    if !matches!(role.as_str(), "wren" | "silas" | "kade") { return Err("runtime hook role is not enrolled".into()); }
    let sid = std::env::var("CHORUS_SESSION_ID").map_err(|_|"runtime hook requires a supervisor-issued CHORUS_SESSION_ID")?;
    if sid.is_empty() || !sid.bytes().all(|b|b.is_ascii_alphanumeric() || b == b'-' || b == b'_') { return Err("invalid Chorus session identity".into()); }
    let mut base = json!({"cwd":cwd,"session_id":sid,"chorus_session_id":sid,"runtime":runtime,"deploy_role":role,"prompt":raw.get("prompt"),
        "stop_hook_active":raw.get("stop_hook_active"),
        "tool_response":raw.get("tool_response").or_else(|| raw.get("tool_output")).or_else(|| raw.get("output")),
        "tool_output_is_error":raw.get("tool_output_is_error")});
    let endpoint = match ev { Event::Before => "pre-tool-use", Event::After => "post-tool-use",
        Event::Prompt => "user-prompt-submit", Event::Stop => "stop", _ => unreachable!() };
    if !matches!(ev, Event::Before | Event::After) { return policy(endpoint, &base); }
    let tool = raw.get("tool_name").or_else(|| raw.get("tool")).and_then(Value::as_str).ok_or("tool event has no tool name")?;
    let input = raw.get("tool_input").or_else(|| raw.get("args")).or_else(|| raw.get("input")).ok_or("tool event has no input")?;
    let operations = normalize_tools(runtime, tool, input, &cwd)?;
    let mut result = Decision::default();
    for op in operations {
        base["cwd"] = json!(op.cwd); base["tool_name"] = json!(op.tool); base["tool_input"] = op.input;
        let next = policy(endpoint, &base)?;
        if next.denied { return Ok(next); }
        if !next.context.is_empty() { result.context.push_str(&next.context); result.context.push('\n'); }
    }
    Ok(result)
}

pub fn run(args: &[String]) -> ExitCode {
    if args.len() != 2 { eprintln!("usage: chorus-hook-shim runtime-hook <runtime> <event>"); return ExitCode::from(2); }
    let (runtime, name) = (&args[0], &args[1]);
    let mut text = String::new();
    let parsed = std::io::stdin().take(16 * 1024 * 1024 + 1).read_to_string(&mut text)
        .map_err(|e| e.to_string()).and_then(|_| {
            if text.len() > 16 * 1024 * 1024 { return Err("hook input exceeds 16 MiB".into()); }
            let mut raw=serde_json::from_str::<Value>(&text).map_err(|e| e.to_string())?;
            if name == "PostToolUseFailure" && raw.is_object() {raw["tool_output_is_error"]=json!(true);}
            Ok(raw)
        });
    let ev = event(name);
    let mut decision = match (&ev, &parsed) {
        (Ok(ev), Ok(raw)) => evaluate(runtime, *ev, raw).unwrap_or_else(|reason| {
            // Missing enforcement is a refusal; missing observation stays loud
            // without hiding completed output or discarding a human prompt.
            eprintln!("chorus runtime hook: {reason}");
            Decision { denied:*ev == Event::Before, reason, context:String::new() }
        }),
        _ => Decision { denied:true, reason:"invalid runtime hook event or JSON input".into(), context:String::new() },
    };
    let mut receipt = DeliveryReceipt::default();
    if let (Ok(ev), Ok(raw)) = (ev, parsed) {
        // Observation failure is not a substitute authorization decision.
        if let Some(boundary) = publish(runtime, ev, &raw, &decision) {
            receipt = append_boundary(&boundary, &mut decision);
        }
    }
    // A successful stdout flush is the delivery boundary. If the native runtime
    // closes the pipe, leave the claims unacknowledged for idempotent redelivery.
    let reply = format!("{}\n", native_reply(runtime, name, &decision));
    let mut stdout = std::io::stdout().lock();
    if stdout.write_all(reply.as_bytes()).and_then(|_| stdout.flush()).is_err() {
        return ExitCode::from(1);
    }
    if !receipt.ids.is_empty() || receipt.context_token.is_some() {
        if let Ok(sid) = std::env::var("CHORUS_SESSION_ID") {
            if !sid.is_empty() && sid.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_') {
                if let Err(error) = daemon_post(&format!("/v1/sessions/{sid}/ack"), &json!({"ids":receipt.ids,"context_token":receipt.context_token})) {
                    eprintln!("chorus runtime delivery acknowledgement unavailable: {error}");
                }
            }
        }
    }
    // Native hooks read explicit structured decisions. OpenCode's generated
    // plugin additionally checks `decision`, never relies on exit zero alone.
    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn denies_exit_two_and_permission_denies_and_approval_requests() {
        for raw in [json!({"exit_code":2,"stderr":"blocked"}),
            json!({"exit_code":0,"stdout":json!({"hookSpecificOutput":{"permissionDecision":"deny","permissionDecisionReason":"secret"}}).to_string()}),
            json!({"exit_code":0,"stdout":json!({"hookSpecificOutput":{"permissionDecision":"ask"}}).to_string()})] {
            assert!(parse_policy_reply(&raw).unwrap().denied);
        }
        assert!(parse_policy_reply(&json!({"ok":true})).is_err());
    }
    #[test]
    fn response_translation_keeps_the_deny_and_context() {
        let d = Decision {denied:true,reason:"bad path".into(),context:"context".into()};
        assert_eq!(native_reply("gemini", "BeforeTool", &d)["decision"], "deny");
        assert_eq!(native_reply("claude-code", "PreToolUse", &d)["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(native_reply("codex", "PreToolUse", &d)["hookSpecificOutput"]["permissionDecision"], "deny");
        assert_eq!(native_reply("opencode", "tool.execute.before", &d)["reason"], "bad path");
    }
    #[test]
    fn event_mapping_is_explicit() {
        assert_eq!(event("BeforeTool").unwrap(), Event::Before);
        assert_eq!(event("tool.execute.before").unwrap(), Event::Before);
        assert!(event("trusted=true").is_err());
    }
    #[test]
    fn boundary_preserves_authorship_unicode_and_acks_only_injected_messages() {
        let mut decision=Decision{context:"existing policy context".into(),..Decision::default()};
        let receipt=append_boundary(&json!({"context":[],"context_token":"not-delivered","messages":[
            {"id":7,"message_id":"pulse:7","from":"silas","kind":"peer_message","content":"First line\nSecond → résumé"},
            {"id":8,"message_id":"pulse:8","from":"jeff","kind":"human_input","content":"Human request"},
            {"id":9,"message_id":"pulse:9","from":"kade","content":"missing kind"},
            {"id":10,"message_id":"pulse:10","from":"kade","kind":"peer_message"}
        ]}),&mut decision);
        assert_eq!(receipt.ids,vec![7,8]); assert!(receipt.context_token.is_none());
        assert!(decision.context.contains("[Chorus message pulse:7 from silas]\nFirst line\nSecond → résumé"));
        assert!(decision.context.contains("[Chorus message pulse:8 from jeff]"));
        assert!(!decision.context.contains("missing kind"));
    }
    #[test]
    fn observation_records_normalized_command_and_failure_without_credential_flags() {
        let data=observation_data("codex",Event::After,&json!({"cwd":"/workspace", "tool_name":"exec_command",
            "trusted":true, "token":"secret", "tool_input":{"cmd":"cargo test","api_key":"secret"},
            "tool_response":{"stdout":"failed: integration test","exit_code":1}}),&Decision::default());
        assert_eq!(data["operations"][0]["tool_name"],"Bash");
        assert_eq!(data["operations"][0]["tool_input"]["command"],"cargo test");
        assert_eq!(data["tool_output_is_error"],true);
        assert_eq!(data["tool_response"]["stdout"],"failed: integration test");
        assert!(!data.to_string().contains("secret"));
        assert!(!data.to_string().contains("trusted"));
    }
}
