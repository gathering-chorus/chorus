use crate::{contract::*, Result};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    path::Path,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    process::{Child, ChildStdin, Command},
    sync::{mpsc, oneshot, Mutex},
};

pub type EventSink = mpsc::Sender<Value>;
type Pending = Arc<std::sync::Mutex<HashMap<String, oneshot::Sender<Result<Value>>>>>;
struct PendingRequest {
    pending: Pending,
    id: String,
}
impl Drop for PendingRequest {
    fn drop(&mut self) {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.id);
    }
}

pub fn executable(profile: &Profile) -> String {
    profile.executable.clone().unwrap_or_else(|| {
        match profile.runtime {
            Runtime::Claude => "claude",
            Runtime::Codex => "codex",
            Runtime::Opencode => "opencode",
            Runtime::Gemini => "gemini",
            Runtime::External => "",
        }
        .into()
    })
}
pub async fn probe(profile: &Profile) -> Result<(String, Capabilities)> {
    let cwd = std::env::current_dir().map_err(|e| e.to_string())?;
    probe_in(profile, &cwd).await
}
pub async fn probe_in(profile: &Profile, cwd: &Path) -> Result<(String, Capabilities)> {
    if profile.runtime == Runtime::External
        || matches!(profile.runtime, Runtime::Opencode | Runtime::Gemini)
    {
        let worker = Worker::start(profile, None, None).await?;
        let mut settings =
            serde_json::to_value(&profile.adapter_config).map_err(|e| e.to_string())?;
        settings["command"] = json!(executable(profile));
        let response = worker.request("probe", json!({"runtime":profile.runtime,"cwd":cwd,"model":profile.model,"endpoint":profile.endpoint,"config":settings}), 15).await;
        worker.stop().await;
        let response = response?;
        let version = response
            .get("runtime_version")
            .or_else(|| response.get("version"))
            .and_then(Value::as_str)
            .unwrap_or("unknown")
            .to_string();
        let caps = response
            .get("capabilities")
            .cloned()
            .ok_or("worker probe omitted capabilities")?;
        let mut caps: Capabilities = serde_json::from_value(caps)
            .map_err(|e| format!("invalid worker capabilities: {e}"))?;
        if profile.mode == Mode::Native {
            caps.autonomous_wake = false;
        }
        return Ok((version, caps));
    }
    let out = tokio::time::timeout(
        Duration::from_secs(10),
        Command::new(executable(profile))
            .arg("--version")
            .kill_on_drop(true)
            .output(),
    )
    .await
    .map_err(|_| "runtime probe timed out")?
    .map_err(|e| format!("runtime unavailable: {e}"))?;
    if !out.status.success() {
        return Err("runtime version probe failed".into());
    }
    let version = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if version.is_empty() || version.len() > 512 {
        return Err("invalid runtime version response".into());
    }
    let caps = Capabilities {
        resume: true,
        autonomous_wake: profile.mode == Mode::Managed,
        cancellation: profile.mode == Mode::Managed,
        structured_output: true,
        context_boundaries: vec!["session_start".into(), "user_prompt".into(), "tool".into()],
        gaps: vec!["filesystem isolation and hook coverage require deployment conformance".into()],
        ..Default::default()
    };
    Ok((version, caps))
}

pub fn command_args(p: &Profile, session: &Session) -> Result<Vec<String>> {
    let mut args: Vec<String> = match p.runtime {
        Runtime::Codex => {
            let mut a = vec!["exec".into()];
            if let Some(id) = &session.native_session_id {
                a.extend(["resume".into(), id.clone()]);
            } else {
                a.extend(["--sandbox".into(), "workspace-write".into()]);
            }
            a.extend(["--json".into(), "--skip-git-repo-check".into()]);
            if let Some(model) = &session.model {
                a.extend(["--model".into(), model.clone()]);
            }
            a.push("-".into());
            a
        }
        Runtime::Claude => {
            let mut a = vec![
                "-p".into(),
                "--output-format".into(),
                "stream-json".into(),
                "--verbose".into(),
            ];
            if let Some(id) = &session.native_session_id {
                a.extend(["--resume".into(), id.clone()]);
            }
            if let Some(model) = &session.model {
                a.extend(["--model".into(), model.clone()]);
            }
            a
        }
        _ => return Err("this runtime uses a worker adapter".into()),
    };
    // Arguments are separate argv entries, never a shell-generated command.
    args.shrink_to_fit();
    Ok(args)
}

