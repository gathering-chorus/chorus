//! #4512 — the runner's HTTP surface: the auth gate in front of the rmcp
//! service, and the door tools behind it. In the lib so tests drive the real
//! router without a network.

use std::process::Command;
use std::sync::{Arc, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Request, State};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use chorus_oidc::oidc::OidcVerifier;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock as Content, ListToolsResult,
    PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::transport::streamable_http_server::{
    session::local::LocalSessionManager, StreamableHttpServerConfig, StreamableHttpService,
};
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler};

use crate::skills::{argv, check_args, input_table, may_invoke, skill_tools, SkillTool};
use crate::{authenticate, door_path, door_tools, error_line, new_trace_id, refusal_body, traceparent, Caller, Refusal};

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn door_base() -> String {
    std::env::var("CHORUS_DOOR").unwrap_or_else(|_| "http://127.0.0.1:3360".into())
}

/// One GET to the door, with the trace passed on. Returns (status, body).
pub fn door_get(path: &str, trace: &str, timeout_s: u64) -> (u16, String) {
    let url = format!("{}{}", door_base(), path);
    let out = Command::new("curl")
        .args(["-s", "--max-time", &timeout_s.to_string(), "-w", "\n%{http_code}", "-H"])
        .arg(format!("traceparent: {}", traceparent(trace)))
        .arg(&url)
        .output();
    match out {
        Ok(o) => {
            let text = String::from_utf8_lossy(&o.stdout).to_string();
            match text.rsplit_once('\n') {
                Some((body, code)) => (code.trim().parse().unwrap_or(0), body.to_string()),
                None => (0, text),
            }
        }
        Err(e) => (0, format!("curl failed to start: {e}")),
    }
}

#[derive(Clone)]
pub struct Runner {
    pub collections: Arc<RwLock<Vec<String>>>,
    /// #4513 — the Skill rows offered as tools, re-read on every tools/list.
    pub skills: Arc<RwLock<Vec<SkillTool>>>,
}

impl Runner {
    fn tools(&self) -> Vec<Tool> {
        let cols = self.collections.read().map(|c| c.clone()).unwrap_or_default();
        let mut tools: Vec<Tool> = door_tools(&cols)
            .into_iter()
            .map(|t| Tool::new(t.name, t.description, Arc::new(t.input_schema)))
            .collect();
        let skills = self.skills.read().map(|s| s.clone()).unwrap_or_default();
        let any_long = skills.iter().any(|t| t.long_running);
        tools.extend(skills.into_iter().map(|t| Tool::new(t.name, t.description, Arc::new(t.schema))));
        if any_long {
            tools.push(Tool::new("task_status", "Where a long tool's run is: running, done with its exit code, or lost; and its last output lines.", Arc::new(crate::skills::task_status_schema())));
        }
        tools
    }
}

/// #4513 — re-read the Skill rows from the door. A row that asks to be a tool
/// but cannot be offered is logged as unmeasured, never dropped silently; a
/// failed read keeps the last list.
pub fn refresh_skills(into: &Arc<RwLock<Vec<SkillTool>>>) {
    match load_skill_tools() {
        Ok(tools) => {
            if let Ok(mut s) = into.write() {
                *s = tools;
            }
        }
        Err(e) => eprintln!("{}", error_line("runner", "tools/list", &format!("{e}; kept the last list"), 503, &new_trace_id())),
    }
}

fn identity_token(principal: &str) -> Option<String> {
    let home = std::env::var("CHORUS_HOME").unwrap_or_else(|_| format!("{}/CascadeProjects/chorus", std::env::var("HOME").unwrap_or_default()));
    let out = Command::new(format!("{home}/platform/scripts/chorus-identity-token")).arg(principal).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|t| !t.is_empty())
}

