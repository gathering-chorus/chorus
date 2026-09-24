use crate::{
    config::{self, Config},
    contract::*,
    execution::{self, Worker},
    store::{self, Store},
    Result,
};
use axum::{
    extract::{DefaultBodyLimit, Path, Query, State as AxState},
    http::StatusCode,
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, HashMap},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    time::Duration,
};
use tokio::sync::{mpsc, oneshot, watch, Mutex, Semaphore};

struct TurnControl {
    message_id: String,
    cancel: Option<oneshot::Sender<()>>,
    finished: watch::Receiver<bool>,
}

pub struct Supervisor {
    config: RwLock<Config>,
    config_file: PathBuf,
    pub store: Mutex<Store>,
    workers: Mutex<HashMap<String, Arc<Worker>>>,
    observations: RwLock<HashMap<String, Arc<AtomicBool>>>,
    cancels: Mutex<HashMap<String, TurnControl>>,
    lifecycle: Mutex<()>,
    jobs: Arc<Semaphore>,
    client: reqwest::Client,
    identity_url: String,
    accepting: AtomicBool,
}
impl Supervisor {
    pub fn new(config: Config, store: Store, identity_url: String) -> Result<Arc<Self>> {
        Self::new_with_config_path(config, store, identity_url, config::config_path())
    }
    pub fn new_with_config_path(
        config: Config,
        store: Store,
        identity_url: String,
        config_file: PathBuf,
    ) -> Result<Arc<Self>> {
        let url = reqwest::Url::parse(&identity_url).map_err(|_| "invalid identity URL")?;
        if !matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"))
            && url.scheme() != "https"
        {
            return Err("identity verification requires loopback or TLS".into());
        }
        let jobs = Arc::new(Semaphore::new(config.max_concurrent_jobs));
        Ok(Arc::new(Self {
            config: RwLock::new(config),
            config_file,
            store: Mutex::new(store),
            workers: Mutex::new(HashMap::new()),
            observations: RwLock::new(HashMap::new()),
            cancels: Mutex::new(HashMap::new()),
            lifecycle: Mutex::new(()),
            jobs,
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|e| e.to_string())?,
            identity_url,
            accepting: AtomicBool::new(true),
        }))
    }
    /// Reload an operator-owned snapshot without interrupting existing sessions.
    /// The caller must atomically write the configured file before using the UDS endpoint.
    pub async fn reload_config(&self, candidate: Config) -> Result<Value> {
        config::validate(&candidate)?;
        let _transition = self.lifecycle.lock().await;
        if !self.accepting.load(Ordering::SeqCst) {
            return Err("supervisor admissions are paused".into());
        }
        let current = self.config_snapshot();
        if candidate.max_concurrent_jobs != current.max_concurrent_jobs {
            return Err("concurrency changes require a drained supervisor restart".into());
        }
        let store = self.store.lock().await;
        for session in store.sessions.values().filter(|s| s.live()) {
            let replacement = candidate.profiles.get(&session.profile);
            if replacement.is_none_or(|p| config::hash(p) != session.profile_hash) {
                return Err(format!("profile {} is bound to live session {}; create a new profile and hand off before replacing it", session.profile, session.session_id));
            }
            if candidate.role_workspaces.get(&session.role)
                != current.role_workspaces.get(&session.role)
            {
                return Err(format!("role workspace is bound to live session {}; stop and reconcile before changing it", session.session_id));
            }
            if session.card.is_some() && candidate.worktree_base != current.worktree_base {
                return Err(format!("worktree base is bound to live card session {}; stop and reconcile before changing it", session.session_id));
            }
        }
        *self
            .config
            .write()
            .expect("profile configuration lock poisoned") = candidate;
        Ok(
            json!({"ok":true,"version":VERSION,"reloaded":true,"sessions_preserved":store.sessions.values().filter(|s|s.live()).count()}),
        )
    }
    pub async fn public_status(&self, session: &Session) -> Value {
        let mut value = session.public();
        let mut blockers = session.switch_blockers();
        if self.cancels.lock().await.contains_key(&session.session_id) {
            blockers.push("previous turn is still stopping; wait for cleanup".into());
        }
        if !self.accepting.load(Ordering::SeqCst) {
            blockers.push("supervisor admissions are paused".into());
        }
        value["switch_ready"] = json!(blockers.is_empty());
        value["switch_blockers"] = json!(blockers);
        value
    }
    pub async fn authenticate(&self, file: &str, role: &str) -> Result<Identity> {
        let path = PathBuf::from(file);
        let mut credential = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|_| "identity credential file unavailable")?;
        let metadata = credential
            .metadata()
            .map_err(|_| "identity credential file unavailable")?;
        if !metadata.is_file()
            || metadata.mode() & 0o077 != 0
            || metadata.uid() != unsafe { libc::geteuid() }
            || metadata.len() > 16384
        {
            return Err("identity credential must be an owner-only regular file".into());
        }
        let mut token = String::new();
        credential
            .by_ref()
            .take(16385)
            .read_to_string(&mut token)
            .map_err(|_| "identity credential unreadable")?;
        if token.len() > 16384 {
            return Err("identity credential exceeds limit".into());
        }
        let token = token.trim();
        if token.is_empty() {
            return Err("identity credential is empty".into());
        }
        let response = self
            .client
            .post(&self.identity_url)
            .bearer_auth(token)
            .json(&json!({}))
            .send()
            .await
            .map_err(|_| "identity verification unavailable")?;
        if !response.status().is_success() {
            return Err(format!("identity refused ({})", response.status().as_u16()));
        }
        let identity: Identity = response
            .json()
            .await
            .map_err(|_| "invalid verified identity response")?;
        if identity.principal.is_empty() || identity.role != role {
            return Err("verified identity does not hold requested role".into());
        }
        Ok(identity)
    }
    async fn auth_session(&self, session: &Session) -> Result<()> {
        let identity = self
            .authenticate(
                session
                    .credential_file
                    .as_deref()
                    .ok_or("session has no credential")?,
                &session.role,
            )
            .await?;
        if identity.principal != session.principal {
            return Err("session credential principal changed".into());
        }
        Ok(())
    }
    pub fn config_snapshot(&self) -> Config {
        self.config
            .read()
            .expect("profile configuration lock poisoned")
            .clone()
    }
    fn profile(&self, name: &str) -> Result<Profile> {
        self.config_snapshot()
            .profiles
            .get(name)
            .cloned()
            .ok_or_else(|| "unknown operator profile".into())
    }
    pub async fn start(self: &Arc<Self>, request: StartRequest) -> Result<Session> {
        let _transition = self.lifecycle.lock().await;
        self.start_reserved(request).await
    }
    async fn start_reserved(self: &Arc<Self>, request: StartRequest) -> Result<Session> {
        if !self.accepting.load(Ordering::SeqCst) {
            return Err("supervisor admissions are paused".into());
        }
        if request.version != VERSION {
            return Err("unsupported protocol major".into());
        }
        if !["wren", "silas", "kade", "jeff"].contains(&request.role.as_str()) {
            return Err("unknown role".into());
        }
        let config = self.config_snapshot();
        let profile = self.profile(&request.profile)?;
        if profile.no_tools {
            return Err("inference profiles cannot enroll interactive sessions".into());
        }
        let cwd =
            std::fs::canonicalize(&request.cwd).map_err(|_| "workspace directory unavailable")?;
        if !cwd.is_dir() {
            return Err("workspace must be a directory".into());
        }
        if let Some(card) = request.card {
            let base = config
                .worktree_base
                .as_ref()
                .ok_or("card enrollment requires operator worktree_base")?;
            let base = std::fs::canonicalize(base).map_err(|_| "worktree base unavailable")?;
            if cwd.parent() != Some(base.as_path())
                || cwd.file_name().and_then(|s| s.to_str())
                    != Some(format!("{}-{card}", request.role).as_str())
            {
                return Err("card workspace must match explicit role/card worktree binding".into());
            }
        } else if let Some(anchor) = config.role_workspaces.get(&request.role) {
            if std::fs::canonicalize(anchor).map_err(|_| "role anchor unavailable")? != cwd {
                return Err("non-card session must use configured role anchor".into());
            }
        }
        if request
            .native_session_id
            .as_ref()
            .is_some_and(|id| id.is_empty() || id.len() > 512)
        {
            return Err("invalid native conversation id".into());
        }
        let identity = self
            .authenticate(&request.credential_file, &request.role)
            .await?;
        if let Some(parent) = &request.parent_session_id {
            let parent = self.store.lock().await.get(parent)?;
            if parent.principal != identity.principal
                || parent.role != request.role
                || request.primary
            {
                return Err("invalid subagent identity or primary lease request".into());
            }
        }
        if profile.runtime == Runtime::Opencode {
            let store = self.store.lock().await;
            if store.sessions.values().any(|s| {
                s.live()
                    && config.profiles.get(&s.profile).is_some_and(|other| {
                        other.runtime == Runtime::Opencode && other.endpoint == profile.endpoint
                    })
            }) {
                return Err("OpenCode server endpoints must be dedicated to one live session for MCP identity isolation".into());
            }
        }
        let (runtime_version, observed) = execution::probe_in(&profile, &cwd).await?;
        let capabilities = if profile.enforcement == Enforcement::Verified {
            let proof = profile
                .conformance
                .as_ref()
                .ok_or("verified profile lacks proof")?;
            if proof.runtime_version != runtime_version
                || proof.adapter_version != env!("CARGO_PKG_VERSION")
            {
                return Err(
                    "conformance attestation does not match installed runtime/adapter".into(),
                );
            }
            if !proof.capabilities.gaps.is_empty() {
                return Err("verified attestation contains unresolved gaps".into());
            }
            if !proof.capabilities.before_tool
                || !proof.capabilities.history_recovery
                || (profile.mode == Mode::Managed && !proof.capabilities.cancellation)
            {
                return Err("verified profile lacks required policy, history or managed cancellation capabilities".into());
            }
            proof.capabilities.clone()
        } else {
            if observed
                .gaps
                .iter()
                .any(|g| !profile.approved_gaps.contains(g))
            {
                return Err(format!(
                    "operator has not approved runtime gaps: {}",
                    observed.gaps.join("; ")
                ));
            }
            observed
        };
        let session = Session {
            version: VERSION,
            session_id: crate::id(),
            principal: identity.principal,
            role: request.role,
            parent_session_id: request.parent_session_id,
            profile: request.profile,
            runtime: profile.runtime.clone(),
            runtime_version,
            adapter_version: env!("CARGO_PKG_VERSION").into(),
            mode: profile.mode.clone(),
            enforcement: profile.enforcement.clone(),
            capabilities,
            native_session_id: request.native_session_id,
            model: request.model.or(profile.model.clone()),
            provider: profile.provider.clone(),
            cwd: cwd.to_string_lossy().into(),
            card: request.card,
            primary: request.primary,
            state: State::Idle,
            cleanly_detached: false,
            created_at: crate::now(),
            heartbeat: crate::now(),
            profile_hash: config::hash(&profile),
            credential_file: Some(request.credential_file),
            message_receipts: BTreeMap::new(),
            message_hashes: BTreeMap::new(),
            pending_context: Vec::new(),
            pending_approvals: BTreeMap::new(),
            last_event_sequence: 0,
        };
        self.store.lock().await.insert(session.clone())?;
        let event = store::event(&session.session_id, "session.registered", session.public());
        self.store.lock().await.append(event)?;
        if session.mode == Mode::Managed
            && !matches!(session.runtime, Runtime::Claude | Runtime::Codex)
        {
            if let Err(error) = self.start_worker(&session, false).await {
                let mut failed = session.clone();
                failed.state = State::Failed;
                self.store.lock().await.update(failed)?;
                return Err(error);
            }
        }
        self.store.lock().await.get(&session.session_id)
    }
    fn event_sink(self: &Arc<Self>, id: String, turn: Option<String>) -> execution::EventSink {
        let (tx, mut rx) = mpsc::channel::<Value>(256);
        let active = Arc::new(AtomicBool::new(true));
        if let Some(previous) = self
            .observations
            .write()
            .expect("observation lock poisoned")
            .insert(id.clone(), active.clone())
        {
            previous.store(false, Ordering::SeqCst);
        }
        let app = self.clone();
        tokio::spawn(async move {
            while let Some(raw) = rx.recv().await {
                let mut event = store::event(
                    &id,
                    raw["type"].as_str().unwrap_or("adapter.unknown"),
                    raw.get("data").cloned().unwrap_or_else(|| json!({})),
                );
                event.native_session_id = raw["native_session_id"].as_str().map(str::to_owned);
                event.turn_id = raw["turn_id"]
                    .as_str()
                    .map(str::to_owned)
                    .or_else(|| turn.clone());
                event.tool_call_id = raw["tool_call_id"].as_str().map(str::to_owned);
                if let Err(error) = app.record_observation(event, Some(&active)).await {
                    eprintln!("agent event persistence failed: {error}");
                    if let Some(control) = app.cancels.lock().await.get_mut(&id) {
                        if let Some(cancel) = control.cancel.take() {
                            let _ = cancel.send(());
                        }
                    }
                    let mut store = app.store.lock().await;
                    if let Ok(mut session) = store.get(&id) {
                        session.state = State::Failed;
                        let _ = store.update(session);
                    }
                    break;
                }
            }
        });
        tx
    }
    async fn start_worker(self: &Arc<Self>, s: &Session, resume: bool) -> Result<()> {
        let profile = self.profile(&s.profile)?;
        let mut env = vec![
            ("CHORUS_SESSION_ID", s.session_id.as_str()),
            ("CHORUS_ROLE", s.role.as_str()),
            ("DEPLOY_ROLE", s.role.as_str()),
        ];
        if let Some(file) = s.credential_file.as_deref() {
            env.push(("CHORUS_SESSION_TOKEN_FILE", file));
        }
        let worker = Worker::start_with_env(
            &profile,
            Some(std::path::Path::new(&s.cwd)),
            Some(self.event_sink(s.session_id.clone(), None)),
            &env,
        )
        .await?;
        let mut settings = serde_json::to_value(&profile.adapter_config).unwrap();
        settings["command"] = json!(execution::executable(&profile));
        settings["env_refs"] = json!({"CHORUS_SESSION_ID":"CHORUS_SESSION_ID","CHORUS_ROLE":"CHORUS_ROLE","CHORUS_SESSION_TOKEN_FILE":"CHORUS_SESSION_TOKEN_FILE"});
        settings["no_tools"] = json!(false);
        let result = worker.request(if resume {"resume"} else {"start"}, json!({"cwd":s.cwd,"model":s.model,"native_session_id":s.native_session_id,"endpoint":profile.endpoint,"config":settings}), 30).await;
        match result {
            Ok(result) => {
                if let Some(native) = result["native_session_id"].as_str() {
                    let mut current = self.store.lock().await.get(&s.session_id)?;
                    current.native_session_id = Some(native.into());
                    self.store.lock().await.update(current)?;
                }
                self.workers
                    .lock()
                    .await
                    .insert(s.session_id.clone(), worker);
                Ok(())
            }
            Err(e) => {
                worker.stop().await;
                Err(e)
            }
        }
    }
    pub async fn record_event(&self, event: Event) -> Result<Event> {
        self.record_observation(event, None).await
    }
    async fn record_observation(&self, event: Event, active: Option<&AtomicBool>) -> Result<Event> {
        let mut store = self.store.lock().await;
        // A stopped/replaced worker may still have buffered events. Check its epoch
        // under the same store lock used to commit detach so it cannot resurrect state.
        if active.is_some_and(|active| !active.load(Ordering::SeqCst)) {
            return Ok(event);
        }
        let mut session = store.get(&event.session_id)?;
        if let Some(native) = &event.native_session_id {
            if session
                .native_session_id
                .as_ref()
                .is_some_and(|old| old != native)
            {
                return Err("native session identity changed without handoff".into());
            }
            if store.sessions.values().any(|other| {
                other.session_id != session.session_id
                    && other.live()
                    && other.runtime == session.runtime
                    && other.native_session_id.as_ref() == Some(native)
            }) {
                return Err("native conversation already belongs to another session".into());
            }
            session.native_session_id = Some(native.clone());
        }
        let event = store.append(event)?;
        // The snapshot cursor prevents duplicate transitions and lets restart replay
        // an authoritative event whose snapshot write was interrupted.
        session.apply_event(&event)?;
        store.update(session)?;
        Ok(event)
    }
    pub async fn send(self: &Arc<Self>, id: &str, req: SendRequest) -> Result<Value> {
        let _transition = self.lifecycle.lock().await;
        if !self.accepting.load(Ordering::SeqCst) {
            return Err("supervisor admissions are paused".into());
        }
        if req.version != VERSION
            || req.message_id.is_empty()
            || req.message_id.len() > 200
            || !req
                .message_id
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
            || req.input.is_empty()
            || req.input.len() > MAX_INPUT_BYTES
        {
            return Err("invalid input envelope".into());
        }
        if !["human_input", "peer_message", "context"].contains(&req.kind.as_str()) {
            return Err("unknown input kind".into());
        }
        let s = self.store.lock().await.get(id)?;
        self.auth_session(&s).await?;
        let profile = self.profile(&s.profile)?;
        if config::hash(&profile) != s.profile_hash {
            return Err("profile changed; explicitly hand off session".into());
        }
        if req.input.len() + s.pending_context.iter().map(|s| s.len() + 2).sum::<usize>()
            > MAX_INPUT_BYTES
        {
            return Err("input plus pending handoff exceeds context envelope budget".into());
        }
        let message_hash = crate::digest(&serde_json::to_vec(&req).unwrap());
        if let Some(receipt) = s.message_receipts.get(&req.message_id) {
            if s.message_hashes.get(&req.message_id) != Some(&message_hash) {
                return Err("message id reused with different content".into());
            }
            return Ok(json!({"status":receipt,"duplicate":true}));
        }
        if !s.live() {
            return Err("session has stopped".into());
        }
        if s.message_receipts
            .values()
            .any(|receipt| receipt == "uncertain")
        {
            return Err(
                "session has uncertain delivery; reconcile it before submitting new work".into(),
            );
        }
        if s.mode == Mode::Native {
            return Ok(json!({"status":"queued","reason":"native_boundary","persisted":false}));
        }
        if self.cancels.lock().await.contains_key(id) {
            return Ok(
                json!({"status":"queued","reason":"previous_turn_cleanup","persisted":false}),
            );
        }
        let permit = self
            .jobs
            .clone()
            .try_acquire_owned()
            .map_err(|_| "agent concurrency limit reached")?;
        {
            let mut store = self.store.lock().await;
            let mut current = store.get(id)?;
            if let Some(receipt) = current.message_receipts.get(&req.message_id) {
                if current.message_hashes.get(&req.message_id) != Some(&message_hash) {
                    return Err("message id reused with different content".into());
                }
                return Ok(json!({"status":receipt,"duplicate":true}));
            }
            if current.state != State::Idle {
                return Ok(
                    json!({"status":"queued","reason":"session_not_idle","persisted":false}),
                );
            }
            current.state = State::Running;
            current
                .message_receipts
                .insert(req.message_id.clone(), "transport_accepted".into());
            current
                .message_hashes
                .insert(req.message_id.clone(), message_hash);
            store.update(current)?;
            let mut event = store::event(
                id,
                "input.accepted",
                json!({"kind":req.kind,"text":req.input}),
            );
            event.message_id = Some(req.message_id.clone());
            store.append(event)?;
        }
        let mut req = req;
        if !s.pending_context.is_empty() {
            req.input = format!("{}\n\n{}", s.pending_context.join("\n\n"), req.input);
        }
        let app = self.clone();
        let session_id = id.to_string();
        let receipt_id = req.message_id.clone();
        let (tx, rx) = oneshot::channel();
        let (finished, completion) = watch::channel(false);
        self.cancels.lock().await.insert(
            id.into(),
            TurnControl {
                message_id: req.message_id.clone(),
                cancel: Some(tx),
                finished: completion,
            },
        );
        tokio::spawn(async move {
            let _permit = permit;
            let result = if matches!(profile.runtime, Runtime::Claude | Runtime::Codex) {
                execution::run_cli(
                    &profile,
                    &s,
                    &req.input,
                    app.event_sink(session_id.clone(), Some(req.message_id.clone())),
                    rx,
                )
                .await
            } else {
                let worker = app.workers.lock().await.get(&session_id).cloned();
                if let Some(worker) = worker {
                    let turn = async {
                        worker.request("send",json!({"native_session_id":s.native_session_id,"input":req.input,"turn_id":req.message_id}),profile.timeout_secs).await?;
                        // Admission is not completion. Keep the slot until terminal evidence.
                        loop {
                            tokio::time::sleep(Duration::from_millis(50)).await;
                            let state = app.store.lock().await.get(&session_id)?.state;
                            if state == State::Idle {
                                return Ok(());
                            }
                            if matches!(state, State::Failed | State::Stopped | State::Disconnected)
                            {
                                return Err("adapter turn failed or disconnected".into());
                            }
                        }
                    };
                    tokio::select! {
                        result=tokio::time::timeout(Duration::from_secs(profile.timeout_secs),turn)=>result.unwrap_or_else(|_|Err("adapter turn deadline exceeded".into())),
                        _=rx=>Err("adapter turn cancelled".into()),
                    }
                } else {
                    Err("worker disconnected; explicitly resume session".into())
                }
            };
            if let Err(error) = result {
                if let Some(worker) = app.workers.lock().await.get(&session_id).cloned() {
                    let _ = worker
                        .request(
                            "cancel",
                            json!({"native_session_id":s.native_session_id}),
                            5,
                        )
                        .await;
                }
                let _ = app
                    .record_event(store::event(
                        &session_id,
                        "turn.failed",
                        json!({"reason":error}),
                    ))
                    .await;
            }
            let _ = finished.send(true);
            let mut cancels = app.cancels.lock().await;
            if cancels
                .get(&session_id)
                .is_some_and(|control| control.message_id == req.message_id)
            {
                cancels.remove(&session_id);
            }
        });
        Ok(json!({"status":"transport_accepted","message_id":receipt_id}))
    }
    pub async fn resume(self: &Arc<Self>, id: &str) -> Result<Session> {
        let _transition = self.lifecycle.lock().await;
        if !self.accepting.load(Ordering::SeqCst) {
            return Err("supervisor admissions are paused".into());
        }
        if self.cancels.lock().await.contains_key(id) {
            return Err("previous turn is still stopping; reconcile before resume".into());
        }
        let mut session = self.store.lock().await.get(id)?;
        self.auth_session(&session).await?;
        if !matches!(
            session.state,
            State::Disconnected | State::Failed | State::Stopped
        ) {
            return Err("session is already active".into());
        }
        if session.native_session_id.is_none() {
            return Err(
                "session has no native history; create a fresh session with handoff".into(),
            );
        }
        if config::hash(&self.profile(&session.profile)?) != session.profile_hash {
            return Err("profile changed; create a fresh session".into());
        }
        {
            let store = self.store.lock().await;
            if session.primary
                && store
                    .sessions
                    .values()
                    .any(|s| s.session_id != id && s.primary && s.role == session.role && s.live())
            {
                return Err("role primary lease is held by another session".into());
            }
            if store.sessions.values().any(|other| {
                other.session_id != id
                    && other.live()
                    && other.runtime == session.runtime
                    && other.native_session_id == session.native_session_id
            }) {
                return Err("native conversation is already bound to another session".into());
            }
        }
        if session.mode == Mode::Managed
            && !matches!(session.runtime, Runtime::Claude | Runtime::Codex)
        {
            if let Some(worker) = self.workers.lock().await.remove(id) {
                worker.stop().await;
            }
            self.start_worker(&session, true).await?;
        }
        let mut store = self.store.lock().await;
        session = store.get(id)?;
        session.cleanly_detached = false;
        session.state = State::Idle;
        session.heartbeat = crate::now();
        store.update(session.clone())?;
        Ok(session)
    }
    pub async fn enqueue_context(&self, id: &str, text: String) -> Result<Value> {
        let _transition = self.lifecycle.lock().await;
        let session = self.store.lock().await.get(id)?;
        self.auth_session(&session).await?;
        if self.cancels.lock().await.contains_key(id) {
            return Err("context requires an idle session with no turn cleanup".into());
        }
        let mut store = self.store.lock().await;
        let mut session = store.get(id)?;
        if session.state != State::Idle {
            return Err("context requires an idle session with no turn cleanup".into());
        }
        if text.trim().is_empty()
            || text.len()
                + session
                    .pending_context
                    .iter()
                    .map(|s| s.len() + 2)
                    .sum::<usize>()
                > MAX_INPUT_BYTES / 2
        {
            return Err("context must be nonempty and fit the pending context budget".into());
        }
        let event = store.append(store::event(id, "context.enqueued", json!({"text":text})))?;
        session.apply_event(&event)?;
        store.update(session)?;
        Ok(json!({"ok":true,"session_id":id,"status":"context_queued","model_called":false}))
    }
    /// Close an operator-owned transport without releasing its role lease or history.
    pub async fn disconnect(&self, id: &str) -> Result<Value> {
        let _transition = self.lifecycle.lock().await;
        let original = self.store.lock().await.get(id)?;
        self.auth_session(&original).await?;
        if !original.live() {
            return Err("session has stopped".into());
        }
        if original.mode == Mode::Native && original.state != State::Idle {
            return Err("finish native work using its client controls before disconnecting".into());
        }
        let completion = {
            let mut controls = self.cancels.lock().await;
            controls.get_mut(id).map(|control| {
                if let Some(cancel) = control.cancel.take() {
                    let _ = cancel.send(());
                }
                control.finished.clone()
            })
        };
        let settled = original.switch_blockers().is_empty() && completion.is_none();
        if let Some(mut completion) = completion {
            if !*completion.borrow() {
                if !matches!(
                    tokio::time::timeout(Duration::from_secs(8), completion.changed()).await,
                    Ok(Ok(()))
                ) || !*completion.borrow()
                {
                    return Err(
                        "turn is still stopping; session and lease retained for reconciliation"
                            .into(),
                    );
                }
            }
        }
        if let Some(worker) = self.workers.lock().await.remove(id) {
            if !settled {
                let _ = worker
                    .request(
                        "cancel",
                        json!({"native_session_id":original.native_session_id}),
                        5,
                    )
                    .await;
            }
            worker.stop().await;
        }
        let mut store = self.store.lock().await;
        if let Some(active) = self
            .observations
            .write()
            .expect("observation lock poisoned")
            .remove(id)
        {
            active.store(false, Ordering::SeqCst);
        }
        let mut session = store.get(id)?;
        let clean = settled
            && session.last_event_sequence == original.last_event_sequence
            && session.switch_blockers().is_empty();
        let event = store.append(store::event(
            id,
            "session.detached",
            json!({"clean":clean,"reason":"operator_transport_closed"}),
        ))?;
        session.apply_event(&event)?;
        store.update(session.clone())?;
        Ok(session.public())
    }
    pub async fn stop(&self, id: &str, terminate: bool) -> Result<Value> {
        let _transition = self.lifecycle.lock().await;
        let mut session = self.store.lock().await.get(id)?;
        self.auth_session(&session).await?;
        if session.mode == Mode::Native && !terminate {
            return Err(
                "native client cancellation is unavailable; use its own client controls".into(),
            );
        }
        let completion = {
            let mut controls = self.cancels.lock().await;
            controls.get_mut(id).map(|control| {
                if let Some(cancel) = control.cancel.take() {
                    let _ = cancel.send(());
                }
                control.finished.clone()
            })
        };
        if terminate {
            let worker = self.workers.lock().await.get(id).cloned();
            if let Some(worker) = worker {
                // Stop the native conversation before terminating its adapter process.
                if worker
                    .request(
                        "stop",
                        json!({"native_session_id":session.native_session_id}),
                        5,
                    )
                    .await
                    .is_err()
                {
                    session = self.store.lock().await.get(id)?;
                    session.state = State::Failed;
                    self.store.lock().await.update(session)?;
                    return Err("runtime stop outcome is uncertain; primary lease retained for reconciliation".into());
                }
                worker.stop().await;
                self.workers.lock().await.remove(id);
            }
            if let Some(mut completion) = completion {
                if !*completion.borrow() {
                    if !matches!(
                        tokio::time::timeout(Duration::from_secs(6), completion.changed()).await,
                        Ok(Ok(()))
                    ) || !*completion.borrow()
                    {
                        return Err("turn has not stopped; primary lease retained".into());
                    }
                }
            }
            session = self.store.lock().await.get(id)?;
            session.state = State::Stopped;
            session.primary = false;
            self.store.lock().await.update(session)?;
        }
        Ok(
            json!({"ok":true,"session_id":id,"status":if terminate {"registration_stopped"} else {"cancellation_requested"}}),
        )
    }
    pub async fn approve(&self, id: &str, req: ApprovalRequest) -> Result<Value> {
        let _transition = self.lifecycle.lock().await;
        let actor = self.authenticate(&req.credential_file, "jeff").await?;
        let session = self.store.lock().await.get(id)?;
        if session.state != State::AwaitingApproval
            || !session.pending_approvals.contains_key(&req.request_id)
        {
            return Err("approval is not pending for this session".into());
        }
        let worker = self
            .workers
            .lock()
            .await
            .get(id)
            .cloned()
            .ok_or("this runtime cannot accept remote approvals")?;
        let response=worker.request("approve",json!({"native_session_id":session.native_session_id,"request_id":req.request_id,"decision":req.decision,"option_id":req.option_id}),5).await?;
        self.record_event(store::event(id,"approval.resolved",json!({"request_id":req.request_id,"principal":actor.principal,"decision":req.decision,"option_id":req.option_id}))).await?;
        Ok(response)
    }
    pub async fn shutdown(&self) {
        self.accepting.store(false, Ordering::SeqCst);
        let _transition = self.lifecycle.lock().await;
        let active = self
            .store
            .lock()
            .await
            .sessions
            .values()
            .filter(|s| s.live())
            .map(|s| (s.session_id.clone(), s.primary))
            .collect::<Vec<_>>();
        let mut completions = Vec::new();
        for control in self.cancels.lock().await.values_mut() {
            if let Some(cancel) = control.cancel.take() {
                let _ = cancel.send(());
            }
            completions.push(control.finished.clone());
        }
        let workers = self.workers.lock().await.drain().collect::<Vec<_>>();
        for (id, worker) in workers {
            let native = self
                .store
                .lock()
                .await
                .get(&id)
                .ok()
                .and_then(|s| s.native_session_id);
            let _ = worker
                .request("stop", json!({"native_session_id":native}), 3)
                .await;
            worker.stop().await;
        }
        for completion in &mut completions {
            if !*completion.borrow() {
                let _ = tokio::time::timeout(Duration::from_secs(6), completion.changed()).await;
            }
        }
        let mut store = self.store.lock().await;
        for (id, primary) in active {
            let Ok(mut session) = store.get(&id) else {
                continue;
            };
            session.state = State::Disconnected;
            session.primary = primary;
            for receipt in session.message_receipts.values_mut() {
                if receipt == "transport_accepted" {
                    *receipt = "uncertain".into();
                }
            }
            let _ = store.update(session);
        }
    }
}