pub fn normalize_native(runtime: &Runtime, v: &Value) -> Vec<Value> {
    let kind = v.get("type").and_then(Value::as_str).unwrap_or("");
    match runtime {
        Runtime::Codex => match kind {
            "thread.started" => {
                vec![json!({"type":"session.started","native_session_id":v["thread_id"]})]
            }
            "turn.started" => vec![json!({"type":"turn.started","data":{}})],
            "turn.completed" => vec![json!({"type":"turn.completed","data":{"usage":v["usage"]}})],
            "turn.failed" | "error" => vec![
                json!({"type":"turn.failed","data":{"error":v.get("error").or_else(||v.get("message"))}}),
            ],
            "item.completed" | "item.started" | "item.updated" => {
                let item = &v["item"];
                if item["type"] == "agent_message" && kind == "item.completed" {
                    vec![
                        json!({"type":"message.completed","data":{"text":item["text"],"item_id":item["id"]}}),
                    ]
                } else if matches!(
                    item["type"].as_str(),
                    Some("command_execution" | "file_change" | "mcp_tool_call" | "web_search")
                ) {
                    vec![
                        json!({"type":if kind == "item.completed" {"tool.completed"} else {"tool.started"},"tool_call_id":item["id"],"data":item}),
                    ]
                } else {
                    vec![]
                }
            }
            _ => vec![],
        },
        Runtime::Claude => match kind {
            "system" if v["subtype"] == "init" => {
                vec![json!({"type":"session.started","native_session_id":v["session_id"]})]
            }
            "assistant" => v["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter_map(|b| {
                    if b["type"] == "text" {
                        Some(json!({"type":"message.completed","data":{"text":b["text"]}}))
                    } else if b["type"] == "tool_use" {
                        Some(json!({"type":"tool.started","tool_call_id":b["id"],"data":b}))
                    } else {
                        None
                    }
                })
                .collect(),
            "user" => v["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|b| b["type"] == "tool_result")
                .map(|b| json!({"type":"tool.completed","tool_call_id":b["tool_use_id"],"data":b}))
                .collect(),
            "result" => vec![
                json!({"type":if v["is_error"] == true {"turn.failed"} else {"turn.completed"},"data":{"text":v["result"],"usage":v["usage"],"subtype":v["subtype"]}}),
            ],
            _ => vec![],
        },
        _ => vec![],
    }
}

