//! #4513 — a tool is a Skill row. A deterministic Skill with an inputShape is
//! a tool the runner offers: its arguments' schema comes from the ToolInput
//! shape athena-gen generated (tool_inputs.json, never hand-written), what it
//! runs comes from the row (implementedBy + an arguments template), and it
//! runs as the Skill's owner with the caller recorded (Jeff 10-12, as the old
//! borg-tend-herald did). Who may call it is authz: the owner and Jeff.

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
    /// #4515 — an "http" tool calls a route (implementedBy = "METHOD url") with
    /// its checked arguments as the JSON body; a "deterministic" one runs a binary.
    pub http: bool,
}

/// #4514 (Silas + Wren, 2026-10-13): a verb a role runs on its own card runs as
/// the verified caller (werk-commit); a tool that acts with one principal's
/// rights runs as its owner (borg-tend-herald). A row that says neither is refused.
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
        let executor = s(row, "executor");
        if !matches!(executor, "deterministic" | "http") || shape.is_empty() {
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
        if executor == "http" {
            if let Err(e) = http_target(binary, &Map::new()) {
                unmeasured.push(Unmeasured { skill: name, reason: format!("implementedBy '{binary}': {e}") });
                continue;
            }
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
            http: executor == "http",
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
/// #4514 — arguments from a command line arrive as text: turn each into the
/// type its schema declares ("true" → true, "42" → 42, a lone value → a
/// one-item list). Anything that does not convert is left as it is, and
/// check_args then refuses it by name.
pub fn coerce_args(args: &Map<String, Value>, schema: &Map<String, Value>) -> Map<String, Value> {
    let props = schema.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
    let one = |v: &Value, ty: &str| -> Value {
        match (ty, v) {
            ("boolean", Value::String(s)) if s == "true" || s == "false" => Value::Bool(s == "true"),
            ("integer", Value::String(s)) => s.parse::<i64>().map(Value::from).unwrap_or_else(|_| v.clone()),
            ("string", Value::Number(n)) => Value::String(n.to_string()),
            _ => v.clone(),
        }
    };
    args.iter()
        .map(|(k, v)| {
            let p = props.get(k);
            let ty = p.and_then(|p| p.get("type")).and_then(Value::as_str).unwrap_or("string");
            let out = if ty == "array" {
                let item_ty = p.and_then(|p| p.get("items")).and_then(|i| i.get("type")).and_then(Value::as_str).unwrap_or("string");
                match v {
                    Value::Array(a) => Value::Array(a.iter().map(|x| one(x, item_ty)).collect()),
                    other => Value::Array(vec![one(other, item_ty)]),
                }
            } else {
                one(v, ty)
            };
            (k.clone(), out)
        })
        .collect()
}

pub fn check_args(args: &Map<String, Value>, schema: &Map<String, Value>) -> Result<(), String> {
    let props = schema.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
    for r in schema.get("required").and_then(Value::as_array).into_iter().flatten().filter_map(Value::as_str) {
        if !args.contains_key(r) {
            return Err(format!("{r} is required"));
        }
    }
    for (k, v) in args {
        let Some(p) = props.get(k) else { return Err(format!("{k} is not an argument of this tool")) };
        // #4514 — a list checks each of its items against the item schema
        let (items, item_schema): (Vec<&Value>, &Value) = if p.get("type").and_then(Value::as_str) == Some("array") {
            match v.as_array() {
                Some(a) if !a.is_empty() => (a.iter().collect(), p.get("items").unwrap_or(&Value::Null)),
                Some(_) => return Err(format!("{k} is an empty list")),
                None => return Err(format!("{k} must be a list")),
            }
        } else {
            (vec![v], p)
        };
        for it in items {
            let ok = match item_schema.get("type").and_then(Value::as_str).unwrap_or("string") {
                "boolean" => it.is_boolean(),
                "integer" => it.is_i64() || it.is_u64(),
                "number" => it.is_number(),
                _ => it.is_string(),
            };
            if !ok {
                return Err(format!("{k} has the wrong type"));
            }
            if let Some(allowed) = item_schema.get("enum").and_then(Value::as_array) {
                if !allowed.contains(it) {
                    return Err(format!("{k} must be one of {}", allowed.iter().filter_map(Value::as_str).collect::<Vec<_>>().join(", ")));
                }
            }
            // #4515 — the shape's sh:pattern: a value that does not match is refused
            // by name; a pattern that is not a valid regex refuses too, never passes
            if let Some(pat) = item_schema.get("pattern").and_then(Value::as_str) {
                let re = regex::Regex::new(pat).map_err(|e| format!("{k} has a pattern that is not a regex ({pat}): {e}"))?;
                if !re.is_match(it.as_str().unwrap_or_default()) {
                    return Err(format!("{k} must match {pat}"));
                }
            }
        }
    }
    Ok(())
}

fn render(tok: &str, args: &Map<String, Value>, caller: &str) -> Option<Vec<String>> {
    // Some(tokens) = emit them (a true boolean emits none); None = absent
    let whole = tok.strip_prefix('{').and_then(|t| t.strip_suffix('}')).filter(|n| !n.contains(['{', '}']));
    let Some(name) = whole else {
        // #4514 — placeholders inside a word (`--domain={domain}`, werk-test's
        // only form; #4515 `{category}:{value}`, cards untag's): every one is
        // filled, and the word is kept only when every value is present
        let mut out = String::new();
        let mut rest = tok;
        while let (Some(i), Some(j)) = (rest.find('{'), rest.find('}')) {
            if j < i { break; }
            let inner = &rest[i + 1..j];
            let value = if inner == "caller" {
                caller.to_string()
            } else {
                match args.get(inner) {
                    None | Some(Value::Null) | Some(Value::Bool(false)) | Some(Value::Array(_)) => return None,
                    Some(v) => v.as_str().map(String::from).unwrap_or_else(|| v.to_string()),
                }
            };
            out.push_str(&rest[..i]);
            out.push_str(&value);
            rest = &rest[j + 1..];
        }
        out.push_str(rest);
        return Some(vec![out]);
    };
    if name == "caller" {
        return Some(vec![caller.to_string()]);
    }
    let text = |v: &Value| match v {
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    match args.get(name) {
        None | Some(Value::Null) | Some(Value::Bool(false)) => None,
        Some(Value::Bool(true)) => Some(vec![]),
        // #4514 — a list is its items, each its own argument, in order
        Some(Value::Array(a)) if a.is_empty() => None,
        Some(Value::Array(a)) => Some(a.iter().map(text).collect()),
        Some(other) => Some(vec![text(other)]),
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
        let rendered = if tok.is_empty() { Some(vec![]) } else { render(tok, args, caller) };
        match (&mut group, rendered) {
            (Some(_), None) => group_ok = false,
            (Some(g), Some(t)) => g.extend(t),
            (None, Some(t)) => out.extend(t),
            (None, None) => {}
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
/// and CHORUS_ROLE / DEPLOY_ROLE naming the owner, as the TS borg-tend-herald did.
/// borg-tend-herald exits 2 without CHORUS_ROLE.
pub fn child_env(owner: &str, token: &str, trace: &str) -> Vec<(&'static str, String)> {
    vec![
        ("CHORUS_IDENTITY_TOKEN", token.to_string()),
        ("CHORUS_ROLE", owner.to_string()),
        ("DEPLOY_ROLE", owner.to_string()),
        ("CHORUS_TRACE", trace.to_string()),
    ]
}

/// #4515 — an http tool's method and URL from its implementedBy ("POST
/// http://127.0.0.1:3475/api/messages"), with `{name}` in the URL filled from
/// the arguments. Only loopback is allowed: the runner never carries a
/// caller's credential off the machine.
pub fn http_target(implemented_by: &str, args: &Map<String, Value>) -> Result<(String, String), String> {
    let (method, url) = implemented_by.split_once(' ').ok_or_else(|| format!("'{implemented_by}' is not METHOD url"))?;
    if !matches!(method, "GET" | "POST" | "PUT" | "PATCH" | "DELETE") {
        return Err(format!("'{method}' is not an HTTP method"));
    }
    // #4515 (Silas 21:33) — a route names one of the runner's own Chorus
    // services, never an address: the caller's credential goes nowhere else
    // (not Loki, not off the machine), and a demo runner reaches its werk's
    // services because the base comes from its own settings.
    let (service, rest) = url.split_once('}').filter(|(s, r)| s.starts_with('{') && r.starts_with('/')).ok_or_else(|| format!("{url} must start with {{api}}, {{door}} or {{pulse}}"))?;
    let base = service_base(&service[1..]).ok_or_else(|| format!("{url} names {}}}, not one of {{api}}, {{door}}, {{pulse}}", service))?;
    let mut out = rest.to_string();
    while let (Some(i), Some(j)) = (out.find('{'), out.find('}')) {
        if j < i {
            break;
        }
        // `{name=default}` fills an optional argument the caller left out
        let inner = out[i + 1..j].to_string();
        let (k, default) = inner.split_once('=').map(|(k, d)| (k.to_string(), Some(d.to_string()))).unwrap_or((inner, None));
        let given = args.get(&k).map(|v| v.as_str().map(String::from).unwrap_or_else(|| v.to_string())).unwrap_or_default();
        // `{role=@caller}` defaults to the verified caller (clearing-wip, clearing-prioritize)
        let fallback = match default.as_deref() {
            Some("@caller") => args.get("caller").and_then(|v| v.as_str()).unwrap_or_default().to_string(),
            other => other.unwrap_or_default().to_string(),
        };
        let v = if given.is_empty() { fallback } else { given };
        if v.is_empty() && default.is_none() && !args.is_empty() {
            return Err(format!("{k} is needed for {url}"));
        }
        let enc: String = v.bytes().map(|b| if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") }).collect();
        out.replace_range(i..=j, &enc);
    }
    Ok((method.to_string(), format!("{}{}", base.trim_end_matches('/'), out)))
}

/// The runner's own Chorus services, from its settings (env-up sets them per werk).
pub fn service_base(name: &str) -> Option<String> {
    let (var, dflt) = match name {
        "api" => ("CHORUS_API_URL", "http://127.0.0.1:3340"),
        "door" => ("CHORUS_DOOR", "http://127.0.0.1:3360"),
        "pulse" => ("CHORUS_PULSE_URL", "http://127.0.0.1:3475"),
        _ => return None,
    };
    Some(std::env::var(var).ok().filter(|v| !v.is_empty()).unwrap_or_else(|| dflt.to_string()))
}

/// #4515 — a tool argument that names a role (`chorus:namesRole` on its shape
/// property, athena-gen's `namesRole` key) must be one the roles door lists:
/// "agent" for a role that acts, "peer" for any agent or human (Jeff owns cards
/// too). The old server checked `role`/`owner`/`target` by hand in every tool
/// (roles-arg.ts); here the shape says which arguments are roles, once.
/// Returns (argument, value, kind) for each role argument present in the call.
pub fn role_args(args: &Map<String, Value>, schema: &Map<String, Value>) -> Vec<(String, String, String)> {
    let props = schema.get("properties").and_then(Value::as_object).cloned().unwrap_or_default();
    let mut out = Vec::new();
    for (k, p) in props {
        let Some(kind) = p.get("namesRole").and_then(Value::as_str) else { continue };
        if let Some(v) = args.get(&k).and_then(Value::as_str).filter(|v| !v.is_empty()) {
            out.push((k.clone(), v.to_string(), kind.to_string()));
        }
    }
    out
}

/// The first role argument the roles door does not list, as a refusal; or
/// None. `rows` is the door's `/v1/roles/roles` answer. Pure.
pub fn role_refusal(checks: &[(String, String, String)], rows: &Value) -> Option<String> {
    let Some(rows) = rows.get("data").and_then(Value::as_array) else {
        return Some("the roles door answered with no data list; nothing ran".into());
    };
    let kind = |r: &Value| r.get("roleKind").and_then(Value::as_str).unwrap_or("").to_string();
    let names = |want: &dyn Fn(&str) -> bool| {
        let mut v: Vec<String> = rows.iter().filter(|r| want(&kind(r))).filter_map(|r| r.get("name").and_then(Value::as_str).map(String::from)).collect();
        v.sort();
        v
    };
    let agents = names(&|k| k == "agent");
    let peers = names(&|k| k == "agent" || k == "human");
    for (arg, value, k) in checks {
        let allowed = if k == "agent" { &agents } else { &peers };
        if !allowed.contains(&value.to_lowercase()) {
            return Some(format!("Unknown {arg} '{value}' — the roles door lists {}", allowed.join(" | ")));
        }
    }
    None
}