type App = Arc<Supervisor>;
type ApiResult = std::result::Result<Json<Value>, (StatusCode, Json<Value>)>;
fn error(message: String) -> (StatusCode, Json<Value>) {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error":{"code":"agent_refused","message":message}})),
    )
}
fn result(value: Result<Value>) -> ApiResult {
    value.map(Json).map_err(error)
}

pub fn router(app: App) -> Router {
    Router::new()
        .route(
            "/health",
            get(|| async { Json(json!({"ok":true,"version":VERSION})) }),
        )
        .route("/v1/config", get(config_status))
        .route("/v1/config/reload", post(reload))
        .route("/v1/sessions", get(list).post(start))
        .route("/v1/sessions/{id}", get(status))
        .route("/v1/sessions/{id}/send", post(send))
        .route("/v1/sessions/{id}/resume", post(resume))
        .route("/v1/sessions/{id}/cancel", post(cancel))
        .route("/v1/sessions/{id}/stop", post(stop))
        .route("/v1/sessions/{id}/disconnect", post(disconnect))
        .route("/v1/sessions/{id}/context", post(context))
        .route("/v1/sessions/{id}/handoff", post(handoff))
        .route("/v1/sessions/{id}/switch", post(switch))
        .route("/v1/sessions/{id}/boundary", post(boundary))
        .route("/v1/sessions/{id}/ack", post(ack))
        .route("/v1/sessions/{id}/approve", post(approve))
        .route("/v1/sessions/{id}/events", get(events))
        .route("/v1/sessions/{id}/receipts", get(receipts))
        .route("/v1/native-binding", post(native_binding))
        .route("/v1/profile-binding", post(profile_binding))
        .route("/v1/events", post(publish_event))
        .route("/v1/jobs", post(job))
        .route("/v1/profiles/{id}/probe", get(probe))
        .layer(DefaultBodyLimit::max(MAX_INPUT_BYTES))
        .with_state(app)
}
async fn config_status(AxState(app): AxState<App>) -> Json<Value> {
    let config = app.config_snapshot();
    let profiles = config.profiles.iter().map(|(name, p)| (name.clone(), json!({
        "runtime":p.runtime,"mode":p.mode,"model":p.model,"enforcement":p.enforcement,
        "approved_gaps":p.approved_gaps,"no_tools":p.no_tools,"profile_hash":config::hash(p)
    }))).collect::<serde_json::Map<_,_>>();
    Json(
        json!({"version":VERSION,"profiles":profiles,"roles":config.roles,"role_workspaces":config.role_workspaces,"worktree_base":config.worktree_base,"max_concurrent_jobs":config.max_concurrent_jobs}),
    )
}
async fn reload(AxState(app): AxState<App>) -> ApiResult {
    let candidate = config::read(&app.config_file).map_err(error)?;
    result(app.reload_config(candidate).await)
}
async fn list(AxState(app): AxState<App>) -> Json<Value> {
    let sessions = app
        .store
        .lock()
        .await
        .sessions
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let mut statuses = Vec::new();
    for session in sessions {
        statuses.push(app.public_status(&session).await);
    }
    Json(json!({"version":VERSION,"sessions":statuses}))
}
async fn start(AxState(app): AxState<App>, Json(req): Json<StartRequest>) -> ApiResult {
    result(app.start(req).await.map(|s| s.public()))
}
async fn status(AxState(app): AxState<App>, Path(id): Path<String>) -> ApiResult {
    let session = app.store.lock().await.get(&id).map_err(error)?;
    Ok(Json(app.public_status(&session).await))
}
async fn send(
    AxState(app): AxState<App>,
    Path(id): Path<String>,
    Json(req): Json<SendRequest>,
) -> ApiResult {
    result(app.send(&id, req).await)
}
async fn resume(AxState(app): AxState<App>, Path(id): Path<String>) -> ApiResult {
    result(app.resume(&id).await.map(|s| s.public()))
}
async fn cancel(AxState(app): AxState<App>, Path(id): Path<String>) -> ApiResult {
    result(app.stop(&id, false).await)
}
async fn stop(AxState(app): AxState<App>, Path(id): Path<String>) -> ApiResult {
    result(app.stop(&id, true).await)
}
async fn disconnect(AxState(app): AxState<App>, Path(id): Path<String>) -> ApiResult {
    result(app.disconnect(&id).await)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextRequest {
    text: String,
}
async fn context(
    AxState(app): AxState<App>,
    Path(id): Path<String>,
    Json(req): Json<ContextRequest>,
) -> ApiResult {
    result(app.enqueue_context(&id, req.text).await)
}
async fn approve(
    AxState(app): AxState<App>,
    Path(id): Path<String>,
    Json(req): Json<ApprovalRequest>,
) -> ApiResult {
    result(app.approve(&id, req).await)
}
#[derive(Deserialize)]
struct Cursor {
    #[serde(default)]
    after: u64,
}
async fn events(
    AxState(app): AxState<App>,
    Path(id): Path<String>,
    Query(cursor): Query<Cursor>,
) -> ApiResult {
    result(app.store.lock().await.events(&id,cursor.after).map(|events|{
        let count=events.len();let mut bytes=0;let mut page=Vec::new();
        for event in events{let size=serde_json::to_vec(&event).unwrap().len();if !page.is_empty()&&(page.len()>=100||bytes+size>MAX_INPUT_BYTES/2){break;}bytes+=size;page.push(event);}
        json!({"next_cursor":page.last().map(|e|e.sequence).unwrap_or(cursor.after),"has_more":page.len()<count,"events":page})
    }))
}
async fn receipts(AxState(app): AxState<App>, Path(id): Path<String>) -> ApiResult {
    result(
        app.store
            .lock()
            .await
            .get(&id)
            .map(|s| json!({"receipts":s.message_receipts})),
    )
}
async fn native_binding(AxState(app): AxState<App>, Json(body): Json<Value>) -> ApiResult {
    let runtime: Runtime = serde_json::from_value(body["runtime"].clone())
        .map_err(|_| error("invalid runtime".into()))?;
    let native = body["native_session_id"]
        .as_str()
        .ok_or_else(|| error("native_session_id required".into()))?;
    let store = app.store.lock().await;
    let matches = store
        .sessions
        .values()
        .filter(|s| {
            s.runtime == runtime && s.native_session_id.as_deref() == Some(native) && s.live()
        })
        .collect::<Vec<_>>();
    if matches.len() != 1 {
        return Err(error("native session binding absent or ambiguous".into()));
    }
    let s = matches[0];
    Ok(Json(
        json!({"session_id":s.session_id,"role":s.role,"cwd":s.cwd,"enforcement":s.enforcement,"capabilities":s.capabilities}),
    ))
}
async fn profile_binding(AxState(app): AxState<App>, Json(body): Json<Value>) -> ApiResult {
    let profile = body["profile"]
        .as_str()
        .ok_or_else(|| error("profile required".into()))?;
    let role = body["role"]
        .as_str()
        .ok_or_else(|| error("role required".into()))?;
    let store = app.store.lock().await;
    let found = store
        .sessions
        .values()
        .filter(|s| {
            s.profile == profile && s.role == role && s.runtime == Runtime::Opencode && s.live()
        })
        .collect::<Vec<_>>();
    if found.len() != 1 {
        return Err(error(
            "profile binding is absent or ambiguous; enroll one session first".into(),
        ));
    }
    Ok(Json(
        json!({"session_id":found[0].session_id,"role":found[0].role,"principal":found[0].principal}),
    ))
}
async fn publish_event(AxState(app): AxState<App>, Json(event): Json<Event>) -> ApiResult {
    let session = app
        .store
        .lock()
        .await
        .get(&event.session_id)
        .map_err(error)?;
    if let Some(runtime) = event.data["runtime"].as_str() {
        if serde_json::to_value(&session.runtime).unwrap() != runtime {
            return Err(error("hook runtime differs from enrolled runtime".into()));
        }
    }
    if ![
        "session.started",
        "session.disconnected",
        "session.ended",
        "turn.started",
        "turn.completed",
        "turn.failed",
        "tool.requested",
        "tool.completed",
        "tool.refused",
        "approval.required",
        "context.compacted",
        "message.completed",
    ]
    .contains(&event.event_type.as_str())
    {
        return Err(error("event type requires supervisor ownership".into()));
    }
    // Runtime hook observations are evidence. Managed completion comes from its owned adapter.
    if session.mode == Mode::Managed
        && matches!(
            event.event_type.as_str(),
            "turn.completed" | "turn.failed" | "session.ended" | "session.disconnected"
        )
    {
        let mut observed = event;
        observed.event_type = format!("hook.{}", observed.event_type);
        return result(
            app.record_event(observed)
                .await
                .map(|e| json!({"ok":true,"event_id":e.event_id,"sequence":e.sequence})),
        );
    }
    result(
        app.record_event(event)
            .await
            .map(|e| json!({"ok":true,"event_id":e.event_id,"sequence":e.sequence})),
    )
}
async fn boundary(
    AxState(app): AxState<App>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    let session = app.store.lock().await.get(&id).map_err(error)?;
    app.auth_session(&session).await.map_err(error)?;
    if !session.live() {
        return Err(error("session has stopped".into()));
    }
    let mut event = store::event(&id, "session.boundary", body.clone());
    if let Some(native) = body["native_session_id"].as_str() {
        event.native_session_id = Some(native.into());
    }
    app.record_event(event).await.map_err(error)?;
    let inbox = pulse_request(
        &app,
        "claim",
        json!({"session_id":id,"role":session.role,"limit":25}),
    )
    .await;
    let context_token = crate::digest(&serde_json::to_vec(&session.pending_context).unwrap());
    // Pulse outages never manufacture receipt. The hook can still deliver handoff context.
    match inbox {
        Ok(inbox) => Ok(Json(
            json!({"ok":true,"context":session.pending_context,"context_token":context_token,"messages":inbox["messages"],"delivery_owner":"pulse"}),
        )),
        Err(reason) => Ok(Json(
            json!({"ok":true,"context":session.pending_context,"context_token":context_token,"messages":[],"delivery_owner":"pulse","delivery_error":reason}),
        )),
    }
}
async fn pulse_request(app: &Supervisor, operation: &str, body: Value) -> Result<Value> {
    let secret = match std::env::var("CHORUS_PULSE_SECRET") {
        Ok(secret) if !secret.is_empty() => secret,
        _ => {
            let path = std::env::var_os("CHORUS_PULSE_SECRET_FILE")
                .map(PathBuf::from)
                .unwrap_or_else(|| config::root().join("pulse-nudge.secret"));
            std::fs::read_to_string(path)
                .map_err(|_| "Pulse credential unavailable")?
                .trim()
                .to_owned()
        }
    };
    if secret.is_empty() {
        return Err("Pulse credential unavailable".into());
    }
    let configured = std::env::var("CHORUS_PULSE_BASE_URL")
        .or_else(|_| std::env::var("CHORUS_PULSE_URL"))
        .unwrap_or_else(|_| "http://127.0.0.1:3475".into());
    let mut url = reqwest::Url::parse(&configured).map_err(|_| "invalid Pulse URL")?;
    if url.scheme() != "https"
        && !matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "[::1]"))
    {
        return Err("Pulse URL requires loopback or TLS".into());
    }
    url.set_path(&format!("/api/agent-inbox/{operation}"));
    url.set_query(None);
    url.set_fragment(None);
    let response = app
        .client
        .post(url)
        .header("X-Chorus-Pulse-Secret", secret)
        .json(&body)
        .send()
        .await
        .map_err(|_| "Pulse inbox unavailable")?;
    if !response.status().is_success() {
        return Err(format!("Pulse inbox refused ({})", response.status()));
    }
    response
        .json()
        .await
        .map_err(|_| "invalid Pulse inbox response".into())
}
async fn ack(
    AxState(app): AxState<App>,
    Path(id): Path<String>,
    Json(body): Json<Value>,
) -> ApiResult {
    let session = app.store.lock().await.get(&id).map_err(error)?;
    app.auth_session(&session).await.map_err(error)?;
    let ids = body["ids"]
        .as_array()
        .ok_or_else(|| error("ack ids required".into()))?;
    if ids.len() > 100 || ids.iter().any(|id| id.as_u64().is_none()) {
        return Err(error("invalid acknowledgement ids".into()));
    }
    let receipt = if ids.is_empty() {
        json!({"ok":true,"acknowledged":0})
    } else {
        pulse_request(
            &app,
            "ack",
            json!({"session_id":id,"role":session.role,"ids":ids}),
        )
        .await
        .map_err(error)?
    };
    if body["context_token"].as_str()
        == Some(crate::digest(&serde_json::to_vec(&session.pending_context).unwrap()).as_str())
    {
        let mut store = app.store.lock().await;
        let mut current = store.get(&id).map_err(error)?;
        if current.pending_context == session.pending_context {
            current.pending_context.clear();
            store.update(current).map_err(error)?;
        }
    }
    app.record_event(store::event(
        &id,
        "context.delivered",
        json!({"message_ids":ids,"boundary":"native_hook_stdout"}),
    ))
    .await
    .map_err(error)?;
    Ok(Json(receipt))
}
async fn job(AxState(app): AxState<App>, Json(req): Json<JobRequest>) -> ApiResult {
    let profile = app.profile(&req.profile).map_err(error)?;
    let _permit = app
        .jobs
        .clone()
        .try_acquire_owned()
        .map_err(|_| error("job concurrency limit reached".into()))?;
    result(
        execution::run_job(&profile, req)
            .await
            .map(|r| serde_json::to_value(r).unwrap()),
    )
}
async fn probe(AxState(app): AxState<App>, Path(id): Path<String>) -> ApiResult {
    let p = app.profile(&id).map_err(error)?;
    result(execution::probe(&p).await.map(|(v,c)|json!({"version":VERSION,"runtime":p.runtime,"runtime_version":v,"capabilities":c})))
}
async fn handoff(
    AxState(app): AxState<App>,
    Path(id): Path<String>,
    Json(request): Json<HandoffRequest>,
) -> ApiResult {
    let _transition = app.lifecycle.lock().await;
    handoff_reserved(&app, &id, request).await
}
async fn switch(
    AxState(app): AxState<App>,
    Path(id): Path<String>,
    Json(request): Json<SwitchRequest>,
) -> ApiResult {
    let _transition = app.lifecycle.lock().await;
    let old = app.store.lock().await.get(&id).map_err(error)?;
    let replacement = StartRequest {
        version: VERSION,
        profile: request.profile,
        role: old.role,
        cwd: old.cwd,
        credential_file: request
            .credential_file
            .or(old.credential_file)
            .ok_or_else(|| error("session credential unavailable".into()))?,
        primary: old.primary,
        card: old.card,
        parent_session_id: old.parent_session_id,
        native_session_id: None,
        model: None,
    };
    handoff_reserved(
        &app,
        &id,
        HandoffRequest {
            replacement,
            context: request.context,
        },
    )
    .await
}
async fn handoff_reserved(app: &App, id: &str, request: HandoffRequest) -> ApiResult {
    if app.cancels.lock().await.contains_key(id) {
        return Err(error(
            "previous turn is still stopping; reconcile before handoff".into(),
        ));
    }
    if request.context.trim().is_empty() || request.context.len() > MAX_INPUT_BYTES / 2 {
        return Err(error(
            "handoff requires bounded task state, evidence and open obligations".into(),
        ));
    }
    let mut req = request.replacement;
    let old = app.store.lock().await.get(id).map_err(error)?;
    app.auth_session(&old).await.map_err(error)?;
    let blockers = old.switch_blockers();
    if !blockers.is_empty() {
        return Err(error(format!("handoff refused: {}", blockers.join("; "))));
    }
    if req.role != old.role {
        return Err(error("handoff cannot change role".into()));
    }
    if req.native_session_id.is_some() {
        return Err(error(
            "cross-session handoff starts a fresh native conversation".into(),
        ));
    }
    let replacement_identity = app
        .authenticate(&req.credential_file, &req.role)
        .await
        .map_err(error)?;
    if replacement_identity.principal != old.principal {
        return Err(error(
            "handoff cannot change authenticated principal".into(),
        ));
    }
    let context_bytes = old
        .pending_context
        .iter()
        .map(|s| s.len() + 2)
        .sum::<usize>()
        + request.context.len();
    if context_bytes > MAX_INPUT_BYTES / 2 {
        return Err(error(
            "pending context plus handoff exceeds context envelope budget".into(),
        ));
    }
    // Reserve the old session before probing/starting the replacement; send cannot race it.
    {
        let mut store = app.store.lock().await;
        let current = store.get(id).map_err(error)?;
        if current.state != old.state
            || current.cleanly_detached != old.cleanly_detached
            || current.last_event_sequence != old.last_event_sequence
            || current.pending_context != old.pending_context
            || current.message_receipts != old.message_receipts
        {
            return Err(error(
                "session changed during handoff; inspect current status and retry".into(),
            ));
        }
        let mut draining = old.clone();
        draining.state = State::Disconnected;
        draining.cleanly_detached = false;
        store.update(draining).map_err(error)?;
    }
    req.primary = false;
    let mut new = match app.start_reserved(req).await {
        Ok(new) => new,
        Err(reason) => {
            let _ = app.store.lock().await.update(old);
            return Err(error(reason));
        }
    };
    {
        let mut store = app.store.lock().await;
        let current = store.get(id).map_err(error)?;
        if current.state != State::Disconnected
            || current.last_event_sequence != old.last_event_sequence
        {
            new.state = State::Failed;
            let _ = store.update(new.clone());
            return Err(error(format!("old session changed during handoff; lease retained. Reconcile and stop unused replacement {}",new.session_id)));
        }
    }
    let mut old_stopped = old.clone();
    old_stopped.primary = false;
    old_stopped.state = State::Stopped;
    new.primary = old.primary;
    new.pending_context = old.pending_context.clone();
    new.pending_context.push(format!(
        "Chorus handoff from session {}. Native history is not transferred.\n{}",
        old.session_id, request.context
    ));
    {
        let mut store = app.store.lock().await;
        store.handoff(old_stopped, new.clone()).map_err(error)?;
        store.append(store::event(&new.session_id,"session.handoff",json!({"from_session_id":id,"open_message_receipts":old.message_receipts,"card":old.card,"history":"fresh_native_conversation"}))).map_err(error)?;
    }
    if let Some(worker) = app.workers.lock().await.remove(id) {
        worker.stop().await;
    }
    Ok(Json(new.public()))
}

