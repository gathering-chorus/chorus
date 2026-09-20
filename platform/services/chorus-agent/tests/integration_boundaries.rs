use chorus_agent::{
    config::Config,
    contract::*,
    server::{self, Supervisor},
    store::{self, Store},
};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::Mutex;

struct Fixture {
    _dir: tempfile::TempDir,
    app: Arc<Supervisor>,
    client: reqwest::Client,
    start: StartRequest,
    tasks: Vec<tokio::task::JoinHandle<()>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        for task in &self.tasks {
            task.abort();
        }
    }
}

async fn fixture() -> Fixture {
    // Keep the UDS path below macOS's sockaddr_un limit; /tmp also exists on Linux CI.
    let dir = tempfile::Builder::new()
        .prefix("agent-boundary-")
        .tempdir_in("/tmp")
        .unwrap();
    let executable = dir.path().join("fake-codex");
    fs::write(&executable, "#!/bin/sh\necho 'fixture-codex 1'\n").unwrap();
    fs::set_permissions(&executable, fs::Permissions::from_mode(0o700)).unwrap();
    let credential = dir.path().join("token");
    fs::write(&credential, "boundary-fixture-token").unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    let identity = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = identity.local_addr().unwrap();
    let identity_task = tokio::spawn(async move {
        let router = axum::Router::new().route("/verify", axum::routing::post(|headers: axum::http::HeaderMap| async move {
            if headers.get("authorization").and_then(|s| s.to_str().ok()) != Some("Bearer boundary-fixture-token") {
                return (axum::http::StatusCode::UNAUTHORIZED, axum::Json(json!({"error":"invalid"})));
            }
            (axum::http::StatusCode::OK, axum::Json(json!({"principal":"https://identity.test/wren", "role":"wren", "scopes":[]})))
        }));
        axum::serve(identity, router).await.unwrap();
    });
    let config: Config = serde_json::from_value(json!({"version":1,"profiles":{"native":{"runtime":"codex","mode":"native","enforcement":"trusted","approved_gaps":["filesystem isolation and hook coverage require deployment conformance"],"executable":executable,"timeout_secs":5}},"role_workspaces":{"wren":dir.path()},"max_concurrent_jobs":2})).unwrap();
    let app = Supervisor::new(
        config,
        Store::open(dir.path().join("state")).unwrap(),
        format!("http://{address}/verify"),
    )
    .unwrap();
    let socket = dir.path().join("supervisor.sock");
    let listener = tokio::net::UnixListener::bind(&socket).unwrap();
    let service = app.clone();
    let supervisor_task = tokio::spawn(async move {
        axum::serve(listener, server::router(service))
            .await
            .unwrap();
    });
    let client = reqwest::Client::builder()
        .unix_socket(socket)
        .build()
        .unwrap();
    let start = serde_json::from_value(json!({"version":1,"profile":"native","role":"wren","cwd":dir.path(),"credential_file":credential,"primary":true,"native_session_id":"native-original"})).unwrap();
    Fixture {
        _dir: dir,
        app,
        client,
        start,
        tasks: vec![identity_task, supervisor_task],
    }
}
async fn post(f: &Fixture, path: &str, body: Value) -> (reqwest::StatusCode, Value) {
    let response = f
        .client
        .post(format!("http://localhost{path}"))
        .json(&body)
        .send()
        .await
        .unwrap();
    (response.status(), response.json().await.unwrap())
}
struct EnvRestore(Vec<(&'static str, Option<String>)>);
impl Drop for EnvRestore {
    fn drop(&mut self) {
        for (name, value) in &self.0 {
            if let Some(value) = value {
                std::env::set_var(name, value);
            } else {
                std::env::remove_var(name);
            }
        }
    }
}

#[tokio::test]
async fn managed_terminal_ingress_cannot_claim_delivery_and_role_cannot_approve() {
    let f = fixture().await;
    let session = f.app.start(f.start.clone()).await.unwrap();
    {
        let mut store = f.app.store.lock().await;
        let mut s = store.get(&session.session_id).unwrap();
        s.mode = Mode::Managed;
        s.state = State::Running;
        s.message_receipts
            .insert("pulse:9".into(), "transport_accepted".into());
        store.update(s).unwrap();
    }
    let event = store::event(&session.session_id, "turn.completed", json!({}));
    let (code, _) = post(&f, "/v1/events", serde_json::to_value(event).unwrap()).await;
    assert_eq!(code, 200);
    let state = f.app.store.lock().await.get(&session.session_id).unwrap();
    assert_eq!(state.state, State::Running);
    assert_eq!(state.message_receipts["pulse:9"], "transport_accepted");
    let event = store::event(
        &session.session_id,
        "tool.completed",
        json!({"runtime":"gemini"}),
    );
    assert_eq!(
        post(&f, "/v1/events", serde_json::to_value(event).unwrap())
            .await
            .0,
        400
    );
    let approval = ApprovalRequest {
        credential_file: f.start.credential_file.clone(),
        request_id: "req".into(),
        decision: Some("once".into()),
        option_id: None,
    };
    assert!(f
        .app
        .approve(&session.session_id, approval)
        .await
        .unwrap_err()
        .contains("role"));
}

#[tokio::test]
async fn uds_boundary_preserves_pulse_claim_and_requires_successful_ack_before_delivery() {
    let f = fixture().await;
    let session = f.app.start(f.start.clone()).await.unwrap();
    {
        let mut store = f.app.store.lock().await;
        let mut s = store.get(&session.session_id).unwrap();
        s.pending_context =
            vec!["handoff obligation: preserve this until stdout is acknowledged".into()];
        store.update(s).unwrap();
    }
    let received = Arc::new(Mutex::new(Vec::<Value>::new()));
    let acknowledged = Arc::new(AtomicBool::new(false));
    let refuse_ack = Arc::new(AtomicBool::new(false));
    let pulse = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = pulse.local_addr().unwrap();
    let expected_id = session.session_id.clone();
    let handler = {
        let received = received.clone();
        let acknowledged = acknowledged.clone();
        let refuse_ack = refuse_ack.clone();
        move |axum::extract::Path(action): axum::extract::Path<String>,
              headers: axum::http::HeaderMap,
              axum::Json(body): axum::Json<Value>| {
            let received = received.clone();
            let acknowledged = acknowledged.clone();
            let refuse_ack = refuse_ack.clone();
            let expected_id = expected_id.clone();
            async move {
                if headers
                    .get("x-chorus-pulse-secret")
                    .and_then(|v| v.to_str().ok())
                    != Some("boundary-shared-secret")
                {
                    return (
                        axum::http::StatusCode::FORBIDDEN,
                        axum::Json(json!({"error":"unauthorized"})),
                    );
                }
                assert_eq!(body["role"], "wren");
                assert_eq!(body["session_id"], expected_id);
                received
                    .lock()
                    .await
                    .push(json!({"action":action,"body":body}));
                if action == "claim" {
                    let messages = if acknowledged.load(Ordering::SeqCst) {
                        json!([])
                    } else {
                        json!([{"id":41,"message_id":"pulse:41","from":"silas","content":"Peer evidence 🌍\nkeep line two", "kind":"peer_message"}])
                    };
                    (
                        axum::http::StatusCode::OK,
                        axum::Json(json!({"ok":true,"messages":messages})),
                    )
                } else {
                    if refuse_ack.load(Ordering::SeqCst) {
                        return (
                            axum::http::StatusCode::SERVICE_UNAVAILABLE,
                            axum::Json(json!({"error":"fixture outage"})),
                        );
                    }
                    assert_eq!(body["ids"], json!([41]));
                    let changed = !acknowledged.swap(true, Ordering::SeqCst);
                    (
                        axum::http::StatusCode::OK,
                        axum::Json(
                            json!({"ok":true,"acknowledged":if changed {1} else {0},"status":"context_delivered"}),
                        ),
                    )
                }
            }
        }
    };
    let pulse_task = tokio::spawn(async move {
        axum::serve(
            pulse,
            axum::Router::new().route("/api/agent-inbox/{action}", axum::routing::post(handler)),
        )
        .await
        .unwrap();
    });
    let _env = EnvRestore(
        ["CHORUS_PULSE_BASE_URL", "CHORUS_PULSE_SECRET"]
            .into_iter()
            .map(|name| (name, std::env::var(name).ok()))
            .collect(),
    );
    std::env::set_var(
        "CHORUS_PULSE_BASE_URL",
        format!("http://{address}/api/nudge"),
    );
    std::env::set_var("CHORUS_PULSE_SECRET", "wrong-secret");
    let path = format!("/v1/sessions/{}/boundary", session.session_id);
    let (_, rejected) = post(&f, &path, json!({"native_session_id":"native-original"})).await;
    assert_eq!(rejected["messages"], json!([]));
    assert!(rejected["delivery_error"].as_str().unwrap().contains("403"));
    assert!(received.lock().await.is_empty());
    std::env::set_var("CHORUS_PULSE_SECRET", "boundary-shared-secret");
    let (status, first) = post(&f, &path, json!({"native_session_id":"native-original"})).await;
    assert!(status.is_success());
    assert_eq!(
        first["messages"][0]["content"],
        "Peer evidence 🌍\nkeep line two"
    );
    let (_, repeated) = post(&f, &path, json!({"native_session_id":"native-original"})).await;
    assert_eq!(repeated["messages"], first["messages"]);
    assert!(!acknowledged.load(Ordering::SeqCst));
    assert!(!f
        .app
        .store
        .lock()
        .await
        .events(&session.session_id, 0)
        .unwrap()
        .iter()
        .any(|e| e.event_type == "context.delivered"));
    let ack = format!("/v1/sessions/{}/ack", session.session_id);
    refuse_ack.store(true, Ordering::SeqCst);
    assert!(!post(
        &f,
        &ack,
        json!({"ids":[41],"context_token":first["context_token"]})
    )
    .await
    .0
    .is_success());
    assert_eq!(
        f.app
            .store
            .lock()
            .await
            .get(&session.session_id)
            .unwrap()
            .pending_context
            .len(),
        1
    );
    refuse_ack.store(false, Ordering::SeqCst);
    let (status, receipt) = post(
        &f,
        &ack,
        json!({"ids":[41],"context_token":first["context_token"]}),
    )
    .await;
    assert!(status.is_success());
    assert_eq!(receipt["status"], "context_delivered");
    assert!(f
        .app
        .store
        .lock()
        .await
        .get(&session.session_id)
        .unwrap()
        .pending_context
        .is_empty());
    assert_eq!(
        post(&f, &ack, json!({"ids":[41]})).await.1["acknowledged"],
        0
    );
    assert_eq!(
        post(&f, &path, json!({"native_session_id":"native-original"}))
            .await
            .1["messages"],
        json!([])
    );
    pulse_task.abort();
}

#[tokio::test]
async fn concurrent_primary_start_resume_and_handoff_keep_exactly_one_live_primary() {
    let f = fixture().await;
    let (a, b) = tokio::join!(f.app.start(f.start.clone()), f.app.start(f.start.clone()));
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let original = a.or(b).unwrap();
    f.app
        .record_event(store::event(
            &original.session_id,
            "session.disconnected",
            json!({}),
        ))
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        f.app.resume(&original.session_id),
        f.app.resume(&original.session_id)
    );
    assert_eq!(usize::from(a.is_ok()) + usize::from(b.is_ok()), 1);
    let mut replacement = f.start.clone();
    replacement.native_session_id = None;
    let body = json!({"replacement":replacement,"context":"Finish the card; branch evidence and open obligations are preserved here."});
    let path = format!("/v1/sessions/{}/handoff", original.session_id);
    let (a, b) = tokio::join!(post(&f, &path, body.clone()), post(&f, &path, body));
    assert_eq!(
        usize::from(a.0.is_success()) + usize::from(b.0.is_success()),
        1
    );
    let store = f.app.store.lock().await;
    let live = store
        .sessions
        .values()
        .filter(|s| s.role == "wren" && s.primary && s.live())
        .collect::<Vec<_>>();
    assert_eq!(live.len(), 1);
    assert_ne!(live[0].session_id, original.session_id);
    assert_eq!(live[0].pending_context.len(), 1);
    assert_eq!(
        store.get(&original.session_id).unwrap().state,
        State::Stopped
    );
}

#[tokio::test]
async fn resume_cannot_reacquire_native_conversation_already_owned_by_replacement() {
    let f = fixture().await;
    let original = f.app.start(f.start.clone()).await.unwrap();
    f.app.stop(&original.session_id, true).await.unwrap();
    let replacement = f.app.start(f.start.clone()).await.unwrap();
    assert_ne!(original.session_id, replacement.session_id);
    assert!(
        f.app.resume(&original.session_id).await.is_err(),
        "resume must refuse a native conversation leased to the replacement"
    );
    assert_eq!(
        f.app
            .store
            .lock()
            .await
            .sessions
            .values()
            .filter(|s| s.live() && s.native_session_id.as_deref() == Some("native-original"))
            .count(),
        1
    );
}