fn process_group(cmd: &mut Command) {
    cmd.process_group(0);
    cmd.kill_on_drop(true);
}
pub fn kill_group(pid: u32) {
    // Only process-group IDs captured from children created by this supervisor.
    if pid > 1 {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
}

pub async fn run_cli(
    p: &Profile,
    s: &Session,
    input: &str,
    sink: EventSink,
    mut cancel: oneshot::Receiver<()>,
) -> Result<()> {
    let args = command_args(p, s)?;
    let mut command = Command::new(executable(p));
    command
        .args(args)
        .current_dir(&s.cwd)
        .env("CHORUS_SESSION_ID", &s.session_id)
        .env("CHORUS_ROLE", &s.role)
        .env("DEPLOY_ROLE", &s.role)
        .env(
            "CHORUS_AGENT_RUNTIME",
            serde_json::to_value(&p.runtime).unwrap().as_str().unwrap(),
        )
        .env("CHORUS_MCP_IDENTITY_MODE", "strict")
        .env_remove("CHORUS_IDENTITY_TOKEN")
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(file) = &s.credential_file {
        command.env("CHORUS_SESSION_TOKEN_FILE", file);
    }
    process_group(&mut command);
    let mut child = command
        .spawn()
        .map_err(|e| format!("runtime spawn failed: {e}"))?;
    let pid = child.id().ok_or("runtime has no process id")?;
    let mut stdin = child.stdin.take().ok_or("missing runtime input")?;
    let bytes = input.as_bytes().to_vec();
    let input_task = tokio::spawn(async move {
        stdin.write_all(&bytes).await?;
        stdin.shutdown().await
    });
    let stdout = child.stdout.take().ok_or("missing runtime output")?;
    let stderr = child.stderr.take().ok_or("missing runtime diagnostics")?;
    // Drain stderr without retaining prompts/credentials or allowing a pipe deadlock.
    let diagnostics = tokio::spawn(async move {
        tokio::io::copy(&mut BufReader::new(stderr), &mut tokio::io::sink()).await
    });
    let runtime = p.runtime.clone();
    let read = async {
        let mut reader = BufReader::new(stdout);
        let mut line = Vec::new();
        let mut terminal = false;
        loop {
            line.clear();
            let count = (&mut reader)
                .take((MAX_INPUT_BYTES + 1) as u64)
                .read_until(b'\n', &mut line)
                .await
                .map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            if line.len() > MAX_INPUT_BYTES {
                return Err("runtime event exceeds size limit".into());
            }
            let v: Value =
                serde_json::from_slice(&line).map_err(|_| "runtime emitted invalid JSONL")?;
            for event in normalize_native(&runtime, &v) {
                if event["type"] == "turn.failed" {
                    let _ = sink.send(event).await;
                    return Err("runtime turn failed".into());
                }
                if event["type"] == "turn.completed" {
                    terminal = true;
                }
                sink.send(event)
                    .await
                    .map_err(|_| "event consumer disconnected")?;
            }
        }
        let status = child.wait().await.map_err(|e| e.to_string())?;
        if !status.success() {
            return Err(format!("runtime exited with {status}"));
        }
        if !terminal {
            return Err("runtime exited without a terminal turn event".into());
        }
        Ok(())
    };
    let result = tokio::select! {
        r = tokio::time::timeout(Duration::from_secs(p.timeout_secs), read) => r.unwrap_or_else(|_| Err("runtime turn deadline exceeded".into())),
        _ = &mut cancel => Err("runtime turn cancelled".into()),
    };
    if result.is_err() && matches!(child.try_wait(), Ok(None)) {
        kill_group(pid);
        let _ = child.wait().await;
    }
    input_task.abort();
    diagnostics.abort();
    result
}

pub struct Worker {
    stdin: Mutex<ChildStdin>,
    child: Mutex<Child>,
    pending: Pending,
    disconnected: Arc<AtomicBool>,
    stopping: Arc<AtomicBool>,
    pid: u32,
}
impl Worker {
    pub async fn start(
        profile: &Profile,
        cwd: Option<&Path>,
        sink: Option<EventSink>,
    ) -> Result<Arc<Self>> {
        Self::start_with_env(profile, cwd, sink, &[]).await
    }
    pub async fn start_with_env(
        profile: &Profile,
        cwd: Option<&Path>,
        sink: Option<EventSink>,
        env: &[(&str, &str)],
    ) -> Result<Arc<Self>> {
        let worker = profile
            .worker
            .as_ref()
            .ok_or("profile requires an adapter worker executable")?;
        let mut command = Command::new(worker);
        command.args(&profile.worker_args);
        command
            .env_remove("CHORUS_IDENTITY_TOKEN")
            .env_remove("CHORUS_SESSION_TOKEN_FILE")
            .env_remove("CHORUS_SESSION_ID");
        for (key, value) in env {
            command.env(key, value);
        }
        if env.iter().any(|(key, _)| *key == "CHORUS_SESSION_ID") {
            command.env("CHORUS_MCP_IDENTITY_MODE", "strict");
        }
        if let Some(cwd) = cwd {
            command.current_dir(cwd);
        }
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit());
        process_group(&mut command);
        let mut child = command
            .spawn()
            .map_err(|e| format!("worker unavailable: {e}"))?;
        let pid = child.id().ok_or("worker missing pid")?;
        let stdin = child.stdin.take().ok_or("worker missing input")?;
        let stdout = child.stdout.take().ok_or("worker missing output")?;
        let pending: Pending = Arc::new(std::sync::Mutex::new(HashMap::new()));
        let disconnected = Arc::new(AtomicBool::new(false));
        let disconnected_reader = disconnected.clone();
        let stopping = Arc::new(AtomicBool::new(false));
        let stopping_reader = stopping.clone();
        let pending_reader = pending.clone();
        tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            loop {
                let mut line = Vec::new();
                match (&mut reader)
                    .take((MAX_INPUT_BYTES + 1) as u64)
                    .read_until(b'\n', &mut line)
                    .await
                {
                    Ok(0) | Err(_) => break,
                    Ok(_) if line.len() > MAX_INPUT_BYTES => break,
                    _ => {}
                }
                let value: Value = match serde_json::from_slice(&line) {
                    Ok(v) => v,
                    Err(_) => break,
                };
                if value["version"] != VERSION {
                    break;
                }
                if value["method"] == "event" {
                    if let Some(sink) = &sink {
                        if sink.send(value["params"].clone()).await.is_err() {
                            break;
                        }
                    }
                } else if let Some(id) = value["id"].as_str() {
                    if let Some(sender) = pending_reader
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .remove(id)
                    {
                        let result = if value.get("error").is_some() {
                            Err(format!(
                                "adapter error: {}",
                                value["error"]["code"].as_str().unwrap_or("unknown")
                            ))
                        } else if let Some(result) = value.get("result") {
                            Ok(result.clone())
                        } else {
                            Err("adapter response omitted result".into())
                        };
                        let _ = sender.send(result);
                    }
                }
            }
            disconnected_reader.store(true, Ordering::Release);
            for (_, sender) in pending_reader
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .drain()
            {
                let _ = sender.send(Err("adapter disconnected; outcome uncertain".into()));
            }
            if !stopping_reader.load(Ordering::Acquire) {
                if let Some(sink) = sink {
                    let _ = sink.send(json!({"type":"session.disconnected","data":{"reason":"adapter output closed or violated protocol; reconcile native state"}})).await;
                }
            }
        });
        Ok(Arc::new(Self {
            stdin: Mutex::new(stdin),
            child: Mutex::new(child),
            pending,
            disconnected,
            stopping,
            pid,
        }))
    }
    pub async fn request(&self, method: &str, params: Value, timeout: u64) -> Result<Value> {
        if self.disconnected.load(Ordering::Acquire) {
            return Err("adapter disconnected; outcome uncertain".into());
        }
        let id = crate::id();
        let (tx, rx) = oneshot::channel();
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(id.clone(), tx);
        // Dropping a timed-out/cancelled request removes its receipt as well.
        let _pending = PendingRequest {
            pending: self.pending.clone(),
            id: id.clone(),
        };
        let mut bytes =
            serde_json::to_vec(&json!({"version":VERSION,"id":id,"method":method,"params":params}))
                .map_err(|e| e.to_string())?;
        if bytes.len() > MAX_INPUT_BYTES {
            return Err("worker request exceeds limit".into());
        }
        bytes.push(b'\n');
        let exchange = async {
            if self.disconnected.load(Ordering::Acquire) {
                return Err("adapter disconnected; outcome uncertain".into());
            }
            self.stdin
                .lock()
                .await
                .write_all(&bytes)
                .await
                .map_err(|e| format!("adapter input failed: {e}"))?;
            rx.await.map_err(|_| "adapter disconnected".to_string())?
        };
        tokio::time::timeout(Duration::from_secs(timeout), exchange)
            .await
            .unwrap_or_else(|_| Err("adapter deadline exceeded; outcome uncertain".into()))
    }
    pub async fn stop(&self) {
        self.stopping.store(true, Ordering::Release);
        let mut child = self.child.lock().await;
        // try_wait reaps and remembers exit; never signal a PID already released.
        if matches!(child.try_wait(), Ok(None)) {
            kill_group(self.pid);
            let _ = child.wait().await;
        }
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        self.stopping.store(true, Ordering::Release);
        if matches!(self.child.get_mut().try_wait(), Ok(None)) {
            kill_group(self.pid);
        }
    }
}

