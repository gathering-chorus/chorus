//! Native tool payloads become the existing policy vocabulary at one boundary.
//! This is parsing, not an authorization grant: runtime/session trust is owned by
//! the supervisor. Unknown operations and incomplete writes are rejected.
use serde_json::{json, Value};
use std::path::{Component, Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Operation {
    pub tool: String,
    pub input: Value,
    pub cwd: String,
}

fn string<'a>(value: &'a Value, names: &[&str]) -> Option<&'a str> {
    names.iter().find_map(|key| value.get(*key).and_then(Value::as_str))
}

/// Resolve .. AND existing symlinks, including a symlinked parent of a new file.
pub fn resolve_path(cwd: &str, raw: &str) -> Result<String, String> {
    if raw.is_empty() || raw.contains('\0') || !Path::new(cwd).is_absolute() {
        return Err("an absolute cwd and a nonempty path are required".into());
    }
    let raw = Path::new(raw);
    let combined = if raw.is_absolute() { raw.to_path_buf() } else { Path::new(cwd).join(raw) };
    let mut resolved = PathBuf::new();
    for component in combined.components() {
        match component {
            Component::ParentDir => { resolved.pop(); }
            Component::CurDir => {},
            part => {
                resolved.push(part.as_os_str());
                if let Ok(target) = std::fs::canonicalize(&resolved) { resolved = target; }
                else if std::fs::symlink_metadata(&resolved).map(|m| m.file_type().is_symlink()).unwrap_or(false) {
                    return Err(format!("cannot resolve symlink {}", resolved.display()));
                }
            }
        }
    }
    Ok(resolved.to_string_lossy().into_owned())
}

fn operation(tool: &str, input: Value, cwd: &str) -> Operation {
    Operation { tool: tool.into(), input, cwd: cwd.into() }
}

fn file_operation(tool: &str, input: &Value, cwd: &str) -> Result<Operation, String> {
    let path = string(input, &["file_path", "filePath", "path", "absolute_path"])
        .ok_or("file operation has no path")?;
    let path = resolve_path(cwd, path)?;
    let mut canonical = input.as_object().cloned().ok_or("tool input must be an object")?;
    canonical.insert("file_path".into(), json!(path));
    for (canonical_key, aliases) in [
        ("old_string", vec!["old_string", "oldString", "old_text"]),
        ("new_string", vec!["new_string", "newString", "new_text"]),
    ] {
        if let Some(value) = string(input, &aliases) { canonical.insert(canonical_key.into(), json!(value)); }
    }
    if tool == "Write" && !canonical.get("content").map(Value::is_string).unwrap_or(false) {
        return Err("write operation has no string content".into());
    }
    if tool == "Edit" && (!canonical.get("old_string").map(Value::is_string).unwrap_or(false)
        || !canonical.get("new_string").map(Value::is_string).unwrap_or(false)) {
        return Err("edit operation requires old and new text".into());
    }
    Ok(operation(tool, Value::Object(canonical), cwd))
}

