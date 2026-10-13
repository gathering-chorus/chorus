//! #4513 — a tool is a Skill row. A deterministic Skill with an inputShape is
//! a tool the runner offers: its arguments' schema comes from the ToolInput
//! shape athena-gen generated (tool_inputs.json, never hand-written), what it
//! runs comes from the row (implementedBy + an arguments template), and it
//! runs as the Skill's owner with the caller recorded (Jeff 10-12, as the old
//! herald_run did). Who may call it is authz: the owner and Jeff.

use serde_json::{Map, Value};

/// The input table athena-gen writes: ToolInput shape name → JSON Schema.
pub const TOOL_INPUTS: &str = include_str!("tool_inputs.json");

pub fn input_table() -> Map<String, Value> {
    serde_json::from_str::<Value>(TOOL_INPUTS).ok().and_then(|v| v.as_object().cloned()).unwrap_or_default()
}

/// A Skill row the runner offers as a tool.
#[derive(Debug, Clone, PartialEq)]
pub struct SkillTool {
    pub name: String,
    pub description: String,
    pub binary: String,
    pub template: String,
    pub owner: String,
    pub timeout_s: u64,
    pub schema: Map<String, Value>,
    /// #4514 — whose identity the binary runs with.
    pub runs_as: RunsAs,
    /// #4514 AC2 — start it as a Task and return a run id at once.
    pub long_running: bool,
}

/// #4514 (Silas + Wren, 2026-10-13): a verb a role runs on its own card runs as
/// the verified caller (werk-commit); a tool that acts with one principal's
/// rights runs as its owner (herald-run). A row that says neither is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunsAs {
    Caller,
    Owner,
}

/// A Skill row that asks to be a tool but cannot be offered, and why. Never
/// dropped silently: each one is logged as unmeasured.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unmeasured {
    pub skill: String,
    pub reason: String,
}

fn s<'a>(row: &'a Value, k: &str) -> &'a str {
    row.get(k).and_then(Value::as_str).unwrap_or("")
}

/// A principal's bare name: `principal-silas` and `silas` are the same caller.
pub fn bare(p: &str) -> &str {
    p.strip_prefix("principal-").unwrap_or(p)
}

/// The tools among the door's Skill rows. A row is a tool when it is
/// deterministic and names an inputShape; one whose shape has no generated
/// schema, or that names no binary, is unmeasured.
pub fn skill_tools(rows: &[Value], inputs: &Map<String, Value>) -> (Vec<SkillTool>, Vec<Unmeasured>) {
    let mut tools = Vec::new();
    let mut unmeasured = Vec::new();
    for row in rows {
        let shape = s(row, "inputShape");
        if s(row, "executor") != "deterministic" || shape.is_empty() {
            continue;
        }
        let name = s(row, "name").to_string();
        let shape_name = shape.rsplit(['#', '/', ':']).next().unwrap_or(shape);
        let Some(schema) = inputs.get(shape_name).and_then(Value::as_object) else {
            unmeasured.push(Unmeasured { skill: name, reason: format!("input shape {shape_name} has no generated schema") });
            continue;
        };
        let binary = s(row, "implementedBy");
        if binary.is_empty() {
            unmeasured.push(Unmeasured { skill: name, reason: "names no binary (implementedBy)".into() });
            continue;
        }
        let runs_as = match s(row, "runsAs").rsplit(['#', '/', ':']).next().unwrap_or("") {
            "caller" => RunsAs::Caller,
            "owner" => RunsAs::Owner,
            other => {
                unmeasured.push(Unmeasured { skill: name, reason: format!("runsAs is '{other}': it must say caller or owner, never a default") });
                continue;
            }
        };
        tools.push(SkillTool {
            name,
            description: s(row, "comment").to_string(),
            binary: binary.to_string(),
            template: s(row, "arguments").to_string(),
            owner: bare(s(row, "ownedBy")).to_string(),
            timeout_s: row.get("timeoutSeconds").and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok())).unwrap_or(60),
            schema: schema.clone(),
            runs_as,
            long_running: row.get("longRunning").is_some_and(|v| v.as_bool() == Some(true) || v.as_str() == Some("true")),
        });
    }
    (tools, unmeasured)
}

/// Authz for a tool run as its owner: it carries the owner's write rights, so
/// only the owner and Jeff may call it.
pub fn may_invoke(caller: &str, owner: &str) -> bool {
    let c = bare(caller);
    c == bare(owner) || c == "jeff"
}