/// OS advisory locks bound jobs across both independent CLIs and daemon callers.
/// Keep lock files permanently; unlinking an active lock would create two owners.
struct JobSlot(std::fs::File);
impl JobSlot {
    fn acquire(directory: &Path, limit: usize) -> Result<Option<Self>> {
        use std::os::unix::{
            fs::{DirBuilderExt, OpenOptionsExt},
            io::AsRawFd,
        };
        if !(1..=32).contains(&limit) {
            return Err("job concurrency limit must be 1..32".into());
        }
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(directory)
            .map_err(|e| format!("create job lock directory: {e}"))?;
        for slot in 0..limit {
            let file = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .mode(0o600)
                .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC)
                .open(directory.join(format!("slot-{slot}")))
                .map_err(|e| format!("open job lock: {e}"))?;
            if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } == 0 {
                return Ok(Some(Self(file)));
            }
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EWOULDBLOCK) {
                return Err(format!("acquire job lock: {error}"));
            }
        }
        Ok(None)
    }
    async fn wait(directory: &Path, limit: usize, deadline: tokio::time::Instant) -> Result<Self> {
        loop {
            if let Some(slot) = Self::acquire(directory, limit)? {
                return Ok(slot);
            }
            if tokio::time::Instant::now() >= deadline {
                return Err("job deadline exceeded waiting for concurrency slot".into());
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    }
}
impl Drop for JobSlot {
    fn drop(&mut self) {
        use std::os::unix::io::AsRawFd;
        unsafe {
            libc::flock(self.0.as_raw_fd(), libc::LOCK_UN);
        }
    }
}

pub async fn run_job(profile: &Profile, req: JobRequest) -> Result<JobResult> {
    let root = crate::config::root();
    let job_id = crate::id();
    let started = crate::now();
    let result = async {
        let config = crate::config::read(&crate::config::config_path())?;
        run_job_with_slots(
            profile,
            req.clone(),
            &root.join("run/job-slots"),
            config.max_concurrent_jobs,
        )
        .await
    }
    .await;
    let result = result.map(|mut value| {
        value.job_id = job_id.clone();
        value
    });
    record_job_metadata(&root, &job_id, profile, &req, &started, &result)?;
    result
}

fn job_error_class(error: &str) -> &'static str {
    if error.contains("deadline") || error.contains("timed out") {
        "timeout"
    } else if error.contains("schema") || error.contains("not JSON") {
        "schema"
    } else if error.contains("unrefused") {
        "incomplete_or_refused"
    } else if error.contains("disable tools") {
        "policy"
    } else if error.contains("unavailable") || error.contains("spawn") {
        "unavailable"
    } else {
        "failed"
    }
}
fn record_job_metadata(
    root: &Path,
    job_id: &str,
    profile: &Profile,
    req: &JobRequest,
    started: &str,
    result: &Result<JobResult>,
) -> Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    let provider = profile.provider.as_ref().map(|provider| {
        // Validated configurations have neither URL credentials nor query strings;
        // sanitize again because failed validation must also be auditable safely.
        let endpoint = provider
            .base_url
            .as_ref()
            .and_then(|value| reqwest::Url::parse(value).ok())
            .map(|mut url| {
                let _ = url.set_username("");
                let _ = url.set_password(None);
                url.set_query(None);
                url.set_fragment(None);
                url.to_string()
            });
        let reference = provider.api_key_env.as_ref().filter(|name| {
            !name.is_empty() && name.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_')
        });
        let protocol = if ["anthropic", "openai-chat", "openai-responses"]
            .contains(&provider.protocol.as_str())
        {
            provider.protocol.as_str()
        } else {
            "unsupported"
        };
        json!({"protocol":protocol,"endpoint":endpoint,"api_key_env":reference})
    });
    let usage=result.as_ref().ok().map(|value|json!({"input_tokens":value.usage.input_tokens,"output_tokens":value.usage.output_tokens})).unwrap_or_else(||json!({"input_tokens":null,"output_tokens":null}));
    let model = req.model.as_ref().or(profile.model.as_ref());
    let status = if result.is_ok() {
        "completed"
    } else {
        "failed"
    };
    let schema_hash = req
        .output_schema
        .as_ref()
        .map(|schema| crate::digest(&serde_json::to_vec(schema).unwrap()));
    let metadata = json!({"version":VERSION,"job_id":job_id,"status":status,"profile":req.profile,"profile_hash":crate::config::hash(profile),"runtime":profile.runtime,
        "adapter_version":profile.conformance.as_ref().map(|value|value.adapter_version.as_str()),"worker_protocol_version":VERSION,"supervisor_version":env!("CARGO_PKG_VERSION"),"model":model,"provider":provider,"started_at":started,"finished_at":crate::now(),
        "input_hash":crate::digest(req.input.as_bytes()),"instruction_hash":crate::digest(req.instructions.as_bytes()),"schema_hash":schema_hash,
        "input_revision":req.input_revision,"trace_id":req.trace_id,"usage":usage,"usage_status":if usage["input_tokens"].is_null() || usage["output_tokens"].is_null(){"unknown_or_partial"}else{"reported"},
        "error_class":result.as_ref().err().map(|error|job_error_class(error))});
    let directory = root.join("agent-jobs");
    crate::store::private_dir(&directory)?;
    crate::store::atomic_json(&directory.join(format!("{job_id}.json")), &metadata)
        .map_err(|_| "job provenance could not be recorded".to_string())?;
    let spine = json!({"ts":crate::now(),"event":format!("job.{status}"),"job_id":job_id,"profile":req.profile,"runtime":profile.runtime,"trace_id":req.trace_id,"input_revision":req.input_revision});
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .mode(0o600)
        .open(root.join("chorus.log"))
        .map_err(|_| "job spine event could not be recorded")?;
    writeln!(file, "{spine}")
        .and_then(|_| file.sync_data())
        .map_err(|_| "job spine event could not be recorded".into())
}

