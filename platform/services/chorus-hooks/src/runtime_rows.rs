// @domain: identity
//! #4444 reopen 2 — a runtime described by its AgentRuntime row (the table
//! athena-gen writes from the model: platform/services/shared/agent_runtimes.json).
//! Jeff, 2026-10-14 08:15: "generating properties on agent config rather than
//! hard coding". The row names the runtime's hook events, the fields its input
//! carries, the tool names the guard knows, and the JSON it expects back; this
//! module reads them. No runtime, event or field name is typed here.
use serde_json::{json, Value};

const TABLE: &str = include_str!("../../shared/agent_runtimes.json");

/// Our moments, the only names this module knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Moment { BeforePrompt, BeforeTool, AfterTool, OnStop, SessionStart, Other }

pub fn table() -> Value { serde_json::from_str(TABLE).unwrap_or(Value::Null) }

/// The runtime's row, from the generated table (or a test's own table).
pub fn row_in<'a>(table: &'a Value, runtime: &str) -> Option<&'a Value> {
    table.get("runtimes")?.get(runtime)
}

fn s<'a>(row: &'a Value, key: &str) -> Option<&'a str> { row.get(key).and_then(Value::as_str) }

fn list(row: &Value, key: &str) -> Vec<String> {
    match row.get(key) {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(String::from).collect(),
        Some(Value::String(v)) => vec![v.clone()],
        _ => Vec::new(),
    }
}

/// `a.b.c` in a JSON value.
fn at<'a>(v: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(v, |cur, k| cur.get(k))
}

fn pairs(row: &Value, key: &str) -> Vec<(String, String)> {
    list(row, key).into_iter().filter_map(|p| p.split_once('=').map(|(a, b)| (a.to_string(), b.to_string()))).collect()
}

pub fn moment(row: &Value, native: &str) -> Moment {
    let is = |k: &str| s(row, k) == Some(native);
    if is("eventBeforePrompt") { Moment::BeforePrompt }
    else if is("eventBeforeTool") { Moment::BeforeTool }
    else if is("eventAfterTool") { Moment::AfterTool }
    else if is("eventOnStop") { Moment::OnStop }
    else if is("eventSessionStart") { Moment::SessionStart }
    else { Moment::Other }
}

/// A before-prompt event that fires once per model call acts only on the first.
pub fn is_turn_start(row: &Value, raw: &Value) -> bool {
    match s(row, "fieldTurnStart") {
        Some(f) => at(raw, f).and_then(Value::as_i64).unwrap_or(0) == 0,
        None => true,
    }
}

/// The prompt: the row's prompt field, or the last transcript line of the
/// row's prompt type (its text inside the row's prompt tag, when it names one).
pub fn prompt(row: &Value, raw: &Value) -> Option<String> {
    if let Some(f) = s(row, "fieldPrompt") { return at(raw, f).and_then(Value::as_str).map(String::from); }
    let kind = s(row, "transcriptPromptType")?;
    let path = at(raw, s(row, "fieldTranscript")?)?.as_str()?;
    let text = std::fs::read_to_string(path).ok()?;
    let content = text.lines().rev()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v.get("type").and_then(Value::as_str) == Some(kind))?
        .get("content")?.as_str()?.to_string();
    let inner = s(row, "transcriptPromptTag").and_then(|tag| {
        content.split_once(&format!("<{tag}>")).and_then(|(_, r)| r.split_once(&format!("</{tag}>"))).map(|(m, _)| m.trim().to_string())
    });
    Some(inner.unwrap_or(content))
}

/// #4548 — with no reply field: the reply is the last line of the row's reply
/// type after the latest prompt line. None when the turn has no reply yet, so a
/// quiet turn never resends the previous one.
pub fn transcript_reply(row: &Value, raw: &Value) -> Option<String> {
    let reply_kind = s(row, "transcriptReplyType")?;
    let prompt_kind = s(row, "transcriptPromptType")?;
    let path = at(raw, s(row, "fieldTranscript")?)?.as_str()?;
    let text = std::fs::read_to_string(path).ok()?;
    let lines: Vec<Value> = text.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).collect();
    let kind = |v: &Value| v.get("type").and_then(Value::as_str).map(str::to_string);
    let turn_start = lines.iter().rposition(|v| kind(v).as_deref() == Some(prompt_kind)).map_or(0, |i| i + 1);
    lines[turn_start..].iter().rev()
        .find(|v| kind(v).as_deref() == Some(reply_kind))
        .and_then(|v| v.get("content")?.as_str())
        .filter(|t| !t.trim().is_empty())
        .map(String::from)
}

