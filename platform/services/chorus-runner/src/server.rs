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
        tools.extend(skills.into_iter().map(|t| Tool::new(t.name, t.description, Arc::new(t.schema))));
        tools
    }
}

/// #4513 — re-read the Skill rows from the door. A row that asks to be a tool
/// but cannot be offered is logged as unmeasured, never dropped silently; a
/// failed read keeps the last list.
pub fn refresh_skills(into: &Arc<RwLock<Vec<SkillTool>>>) {
    let (status, body) = door_get("/v1/skills/skills?limit=500", &new_trace_id(), 15);
    let rows = serde_json::from_str::<serde_json::Value>(&body)
        .ok()
        .and_then(|v| v.get("items").or_else(|| v.get("data")).and_then(|r| r.as_array()).cloned());
    match rows {
        Some(rows) if (200..300).contains(&status) => {
            let (tools, unmeasured) = skill_tools(&rows, &input_table());
            for u in unmeasured {
                eprintln!("{}", serde_json::json!({"event": "runner.tool.unmeasured", "skill": u.skill, "reason": u.reason}));
            }
            if let Ok(mut s) = into.write() {
                *s = tools;
            }
        }
        _ => eprintln!("{}", error_line("runner", "tools/list", "skill rows unreadable; kept the last list", status, &new_trace_id())),
    }
}

fn identity_token(principal: &str) -> Option<String> {
    let home = std::env::var("CHORUS_HOME").unwrap_or_else(|_| format!("{}/CascadeProjects/chorus", std::env::var("HOME").unwrap_or_default()));
    let out = Command::new(format!("{home}/platform/scripts/chorus-identity-token")).arg(principal).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string()).filter(|t| !t.is_empty())
}

fn binary_path(name: &str) -> String {
    let installed = format!("{}/.chorus/bin/{name}", std::env::var("HOME").unwrap_or_default());
    if std::path::Path::new(&installed).exists() { installed } else { name.to_string() }
}

/// #4513 — run a Skill tool: authz, argument check, then its binary as the
/// Skill's owner with the caller recorded, under the row's timeout.
async fn run_skill(tool: &SkillTool, args: &serde_json::Map<String, serde_json::Value>, caller: &str, trace: &str) -> (bool, String, u16) {
    if !may_invoke(caller, &tool.owner) {
        return (false, format!("{caller} may not run {}: it runs as {} with their write rights; only {} and jeff may call it", tool.name, tool.owner, tool.owner), 403);
    }
    if let Err(e) = check_args(args, &tool.schema) {
        return (false, e, 400);
    }
    let argv = argv(&tool.template, args, caller);
    let owner = tool.owner.clone();
    let Some(token) = tokio::task::spawn_blocking(move || identity_token(&owner)).await.ok().flatten() else {
        return (false, format!("could not mint {}'s identity to run as them", tool.owner), 503);
    };
    let mut cmd = tokio::process::Command::new(binary_path(&tool.binary));
    cmd.args(&argv)
        .env("CHORUS_IDENTITY_TOKEN", token)
        .env("CHORUS_TRACE", trace)
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    match tokio::time::timeout(std::time::Duration::from_secs(tool.timeout_s), cmd.output()).await {
        Err(_) => (false, format!("stopped after {}s (the row's timeout)", tool.timeout_s), 504),
        Ok(Err(e)) => (false, format!("{} did not start: {e}", tool.binary), 500),
        Ok(Ok(out)) => {
            let text = format!("{}{}", String::from_utf8_lossy(&out.stdout), String::from_utf8_lossy(&out.stderr));
            let tail: Vec<&str> = text.trim().lines().rev().take(12).collect::<Vec<_>>().into_iter().rev().collect();
            let code = out.status.code().unwrap_or(-1);
            (out.status.success(), format!("{} {} → exit {code}\n{}", tool.binary, argv.join(" "), tail.join("\n")), if out.status.success() { 200 } else { 500 })
        }
    }
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
        let skill = self.skills.read().ok().and_then(|s| s.iter().find(|t| t.name == tool).cloned());
        if let Some(st) = skill {
            let (ok, text, status) = run_skill(&st, &args, &caller, &trace).await;
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
    axum::Router::new().nest_service("/mcp", service).layer(middleware::from_fn_with_state(verifier, gate))
}
