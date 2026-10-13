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
}

impl Runner {
    fn tools(&self) -> Vec<Tool> {
        let cols = self.collections.read().map(|c| c.clone()).unwrap_or_default();
        door_tools(&cols)
            .into_iter()
            .map(|t| Tool::new(t.name, t.description, Arc::new(t.input_schema)))
            .collect()
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
    let runner = Runner { collections };
    let config = StreamableHttpServerConfig::default().with_json_response(true).with_sse_keep_alive(None);
    let service: StreamableHttpService<Runner, LocalSessionManager> =
        StreamableHttpService::new(move || Ok(runner.clone()), Default::default(), config);
    axum::Router::new().nest_service("/mcp", service).layer(middleware::from_fn_with_state(verifier, gate))
}