/// Which binary a tool runs. #4514 — the caller's werk bin slot first, as the
/// cicd pipeline's PATH does today (${CHORUS_WERK_BASE}/<role>-bin first), so a
/// card that changes a verb runs its own build; then the installed copy.
pub fn binary_path(name: &str, who: &str) -> String {
    if let Ok(base) = std::env::var("CHORUS_WERK_BASE") {
        let slot = format!("{base}/{who}-bin/{name}");
        if !who.is_empty() && std::path::Path::new(&slot).is_file() {
            return slot;
        }
    }
    let installed = format!("{}/.chorus/bin/{name}", std::env::var("HOME").unwrap_or_default());
    if std::path::Path::new(&installed).exists() { installed } else { name.to_string() }
}

/// #4514 — everything a tool call decides before it runs, shared by the MCP
/// server and `chorus-runner call` (one path, Silas 19:11): who it runs as,
/// its arguments checked against the generated schema, its argv, the binary
/// (the caller's werk slot first), and the child's environment.
pub struct Prepared {
    pub who: String,
    pub binary: String,
    pub argv: Vec<String>,
    pub env: Vec<(&'static str, String)>,
    /// #4515 — an http tool: method, URL, JSON body, and the credential it carries
    pub http: Option<(String, String, String, String)>,
}

pub fn prepare(tool: &SkillTool, args: &serde_json::Map<String, serde_json::Value>, caller: &str, trace: &str) -> Result<Prepared, (String, u16)> {
    prepare_with(tool, args, caller, trace, None)
}

/// `caller_token` is the caller's own verified credential when the call came
/// through the gate. A caller-run http tool forwards exactly that (Silas 21:07:
/// never a role header); a token is minted only when there is none (the CLI).
pub fn prepare_with(tool: &SkillTool, args: &serde_json::Map<String, serde_json::Value>, caller: &str, trace: &str, caller_token: Option<&str>) -> Result<Prepared, (String, u16)> {
    let who = crate::skills::run_identity(tool, caller).map_err(|e| (e, 403))?;
    let args = crate::skills::coerce_args(args, &tool.schema);
    check_args(&args, &tool.schema).map_err(|e| (e, 400))?;
    let argv = argv(&tool.template, &args, crate::skills::bare(caller));
    let forwarded = if tool.runs_as == crate::skills::RunsAs::Caller { caller_token.map(String::from) } else { None };
    let token = match forwarded {
        Some(t) => t,
        None => identity_token(&who).ok_or_else(|| (format!("could not mint {who}'s identity to run as them"), 503))?,
    };
    let http = if tool.http {
        let (method, url) = crate::skills::http_target(&tool.binary, &args).map_err(|e| (e, 400))?;
        Some((method, url, serde_json::Value::Object(args.clone()).to_string(), token.clone()))
    } else {
        None
    };
    Ok(Prepared { binary: binary_path(&tool.binary, &who), argv, env: crate::skills::child_env(&who, &token, trace), who, http })
}

/// #4515 — run an http tool. The credential and body go to curl through its
/// stdin config and a 0600 file, never its argv. Returns (ok, text, status).
pub fn run_http(method: &str, url: &str, body: &str, token: &str, trace: &str, timeout_s: u64) -> (bool, String, u16) {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let dir = crate::tasks::tasks_dir();
    let _ = std::fs::create_dir_all(&dir);
    let body_file = dir.join(format!(".http-{}-{trace}.json", std::process::id()));
    let wrote = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&body_file).and_then(|mut f| f.write_all(body.as_bytes()));
    if let Err(e) = wrote {
        return (false, format!("could not stage the body: {e}"), 500);
    }
    let config = format!(
        "url = \"{url}\"\nrequest = \"{method}\"\nsilent\nmax-time = {timeout_s}\nwrite-out = \"\\n%{{http_code}}\"\nheader = \"Content-Type: application/json\"\nheader = \"traceparent: {}\"\nheader = \"Authorization: Bearer {token}\"\ndata-binary = \"@{}\"\n",
        crate::traceparent(trace),
        body_file.display()
    );
    let out = Command::new("curl")
        .args(["-K", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut c| {
            if let Some(mut si) = c.stdin.take() {
                si.write_all(config.as_bytes())?;
            }
            c.wait_with_output()
        });
    let _ = std::fs::remove_file(&body_file);
    let Ok(out) = out else { return (false, "curl failed to run".into(), 500) };
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let (resp, code) = match text.rsplit_once('\n') {
        Some((b, c)) => (b.to_string(), c.trim().parse::<u16>().unwrap_or(0)),
        None => (String::new(), text.trim().parse::<u16>().unwrap_or(0)),
    };
    let ok = (200..300).contains(&code);
    (ok, format!("{method} {url} → {code}\n{}", resp.chars().take(4000).collect::<String>()), if code == 0 { 502 } else { code })
}

/// #4513 — run a Skill tool for an MCP caller, under the row's timeout; a long
/// one starts as a Task (#4514 AC2) and returns its run id at once.
async fn run_skill(tool: &SkillTool, args: &serde_json::Map<String, serde_json::Value>, caller: &str, trace: &str, caller_token: Option<String>) -> (bool, String, u16) {
    let (t, a, c, tr) = (tool.clone(), args.clone(), caller.to_string(), trace.to_string());
    let p = match tokio::task::spawn_blocking(move || prepare_with(&t, &a, &c, &tr, caller_token.as_deref())).await {
        Ok(Ok(p)) => p,
        Ok(Err((e, code))) => return (false, e, code),
        Err(e) => return (false, format!("prepare failed: {e}"), 500),
    };
    if let Some((method, url, body, token)) = p.http.clone() {
        let (tr, to) = (trace.to_string(), tool.timeout_s);
        return tokio::task::spawn_blocking(move || run_http(&method, &url, &body, &token, &tr, to)).await.unwrap_or((false, "http call failed".into(), 500));
    }
    if tool.long_running {
        let id = format!("{}-{trace}", tool.name);
        return match crate::tasks::spawn(&crate::tasks::tasks_dir(), &id, &p.binary, &p.argv, &p.env, crate::skills::bare(caller)) {
            Ok(_) => (true, format!("started {} as task {id}: call task_status with this id", tool.name), 202),
            Err(e) => (false, e, 500),
        };
    }
    let mut cmd = tokio::process::Command::new(&p.binary);
    cmd.args(&p.argv).envs(p.env.clone()).stdin(std::process::Stdio::null()).kill_on_drop(true);
    match tokio::time::timeout(std::time::Duration::from_secs(tool.timeout_s), cmd.output()).await {
        Err(_) => (false, format!("stopped after {}s (the row's timeout)", tool.timeout_s), 504),
        Ok(Err(e)) => (false, format!("{} did not start: {e}", tool.binary), 500),
        Ok(Ok(out)) => {
            let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            let tail: Vec<&str> = text.trim().lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect();
            let code = out.status.code().unwrap_or(-1);
            (out.status.success(), format!("{} {} → exit {code}\n{}", tool.binary, p.argv.join(" "), tail.join("\n")), if out.status.success() { 200 } else { 500 })
        }
    }
}

/// #4514 (Silas + Wren 19:12) — the Skill rows, read from the skills graph in
/// the store, not through the door: the door serves main's SkillShape, so a
/// card that adds a field (runsAs) could not be read by its own run. One read
/// for the server and `call` alike. Each row is the door's shape: field → value,
/// IRIs as local names, the row's name without its class prefix.
pub const SKILLS_QUERY: &str = "SELECT ?s ?p ?o WHERE { GRAPH <urn:chorus:domains:skills> { ?s a <https://jeffbridwell.com/chorus#Skill> ; ?p ?o } }";

pub fn rows_from_sparql(body: &str) -> Result<Vec<serde_json::Value>, String> {
    let v: serde_json::Value = serde_json::from_str(body).map_err(|e| format!("skills read: {e}"))?;
    let local = |x: &str| x.rsplit(['#', '/']).next().unwrap_or(x).to_string();
    let mut rows: std::collections::BTreeMap<String, serde_json::Map<String, serde_json::Value>> = Default::default();
    for b in v["results"]["bindings"].as_array().ok_or("skills read: no bindings")? {
        let (s, p, o) = (b["s"]["value"].as_str().unwrap_or(""), b["p"]["value"].as_str().unwrap_or(""), &b["o"]);
        let row = rows.entry(s.to_string()).or_insert_with(|| {
            let n = local(s);
            let mut m = serde_json::Map::new();
            m.insert("name".into(), serde_json::json!(n.strip_prefix("skill-").unwrap_or(&n)));
            m
        });
        let val = o["value"].as_str().unwrap_or("");
        let val = if o["type"] == "uri" { local(val) } else { val.to_string() };
        row.entry(local(p)).or_insert(serde_json::json!(val));
    }
    Ok(rows.into_values().map(serde_json::Value::Object).collect())
}

pub fn load_skill_tools() -> Result<Vec<SkillTool>, String> {
    let body = athena_make::sparql_json(SKILLS_QUERY).map_err(|e| format!("the skills graph is unreadable: {e}"))?;
    let rows = rows_from_sparql(&body)?;
    let (tools, unmeasured) = skill_tools(&rows, &input_table());
    for u in unmeasured {
        eprintln!("{}", serde_json::json!({"event": "runner.tool.unmeasured", "skill": u.skill, "reason": u.reason}));
    }
    Ok(tools)
}

/// #4514 AC2 — a Task's state for its caller (or jeff), from its files.
pub fn task_status_text(id: &str, caller: &str) -> Result<String, String> {
    if id.is_empty() || id.contains(['/', '.']) {
        return Err("id must be a run id a long tool returned".into());
    }
    let dir = crate::tasks::tasks_dir();
    let rec: serde_json::Value = std::fs::read_to_string(dir.join(format!("{id}.json")))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .ok_or_else(|| format!("no task {id}"))?;
    let c = crate::skills::bare(caller);
    if rec["caller"].as_str() != Some(c) && c != "jeff" {
        return Err(format!("task {id} is {}'s; {c} may not read it", rec["caller"].as_str().unwrap_or("?")));
    }
    Ok(format!("{id}: {}\n{}", crate::tasks::status(&dir, id).word(), crate::tasks::tail(&dir, id, 12)))
}

fn caller_of(ctx: &RequestContext<RoleServer>) -> String {
    ctx.extensions
        .get::<http::request::Parts>()
        .and_then(|p| p.extensions.get::<Caller>())
        .map(|c| c.0.clone())
        .unwrap_or_else(|| "unverified".into())
}

impl ServerHandler for Runner {
    fn get_info(&self) -> ServerConfig {
        ServerConfig::new(ServerCapabilities::builder().enable_tools().build())
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        self.tools().into_iter().find(|t| t.name == name)
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _ctx: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        // Read the door on every list, so a kind added at a land shows with no
        // restart; a failed read keeps the last list rather than dropping it.
        let c = self.collections.clone();
        let _ = tokio::task::spawn_blocking(move || refresh_collections(&c)).await;
        let k = self.skills.clone();
        let _ = tokio::task::spawn_blocking(move || refresh_skills(&k)).await;
        Ok(ListToolsResult::with_all_items(self.tools()))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        ctx: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let caller = caller_of(&ctx);
        let tool = request.name.to_string();
        let trace = new_trace_id();
        let args = request.arguments.unwrap_or_default();
        if tool == "task_status" {
            let id = args.get("id").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let text = task_status_text(&id, &caller);
            return Ok(CallToolResponse::Complete(match text {
                Ok(t) => CallToolResult::success(vec![Content::text(t)]),
                Err(e) => CallToolResult::error(vec![Content::text(format!("{e} (trace {trace})"))]),
            }));
        }
        let skill = self.skills.read().ok().and_then(|s| s.iter().find(|t| t.name == tool).cloned());
        if let Some(st) = skill {
            // the caller's own verified credential, only when the gate verified it
            let token = ctx
                .extensions
                .get::<http::request::Parts>()
                .filter(|p| p.extensions.get::<Caller>().is_some())
                .and_then(|p| crate::bearer(p.headers.get(http::header::AUTHORIZATION).and_then(|h| h.to_str().ok())).map(String::from));
            let (ok, text, status) = run_skill(&st, &args, &caller, &trace, token).await;
            if ok {
                return Ok(CallToolResponse::Complete(CallToolResult::success(vec![Content::text(text)])));
            }
            eprintln!("{}", error_line(&caller, &tool, text.lines().next().unwrap_or(""), status, &trace));
            return Ok(CallToolResponse::Complete(CallToolResult::error(vec![Content::text(format!("{text} (trace {trace})"))])));
        }
        let cols = self.collections.read().map(|c| c.clone()).unwrap_or_default();
        let path = match door_path(&tool, &args, &cols) {
            Ok(p) => p,
            Err(bad) => {
                eprintln!("{}", error_line(&caller, &tool, &bad.reason, bad.status, &trace));
                return Ok(CallToolResponse::Complete(CallToolResult::error(vec![Content::text(format!(
                    "{} (trace {trace})",
                    bad.reason
                ))])));
            }
        };
        let t = trace.clone();
        let (status, body) = tokio::task::spawn_blocking(move || door_get(&path, &t, 20))
            .await
            .unwrap_or((0, "door call panicked".into()));
        if (200..300).contains(&status) {
            Ok(CallToolResponse::Complete(CallToolResult::success(vec![Content::text(body)])))
        } else {
            let reason = if status == 0 { "the door did not answer".to_string() } else { format!("the door answered {status}") };
            eprintln!("{}", error_line(&caller, &tool, &reason, status, &trace));
            Ok(CallToolResponse::Complete(CallToolResult::error(vec![Content::text(format!(
                "{reason} (trace {trace}): {}",
                body.chars().take(600).collect::<String>()
            ))])))
        }
    }
}

/// The auth gate, before rmcp: refused requests never reach tool code.
async fn gate(State(v): State<Arc<OidcVerifier>>, mut req: Request, next: Next) -> Response {
    let header = req.headers().get(http::header::AUTHORIZATION).and_then(|h| h.to_str().ok()).map(String::from);
    let vv = v.clone();
    let result = tokio::task::spawn_blocking(move || authenticate(header.as_deref(), &vv, now_secs()))
        .await
        .unwrap_or(Err(Refusal { reason: "verifier failed" }));
    match result {
        Ok(caller) => {
            req.extensions_mut().insert(caller);
            next.run(req).await
        }
        Err(r) => {
            eprintln!("{}", error_line("unverified", "-", r.reason, 401, &new_trace_id()));
            (http::StatusCode::UNAUTHORIZED, [(http::header::CONTENT_TYPE, "application/json")], refusal_body(&r))
                .into_response()
        }
    }
}

/// Re-read the door's served collections from its discovery (`GET /`).
/// Never empties the list on a failed read: the last good list stays.
pub fn refresh_collections(into: &Arc<RwLock<Vec<String>>>) {
    let (status, body) = door_get("/", &new_trace_id(), 10);
    match crate::served_collections(&body) {
        Ok(cols) => {
            if let Ok(mut c) = into.write() {
                *c = cols;
            }
        }
        Err(e) => eprintln!("{}", error_line("runner", "tools/list", &format!("door discovery failed: {e}; kept the last list"), status, &new_trace_id())),
    }
}

/// The whole surface: /mcp behind the gate. Every route, every method.
pub fn router(verifier: Arc<OidcVerifier>, collections: Arc<RwLock<Vec<String>>>) -> axum::Router {
    let runner = Runner { collections, skills: Arc::new(RwLock::new(Vec::new())) };
    let config = StreamableHttpServerConfig::default().with_json_response(true).with_sse_keep_alive(None);
    let service: StreamableHttpService<Runner, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(runner.clone()), Default::default(), config);
    // #4514 — /health answers status only (no rows, no settings; Silas 18:55),
    // outside the gate, so a demo env can smoke the runner without signing
    let health = axum::Router::new().route("/health", axum::routing::get(|| async { axum::Json(serde_json::json!({ "status": "ok", "service": "chorus-runner" })) }));
    axum::Router::new().nest_service("/mcp", service).layer(middleware::from_fn_with_state(verifier, gate)).merge(health)
}