#[cfg(test)]
mod observation_tests {
    use super::*;

    #[tokio::test]
    async fn replaced_worker_cannot_apply_buffered_events_to_resumed_session() {
        let dir = tempfile::tempdir().unwrap();
        let config: Config = serde_json::from_value(json!({"version":1,"profiles":{}})).unwrap();
        let app = Supervisor::new(
            config,
            Store::open(dir.path().into()).unwrap(),
            "http://127.0.0.1:1/unused".into(),
        )
        .unwrap();
        let session: Session = serde_json::from_value(json!({
            "version":1,"session_id":"epoch-test","principal":"wren","role":"wren","profile":"fake",
            "runtime":"opencode","runtime_version":"fixture","adapter_version":"fixture","mode":"managed",
            "enforcement":"trusted","capabilities":{},"cwd":"/tmp","primary":true,"state":"idle",
            "created_at":"fixture","heartbeat":"fixture","profile_hash":"fixture"
        })).unwrap();
        app.store.lock().await.insert(session).unwrap();
        let old = app.event_sink("epoch-test".into(), None);
        let store = app.store.lock().await;
        old.send(json!({"type":"turn.started"})).await.unwrap();
        let replacement = app.event_sink("epoch-test".into(), None);
        replacement
            .send(json!({"type":"message.delta","data":{"text":"new worker"}}))
            .await
            .unwrap();
        drop(store);
        tokio::time::timeout(Duration::from_secs(1), async {
            loop {
                let store = app.store.lock().await;
                if !store.events("epoch-test", 0).unwrap().is_empty() {
                    assert_eq!(store.get("epoch-test").unwrap().state, State::Idle);
                    assert_eq!(store.events("epoch-test", 0).unwrap().len(), 1);
                    break;
                }
                drop(store);
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}
