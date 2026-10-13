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
        tools.push(SkillTool {
            name,
            description: s(row, "comment").to_string(),
            binary: binary.to_string(),
            template: s(row, "arguments").to_string(),
            owner: bare(s(row, "ownedBy")).to_string(),
            timeout_s: row.get("timeoutSeconds").and_then(|v| v.as_u64().or_else(|| v.as_str()?.parse().ok())).unwrap_or(60),
            schema: schema.clone(),
        });
    }
    (tools, unmeasured)
}

/// Authz for a tool: it runs with its owner's write rights, so only the owner
/// and Jeff may call it.
pub fn may_invoke(caller: &str, owner: &str) -> bool {
    let c = bare(caller);
    c == bare(owner) || c == "jeff"
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