/// #4514 — who may call a tool, and whose identity it runs with. A caller-run
/// tool runs as the verified caller (the verb then decides what that caller may
/// touch); an unverified caller never runs anything.
pub fn run_identity(tool: &SkillTool, caller: &str) -> Result<String, String> {
    let c = bare(caller);
    if c.is_empty() || c == "unverified" {
        return Err(format!("{} needs a verified caller", tool.name));
    }
    match tool.runs_as {
        RunsAs::Caller => Ok(c.to_string()),
        RunsAs::Owner if may_invoke(c, &tool.owner) => Ok(bare(&tool.owner).to_string()),
        RunsAs::Owner => Err(format!("{c} may not run {}: it runs as {} with their write rights; only {} and jeff may call it", tool.name, tool.owner, tool.owner)),
    }
}

/// #4514 AC2 — the status tool every long verb reports through.
pub fn task_status_schema() -> Map<String, Value> {
    serde_json::json!({
        "type": "object",
        "properties": { "id": { "type": "string", "description": "The run id a long tool returned" } },
        "required": ["id"]
    }).as_object().cloned().unwrap_or_default()
}

/// The arguments checked against the generated schema: required present,
/// nothing undeclared, types and enums as declared.
pub fn check_args(args: &Map<String, Value>, schema: &Map<String, Value>) -> Result<(), String> {
    let props = schema.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
    for r in schema.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        if !args.contains_key(r) {
            return Err(format!("{r} is required"));
        }
    }
    for (k, v) in args {
        let Some(p) = props.get(k) else { return Err(format!("{k} is not an argument of this tool")) };
        let ok = match p.get("type").and_then(Value::as_str).unwrap_or("string") {
            "boolean" => v.is_boolean(),
            "integer" => v.is_i64() || v.is_u64(),
            "number" => v.is_number(),
            _ => v.is_string(),
        };
        if !ok {
            return Err(format!("{k} has the wrong type"));
        }
        if let Some(allowed) = p.get("enum").and_then(Value::as_array) {
            if !allowed.contains(v) {
                return Err(format!("{k} must be one of {}", allowed.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")));
            }
        }
    }
    Ok(())
}

fn render(tok: &str, args: &Map<String, Value>, caller: &str) -> Option<Option<String>> {
    // Some(Some(text)) = emit; Some(None) = a true boolean, emit nothing; None = absent
    let Some(name) = tok.strip_prefix('{').and_then(|t| t.strip_suffix('}')) else { return Some(Some(tok.to_string())) };
    if name == "caller" {
        return Some(Some(caller.to_string()));
    }
    match args.get(name) {
        None | Some(Value::Null) | Some(Value::Bool(false)) => None,
        Some(Value::Bool(true)) => Some(None),
        Some(Value::String(v)) => Some(Some(v.clone())),
        Some(other) => Some(Some(other.to_string())),
    }
}

/// The argv a call runs, from the row's template: `{x}` is an argument's value
/// (a true boolean adds nothing but keeps its group), `{caller}` is who asked,
/// and a `[...]` group is kept only when every placeholder in it is present.
pub fn argv(template: &str, args: &Map<String, Value>, caller: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut group: Option<Vec<String>> = None;
    let mut group_ok = true;
    for raw in template.split_whitespace() {
        let mut tok = raw;
        if let Some(t) = tok.strip_prefix('[') {
            group = Some(Vec::new());
            group_ok = true;
            tok = t;
        }
        let closes = tok.ends_with(']');
        let tok = tok.strip_suffix(']').unwrap_or(tok);
        let rendered = if tok.is_empty() { Some(None) } else { render(tok, args, caller) };
        match (&mut group, rendered) {
            (Some(_), None) => group_ok = false,
            (Some(g), Some(Some(t))) => g.push(t),
            (Some(_), Some(None)) => {}
            (None, Some(Some(t))) => out.push(t),
            (None, _) => {}
        }
        if closes {
            if let Some(g) = group.take() {
                if group_ok {
                    out.extend(g);
                }
            }
        }
    }
    out
}

/// #4513 reopen 1 — the child runs as the Skill's owner: its identity token,
/// and CHORUS_ROLE / DEPLOY_ROLE naming the owner, as the TS herald_run did.
/// herald-run exits 2 without CHORUS_ROLE.
pub fn child_env(owner: &str, token: &str, trace: &str) -> Vec<(&'static str, String)> {
    vec![
        ("CHORUS_IDENTITY_TOKEN", token.to_string()),
        ("CHORUS_ROLE", owner.to_string()),
        ("DEPLOY_ROLE", owner.to_string()),
        ("CHORUS_TRACE", trace.to_string()),
    ]
}
