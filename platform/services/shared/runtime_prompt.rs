// #4965 (Silas: one reader) — a turn's prompt, read through its runtime's row, for every
// reader of hook input: chorus-hooks' row hook and chorus-principal's `seen`. Included,
// not copied (like conversation_id.rs). agy's hook input never carries the prompt (the
// row's measured notes, 10-14): it is the transcript's last line of the row's prompt type.

fn rp_s<'a>(row: &'a serde_json::Value, key: &str) -> Option<&'a str> { row.get(key).and_then(serde_json::Value::as_str) }
fn rp_at<'a>(v: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    path.split('.').try_fold(v, |cur, k| cur.get(k))
}

/// The prompt: the row's prompt field, or the last transcript line of the row's prompt
/// type (its text inside the row's prompt tag, when it names one).
pub fn prompt(row: &serde_json::Value, raw: &serde_json::Value) -> Option<String> {
    use serde_json::Value;
    if let Some(f) = rp_s(row, "fieldPrompt") { return rp_at(raw, f).and_then(Value::as_str).map(String::from); }
    let kind = rp_s(row, "transcriptPromptType")?;
    let path = rp_at(raw, rp_s(row, "fieldTranscript")?)?.as_str()?;
    let text = std::fs::read_to_string(path).ok()?;
    let content = text.lines().rev()
        .filter_map(|l| serde_json::from_str::<Value>(l).ok())
        .find(|v| v.get("type").and_then(Value::as_str) == Some(kind))?
        .get("content")?.as_str()?.to_string();
    let inner = rp_s(row, "transcriptPromptTag").and_then(|tag| {
        content.split_once(&format!("<{tag}>")).and_then(|(_, r)| r.split_once(&format!("</{tag}>"))).map(|(m, _)| m.trim().to_string())
    });
    Some(inner.unwrap_or(content))
}