/// Every affected source AND destination is evaluated. A malformed suffix must
/// reject the whole patch before any policy call, never authorize its prefix.
pub fn patch_operations(cwd: &str, patch: &str) -> Result<Vec<Operation>, String> {
    let lines: Vec<&str> = patch.lines().collect();
    if lines.first() != Some(&"*** Begin Patch") || lines.last() != Some(&"*** End Patch") {
        return Err("unsupported patch format: expected Begin/End Patch envelope".into());
    }
    let mut result = Vec::new();
    let mut i = 1;
    while i + 1 < lines.len() {
        let header = lines[i];
        let (kind, raw) = if let Some(p) = header.strip_prefix("*** Add File: ") { ("add", p) }
            else if let Some(p) = header.strip_prefix("*** Delete File: ") { ("delete", p) }
            else if let Some(p) = header.strip_prefix("*** Update File: ") { ("update", p) }
            else { return Err(format!("unsupported patch header: {header}")); };
        let source = resolve_path(cwd, raw)?;
        i += 1;
        let mut target = None;
        if i + 1 < lines.len() {
            if let Some(p) = lines[i].strip_prefix("*** Move to: ") {
                if kind != "update" { return Err("only Update File can move a path".into()); }
                target = Some(resolve_path(cwd, p)?);
                i += 1;
            }
        }
        let mut old = String::new();
        let mut new = String::new();
        while i + 1 < lines.len() && !lines[i].starts_with("*** Add File: ")
            && !lines[i].starts_with("*** Delete File: ") && !lines[i].starts_with("*** Update File: ") {
            let line = lines[i];
            if kind == "delete" { return Err("Delete File cannot contain hunks".into()); }
            if let Some(s) = line.strip_prefix('+') { new.push_str(s); new.push('\n'); }
            else if kind == "update" {
                if let Some(s) = line.strip_prefix('-') { old.push_str(s); old.push('\n'); }
                else if let Some(s) = line.strip_prefix(' ') {
                    old.push_str(s); old.push('\n'); new.push_str(s); new.push('\n');
                } else if !(line == "@@" || line.starts_with("@@ ") || line == "*** End of File") {
                    return Err(format!("unsupported patch line: {line}"));
                }
            } else { return Err("Add File accepts only + lines".into()); }
            i += 1;
        }
        // Write is deliberately conservative for patches/deletion: no claim that
        // a partial hunk is a harmless comment edit or an entire replacement.
        let common = json!({"file_path": source, "content": new,
            "old_string": old, "new_string": new, "operation": kind, "patch": patch});
        result.push(operation("Write", common, cwd));
        if let Some(path) = target {
            result.push(operation("Write", json!({"file_path": path, "content": new,
                "operation": "move_destination", "patch": patch}), cwd));
        }
    }
    if result.is_empty() { return Err("empty patch has no operations".into()); }
    Ok(result)
}

