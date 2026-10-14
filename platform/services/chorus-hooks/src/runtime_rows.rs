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

/// The native input in the guard's vocabulary: session_id, prompt, tool_name,
/// tool_input, cwd, transcript_path. A tool the row does not map is None: the
/// guard does not know it, and the person is asked instead.
pub fn canonical(row: &Value, m: Moment, raw: &Value) -> Result<Value, Option<String>> {
    let mut out = json!({
        "session_id": s(row, "fieldSessionId").and_then(|f| at(raw, f)).cloned().unwrap_or(Value::Null),
        "transcript_path": s(row, "fieldTranscript").and_then(|f| at(raw, f)).cloned().unwrap_or(Value::Null),
    });
    if m == Moment::BeforePrompt { out["prompt"] = json!(prompt(row, raw)); }
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
