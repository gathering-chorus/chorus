// #4895 (Kade's review: one path) — the conversation a turn belongs to, for every reader of
// hook input: chorus-hooks' shim (#4785) and chorus-principal's `seen`. Included, not copied.
// Claude's hook input carries it as `session_id`. Antigravity (agy) sends the word
// "transcript" there; its id is the brain folder in `transcript_path`
// (`.../antigravity-cli/brain/<uuid>/.system_generated/logs/transcript.jsonl`).

/// #4785 — Extract the conversation UUID from an Antigravity transcript path,
/// refusing literal `"transcript"` / `"transcript_full"` file stems so turns never
/// collide on `session_id = "transcript"`.
pub fn antigravity_session_id(path: &str) -> Option<String> {
    let p = std::path::Path::new(path.trim());
    let stem = p.file_stem()?.to_str()?;
    if path.contains(".system_generated/logs/") || stem == "transcript" || stem == "transcript_full" {
        let logs = p.parent()?;
        let sys = logs.parent()?;
        let conv = sys.parent()?.file_name()?.to_str()?;
        if !conv.is_empty() && conv != "transcript" && conv != "transcript_full" && conv != ".system_generated" && conv != "logs" {
            return Some(conv.to_string());
        }
        return None;
    }
    if !stem.is_empty() && stem != "transcript" && stem != "transcript_full" {
        return Some(stem.to_string());
    }
    None
}

/// The turn's conversation: the session id the runtime gave, unless it is empty or the
/// word "transcript"/"transcript_full"; else the one in the transcript path.
pub fn conversation_id(session_id: Option<&str>, transcript_path: Option<&str>) -> Option<String> {
    session_id
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "transcript" && *s != "transcript_full")
        .map(str::to_string)
        .or_else(|| transcript_path.and_then(antigravity_session_id))
}