/// The native input in the guard's vocabulary: session_id, prompt, tool_name,
/// tool_input, cwd, transcript_path. A tool the row does not map is None: the
/// guard does not know it, and the person is asked instead.
pub fn canonical(row: &Value, m: Moment, raw: &Value) -> Result<Value, Option<String>> {
    let mut out = json!({
        "session_id": s(row, "fieldSessionId").and_then(|f| at(raw, f)).cloned().unwrap_or(Value::Null),
        "transcript_path": s(row, "fieldTranscript").and_then(|f| at(raw, f)).cloned().unwrap_or(Value::Null),
    });
    if m == Moment::BeforePrompt { out["prompt"] = json!(prompt(row, raw)); }
    if m == Moment::OnStop { out["prompt_response"] = json!(transcript_reply(row, raw)); }
    if matches!(m, Moment::BeforeTool | Moment::AfterTool) {
        let native = s(row, "fieldToolName").and_then(|f| at(raw, f)).and_then(Value::as_str).unwrap_or("");
        let Some((_, ours)) = pairs(row, "runtimeToolMap").into_iter().find(|(n, _)| n == native) else {
            return Err(Some(format!("the guard does not know {native}; asking")));
        };
        let args = s(row, "fieldToolArgs").and_then(|f| at(raw, f)).and_then(Value::as_object).cloned().unwrap_or_default();
        let map = pairs(row, "runtimeArgMap");
        let mut input = serde_json::Map::new();
        for (k, v) in args {
            let key = map.iter().find(|(n, _)| *n == k).map(|(_, o)| o.clone()).unwrap_or(k);
            input.insert(key, v);
        }
        if let Some(cwd) = input.get("cwd").cloned() { out["cwd"] = cwd; }
        out["tool_name"] = json!(ours);
        out["tool_input"] = Value::Object(input);
    }
    Ok(out)
}

fn fill(template: &str, key: &str, value: &str) -> String {
    let escaped = serde_json::to_string(value).unwrap_or_default();
    template.replace(&format!("{{{key}}}"), escaped.trim_matches('"'))
}

/// The JSON to print, from the row's reply templates.
pub fn reply(row: &Value, m: Moment, denied: bool, reason: &str, context: &str, unknown_tool: bool) -> Value {
    let pick = |k: &str| s(row, k).unwrap_or("{}").to_string();
    let text = match m {
        Moment::BeforeTool if unknown_tool => pick("replyAsk"),
        Moment::BeforeTool if denied => fill(&pick("replyDeny"), "reason", reason),
        Moment::BeforeTool => pick("replyAllow"),
        Moment::BeforePrompt if !context.is_empty() => fill(&pick("replyContext"), "context", context),
        Moment::OnStop if denied => fill(&pick("replyStopHold"), "reason", reason),
        Moment::OnStop => pick("replyStopOk"),
        _ => pick("replyNone"),
    };
    serde_json::from_str(&text).unwrap_or_else(|_| json!({}))
}

/// #4707 AC2 — Abby's real path (agy): her native PreToolUse JSON, mapped by
/// her AgentRuntime row exactly as the runtime hook maps it, then the land
/// guard in hooks/infra_guardrails.rs. Lib-only: the bin does not build this module.
#[cfg(test)]
mod land_guard_on_the_agy_path_4707 {
    use super::{canonical, row_in, table, Moment};
    use crate::types::{HookInput, HookResponse};
    use serde_json::json;

    async fn through_agy(command: &str) -> HookResponse {
        let t = table();
        let row = row_in(&t, "agy").expect("agy row in the generated table");
        let cwd = format!("{}/roles/abby-normal", crate::shared::state_paths::chorus_root());
        let raw = json!({"conversationId": "c-4707", "toolCall": {"name": "run_command",
            "args": {"CommandLine": command, "Cwd": cwd}}});
        let mut v = canonical(row, Moment::BeforeTool, &raw).expect("agy maps run_command");
        v["deploy_role"] = json!("abby-normal");
        let input: HookInput = serde_json::from_value(v).expect("canonical JSON is a HookInput");
        assert_eq!(input.tool_name_str(), "Bash", "agy run_command reaches the guard as Bash");
        crate::hooks::infra_guardrails::check(&input).await
    }

    fn denied_as_land(r: &HookResponse) -> bool {
        r.stdout.as_deref().is_some_and(|o| o.contains("\"permissionDecision\":\"deny\"") && o.contains("#4707"))
    }

    #[tokio::test]
    async fn abbys_curl_through_her_agy_hook_path_is_refused() {
        let r = through_agy("curl -s -X POST 'http://localhost:8526/api/v1/dags/cicd-land/enqueue' -d '{}'").await;
        assert!(denied_as_land(&r), "{:?}", r.stdout);
        let r = through_agy("dagu --config platform/config/dagu.yaml enqueue cicd-land -- CARD=4704").await;
        assert!(denied_as_land(&r), "{:?}", r.stdout);
    }

    #[tokio::test]
    async fn negative_proof_agy_path_lets_the_sanctioned_enqueue_through() {
        let r = through_agy("dagu --config platform/config/dagu.yaml enqueue cicd -- CARD=4707 ROLE=abby-normal").await;
        assert!(r.stdout.is_none(), "{:?}", r.stdout);
    }
}