pub fn normalize_tools(runtime: &str, tool: &str, input: &Value, cwd: &str) -> Result<Vec<Operation>, String> {
    if !matches!(runtime, "claude-code" | "claude" | "codex" | "opencode" | "gemini" | "openai-compatible" | "external") {
        return Err(format!("unsupported runtime {runtime}"));
    }
    if !Path::new(cwd).is_absolute() { return Err("absolute tool cwd is required".into()); }
    let short = tool.strip_prefix("functions.").unwrap_or(tool);
    if matches!(short, "apply_patch" | "patch") {
        let patch = input.as_str().or_else(|| string(input, &["patch", "input", "patchText", "command"]))
            .ok_or("patch operation has no patch text")?;
        return patch_operations(cwd, patch);
    }
    if short == "MultiEdit" {
        let edits = input.get("edits").and_then(Value::as_array).filter(|e| !e.is_empty()).ok_or("MultiEdit requires edits")?;
        return edits.iter().map(|edit| {
            let mut e = edit.as_object().cloned().ok_or("edit must be an object")?;
            e.insert("file_path".into(), input.get("file_path").cloned().ok_or("MultiEdit requires file_path")?);
            file_operation("Edit", &Value::Object(e), cwd)
        }).collect();
    }
    // MCP spelling differs by host; Chorus tool names stay identical after the
    // prefix so reply-path and domain-specific gates continue to recognize them.
    let mcp_name = short.strip_prefix("mcp_chorus-api_")
        .or_else(|| short.strip_prefix("chorus-api_"))
        .map(|name| format!("mcp__chorus-api__{name}"));
    let short = mcp_name.as_deref().unwrap_or(short);
    let canonical = match short {
        "exec_command" | "run_shell_command" | "bash" | "shell" | "Bash" => "Bash",
        "read_file" | "read" | "Read" => "Read",
        "write_file" | "write" | "Write" => "Write",
        "replace" | "edit" | "Edit" => "Edit",
        "glob" | "Glob" | "list_directory" => "Glob",
        "grep" | "Grep" | "search_file_content" => "Grep",
        "task" | "Task" | "Agent" | "spawn_agent" => "Agent",
        "question" | "AskUserQuestion" | "request_user_input" => "AskUserQuestion",
        "skill" | "Skill" => "Skill",
        "WebFetch" | "web_fetch" => "WebFetch",
        "WebSearch" | "google_web_search" => "WebSearch",
        "TodoWrite" | "TodoRead" | "EnterPlanMode" | "ExitPlanMode" => short,
        name if name.starts_with("mcp__") => name,
        _ => return Err(format!("unmapped tool {tool}: runtime capability gap; execution refused")),
    };
    if matches!(canonical, "Read" | "Write" | "Edit") {
        return Ok(vec![file_operation(canonical, input, cwd)?]);
    }
    let mut canonical_input = input.as_object().cloned().ok_or("tool input must be an object")?;
    let mut actual_cwd = cwd.to_string();
    if canonical == "Bash" {
        let command = string(input, &["command", "cmd"]).filter(|s| !s.trim().is_empty()).ok_or("shell operation has no command")?;
        canonical_input.insert("command".into(), json!(command));
        if let Some(dir) = string(input, &["workdir", "dir", "cwd"]) { actual_cwd = resolve_path(cwd, dir)?; }
    }
    Ok(vec![operation(canonical, Value::Object(canonical_input), &actual_cwd)])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn patch_checks_every_source_destination_delete_and_add() {
        let patch = "*** Begin Patch\n*** Update File: a.rs\n*** Move to: ../peer/a.rs\n@@\n-old\n+new\n*** Delete File: secret\n*** Add File: tests/new.rs\n+assert!(true);\n*** End Patch";
        let ops = patch_operations("/workspace/own", patch).unwrap();
        let paths: Vec<_> = ops.iter().map(|o| o.input["file_path"].as_str().unwrap()).collect();
        assert_eq!(paths, ["/workspace/own/a.rs", "/workspace/peer/a.rs", "/workspace/own/secret", "/workspace/own/tests/new.rs"]);
        assert_eq!(ops[2].input["operation"], "delete");
    }
    #[test]
    fn invalid_suffix_cannot_authorize_a_partial_patch() {
        for suffix in ["*** Surprise: /secret", "evil", "*** Move to: "] {
            assert!(patch_operations("/workspace", &format!("*** Begin Patch\n*** Add File: a\n+x\n{suffix}\n*** End Patch")).is_err());
        }
    }
    #[test]
    fn native_file_tools_have_equivalent_policy_inputs() {
        for (runtime, tool, input) in [
            ("claude-code", "Edit", json!({"file_path":"a", "old_string":"x", "new_string":"y"})),
            ("opencode", "edit", json!({"filePath":"a", "oldString":"x", "newString":"y"})),
            ("gemini", "replace", json!({"file_path":"a", "old_string":"x", "new_string":"y"})),
        ] {
            let op = normalize_tools(runtime, tool, &input, "/workspace").unwrap().remove(0);
            assert_eq!(op.tool, "Edit"); assert_eq!(op.input["file_path"], "/workspace/a");
            assert_eq!(op.input["new_string"], "y");
        }
    }
    #[test]
    fn unknown_and_missing_mutations_are_refused() {
        assert!(normalize_tools("codex", "write_stdin", &json!({"chars":"rm -rf /"}), "/workspace").is_err());
        assert!(normalize_tools("gemini", "write_file", &json!({"file_path":"a"}), "/workspace").is_err());
        assert!(normalize_tools("codex", "apply_patch", &json!({"input":"bad"}), "/workspace").is_err());
    }
    #[test]
    fn symlinked_new_file_and_parent_traversal_resolve_physically() {
        let dir = tempfile::tempdir().unwrap();
        let base = dir.path().canonicalize().unwrap();
        std::fs::create_dir(base.join("real")).unwrap();
        std::os::unix::fs::symlink(base.join("real"), base.join("link")).unwrap();
        assert_eq!(resolve_path(base.to_str().unwrap(), "link/new").unwrap(), base.join("real/new").to_str().unwrap());
        assert_eq!(resolve_path(base.to_str().unwrap(), "link/../outside").unwrap(), base.join("outside").to_str().unwrap());
    }
}