async fn run_job_with_slots(
    profile: &Profile,
    req: JobRequest,
    slots: &Path,
    limit: usize,
) -> Result<JobResult> {
    crate::config::validate_profile(profile)?;
    if req.version != VERSION || !profile.no_tools {
        return Err("job profile must explicitly disable tools".into());
    }
    if req.input.len() + req.instructions.len() > MAX_INPUT_BYTES {
        return Err("job input exceeds limit".into());
    }
    if let Some(schema) = &req.output_schema {
        jsonschema::validator_for(schema).map_err(|e| format!("invalid output schema: {e}"))?;
    }
    let timeout = req
        .timeout_secs
        .unwrap_or(profile.timeout_secs)
        .min(profile.timeout_secs);
    if timeout == 0 {
        return Err("job timeout must be positive".into());
    }
    if profile.provider.is_none() {
        return Err("text jobs require an explicit provider configuration".into());
    }
    let model = req.model.clone().or_else(|| profile.model.clone());
    if model.as_deref().is_none_or(|value| value.trim().is_empty()) {
        return Err("text jobs require a model".into());
    }
    let deadline = tokio::time::Instant::now() + Duration::from_secs(timeout);
    let _slot = JobSlot::wait(slots, limit, deadline).await?;
    let worker = Worker::start(profile, None, None).await?;
    let model = req.model.clone().or_else(|| profile.model.clone());
    let remaining = deadline
        .saturating_duration_since(tokio::time::Instant::now())
        .as_millis()
        .min(u64::MAX as u128) as u64;
    let result = tokio::time::timeout_at(deadline, worker.request("run", json!({"input":req.input,"instructions":req.instructions,"model":model,"provider":profile.provider,"output_schema":req.output_schema,"timeout_ms":remaining.max(1),"no_tools":true,"max_output_tokens":profile.max_output_tokens}), timeout)).await
        .unwrap_or_else(|_| Err("job deadline exceeded; outcome uncertain".into()));
    worker.stop().await;
    let result = result?;
    let finish = result["finish_reason"].as_str().unwrap_or("");
    if !["stop", "end_turn", "completed"].contains(&finish) {
        return Err("model did not produce a complete, unrefused response".into());
    }
    let text = result["text"]
        .as_str()
        .ok_or("job omitted text")?
        .to_string();
    let output = if let Some(schema) = &req.output_schema {
        let value: Value = serde_json::from_str(&text).map_err(|_| "job output is not JSON")?;
        jsonschema::validate(schema, &value)
            .map_err(|e| format!("job output schema mismatch: {e}"))?;
        Some(value)
    } else {
        None
    };
    Ok(JobResult {
        version: VERSION,
        job_id: crate::id(),
        text,
        output,
        usage: Usage {
            input_tokens: result["usage"]["input_tokens"].as_u64(),
            output_tokens: result["usage"]["output_tokens"].as_u64(),
        },
        runtime: profile.runtime.clone(),
        model,
        profile_hash: crate::config::hash(profile),
        input_hash: crate::digest(format!("{}\0{}", req.instructions, req.input).as_bytes()),
        trace_id: req.trace_id,
        input_revision: req.input_revision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str = r#"
import json,sys,time
for line in sys.stdin:
 r=json.loads(line); p=r.get('params',{}); mode=p.get('input','good')
 if mode=='disconnect': sys.exit(0)
 if mode=='timeout': time.sleep(30); continue
 if r['method']=='run':
  if not (p['no_tools'] is True and p['provider']['protocol']=='openai-chat' and p['model']=='fixture-model' and p['max_output_tokens']==32):
   print(json.dumps({'version':1,'id':r['id'],'error':{'code':'bad_contract'}}),flush=True); continue
  text='not-json' if mode=='malformed' else json.dumps({'result':'maybe' if mode=='schema-mismatch' else 'pass'})
  result={'text':text,'finish_reason':'content_filter' if mode=='refusal' else 'stop','usage':{'input_tokens':3,'output_tokens':None}}
 else: result={}
 print(json.dumps({'version':1,'id':r['id'],'result':result}),flush=True)
"#;
    fn profile(script: &str) -> Profile {
        serde_json::from_value(json!({"runtime":"external","mode":"managed","enforcement":"trusted","worker":"/usr/bin/python3","worker_args":["-u","-c",script],"model":"fixture-model","provider":{"protocol":"openai-chat","base_url":"http://127.0.0.1:1234/v1"},"no_tools":true,"max_output_tokens":32,"timeout_secs":3})).unwrap()
    }
    fn request(input: &str) -> JobRequest {
        serde_json::from_value(json!({"version":1,"profile":"fixture","input":input,"instructions":"check","output_schema":{"type":"object","required":["result"],"properties":{"result":{"enum":["pass","fail"]}},"additionalProperties":false},"trace_id":"trace-fixture","input_revision":"abc123"})).unwrap()
    }
    #[tokio::test]
    async fn job_contract_preserves_arguments_unknown_usage_and_schema() {
        let slots = tempfile::tempdir().unwrap();
        let result = run_job_with_slots(&profile(FIXTURE), request("good"), slots.path(), 1)
            .await
            .unwrap();
        assert_eq!(result.output.unwrap()["result"], "pass");
        assert_eq!(result.usage.input_tokens, Some(3));
        assert_eq!(result.usage.output_tokens, None);
        assert_eq!(result.trace_id.as_deref(), Some("trace-fixture"));
        assert_eq!(result.input_revision.as_deref(), Some("abc123"));
    }
    #[tokio::test]
    async fn jobs_reject_refusal_malformed_output_and_schema_mismatch() {
        let slots = tempfile::tempdir().unwrap();
        let p = profile(FIXTURE);
        for (input, expected) in [
            ("refusal", "unrefused"),
            ("malformed", "not JSON"),
            ("schema-mismatch", "schema mismatch"),
        ] {
            assert!(run_job_with_slots(&p, request(input), slots.path(), 1)
                .await
                .unwrap_err()
                .contains(expected));
        }
    }
    #[tokio::test]
    async fn jobs_validate_no_tools_and_schema_before_spawning() {
        let slots = tempfile::tempdir().unwrap();
        let mut p = profile(FIXTURE);
        p.worker = Some("/does/not/exist".into());
        p.no_tools = false;
        assert!(run_job_with_slots(&p, request("good"), slots.path(), 1)
            .await
            .unwrap_err()
            .contains("disable tools"));
        p.no_tools = true;
        let mut req = request("good");
        req.output_schema = Some(json!({"type":"invented"}));
        assert!(run_job_with_slots(&p, req, slots.path(), 1)
            .await
            .unwrap_err()
            .contains("invalid output schema"));
    }
    #[tokio::test]
    async fn job_timeout_releases_slot_and_kills_worker() {
        let slots = tempfile::tempdir().unwrap();
        let mut p = profile(FIXTURE);
        p.timeout_secs = 1;
        let started = std::time::Instant::now();
        assert!(run_job_with_slots(&p, request("timeout"), slots.path(), 1)
            .await
            .unwrap_err()
            .contains("deadline"));
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(JobSlot::acquire(slots.path(), 1).unwrap().is_some());
    }
    #[tokio::test]
    async fn worker_eof_reports_disconnection_and_rejects_future_requests() {
        let (tx, mut rx) = mpsc::channel(4);
        let worker = Worker::start(&profile(FIXTURE), None, Some(tx))
            .await
            .unwrap();
        assert!(worker
            .request("send", json!({"input":"disconnect"}), 2)
            .await
            .unwrap_err()
            .contains("disconnected"));
        let event = tokio::time::timeout(Duration::from_secs(1), rx.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(event["type"], "session.disconnected");
        assert!(worker
            .request("status", json!({}), 2)
            .await
            .unwrap_err()
            .contains("disconnected"));
        worker.stop().await;
        worker.stop().await;
        assert!(worker.child.lock().await.try_wait().unwrap().is_some());
    }
    #[tokio::test]
    async fn worker_deadline_includes_blocked_stdin_and_cleans_pending_receipt() {
        let worker = Worker::start(&profile("import time; time.sleep(30)"), None, None)
            .await
            .unwrap();
        let started = std::time::Instant::now();
        assert!(worker
            .request("send", json!({"input":"x".repeat(512*1024)}), 1)
            .await
            .unwrap_err()
            .contains("deadline"));
        assert!(started.elapsed() < Duration::from_secs(3));
        assert!(worker.pending.lock().unwrap().is_empty());
        worker.stop().await;
    }
    #[test]
    fn concurrency_slots_are_shared_with_other_processes_and_released() {
        let slots = tempfile::tempdir().unwrap();
        let held = JobSlot::acquire(slots.path(), 1).unwrap().unwrap();
        assert!(JobSlot::acquire(slots.path(), 1).unwrap().is_none());
        let probe = "import fcntl,sys; f=open(sys.argv[1],'r+');\ntry: fcntl.flock(f,fcntl.LOCK_EX|fcntl.LOCK_NB); sys.exit(5)\nexcept BlockingIOError: sys.exit(0)";
        let out = std::process::Command::new("/usr/bin/python3")
            .args(["-c", probe])
            .arg(slots.path().join("slot-0"))
            .output()
            .unwrap();
        assert!(out.status.success());
        drop(held);
        assert!(JobSlot::acquire(slots.path(), 1).unwrap().is_some());
    }
    #[tokio::test]
    async fn waiting_for_slot_is_bounded() {
        let slots = tempfile::tempdir().unwrap();
        let _held = JobSlot::acquire(slots.path(), 1).unwrap().unwrap();
        let result = JobSlot::wait(
            slots.path(),
            1,
            tokio::time::Instant::now() + Duration::from_millis(50),
        )
        .await;
        assert!(matches!(result,Err(e) if e.contains("concurrency slot")));
    }
    #[test]
    fn durable_job_metadata_retains_provenance_without_content_or_raw_errors() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let p = profile(FIXTURE);
        let mut req = request("PRIVATE_INPUT");
        req.instructions = "PRIVATE_INSTRUCTIONS".into();
        let failure: Result<JobResult> = Err("provider failed: PRIVATE_RAW_ERROR".into());
        record_job_metadata(directory.path(), "job-fixture", &p, &req, "start", &failure).unwrap();
        let path = directory.path().join("agent-jobs/job-fixture.json");
        let text = std::fs::read_to_string(&path).unwrap();
        let metadata: Value = serde_json::from_str(&text).unwrap();
        assert!(!text.contains("PRIVATE_"));
        assert_eq!(metadata["status"], "failed");
        assert_eq!(metadata["provider"]["protocol"], "openai-chat");
        assert!(metadata["usage"]["input_tokens"].is_null());
        assert_eq!(metadata["trace_id"], "trace-fixture");
        assert!(metadata["schema_hash"].as_str().unwrap().len() == 64);
        assert_eq!(
            std::fs::metadata(path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(std::fs::read_to_string(directory.path().join("chorus.log"))
            .unwrap()
            .contains("job.failed"));
    }
}
