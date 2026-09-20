use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const VERSION: u32 = 1;
pub const MAX_INPUT_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum Runtime {
    Claude,
    Codex,
    Opencode,
    Gemini,
    External,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Native,
    Managed,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Enforcement {
    Verified,
    Trusted,
}
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum State {
    Idle,
    Running,
    AwaitingApproval,
    Disconnected,
    Failed,
    Stopped,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Capabilities {
    pub resume: bool,
    pub autonomous_wake: bool,
    pub mid_turn_steering: bool,
    pub cancellation: bool,
    pub structured_output: bool,
    pub before_tool: bool,
    pub history_recovery: bool,
    pub no_tools: bool,
    #[serde(default)]
    pub context_boundaries: Vec<String>,
    #[serde(default)]
    pub gaps: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provider {
    pub protocol: String,
    pub base_url: Option<String>,
    pub api_key_env: Option<String>,
    pub model_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    pub runtime: Runtime,
    pub mode: Mode,
    pub enforcement: Enforcement,
    #[serde(default)]
    pub approved_gaps: Vec<String>,
    pub model: Option<String>,
    pub provider: Option<Provider>,
    pub executable: Option<String>,
    #[serde(default)]
    pub worker_args: Vec<String>,
    pub worker: Option<String>,
    pub endpoint: Option<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default)]
    pub no_tools: bool,
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    /// Operator-owned capability attestation. Enrollment never accepts this from a model.
    pub conformance: Option<Conformance>,
    /// Operator-controlled adapter settings; never accepted on enrollment requests.
    #[serde(default)]
    pub adapter_config: BTreeMap<String, Value>,
}
pub fn default_timeout() -> u64 {
    180
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Conformance {
    pub runtime_version: String,
    pub adapter_version: String,
    pub report_sha256: String,
    pub report_path: String,
    pub capabilities: Capabilities,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Identity {
    pub principal: String,
    pub role: String,
    #[serde(default)]
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub version: u32,
    pub session_id: String,
    pub principal: String,
    pub role: String,
    pub parent_session_id: Option<String>,
    pub profile: String,
    pub runtime: Runtime,
    pub runtime_version: String,
    pub adapter_version: String,
    pub mode: Mode,
    pub enforcement: Enforcement,
    pub capabilities: Capabilities,
    pub native_session_id: Option<String>,
    pub model: Option<String>,
    pub provider: Option<Provider>,
    pub cwd: String,
    pub card: Option<u64>,
    pub primary: bool,
    pub state: State,
    /// True only after an intentional idle detach, never inferred from a crash.
    #[serde(default)]
    pub cleanly_detached: bool,
    pub created_at: String,
    pub heartbeat: String,
    pub profile_hash: String,
    /// File reference only. Never persisted in events or returned by the API.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credential_file: Option<String>,
    #[serde(default)]
    pub message_receipts: BTreeMap<String, String>,
    #[serde(default)]
    pub message_hashes: BTreeMap<String, String>,
    #[serde(default)]
    pub pending_context: Vec<String>,
    #[serde(default)]
    pub pending_approvals: BTreeMap<String, Value>,
    #[serde(default)]
    pub last_event_sequence: u64,
}

impl Session {
    pub fn public(&self) -> Value {
        let mut value = serde_json::to_value(self).expect("serializable session");
        value.as_object_mut().unwrap().remove("credential_file");
        value.as_object_mut().unwrap().remove("message_receipts");
        value.as_object_mut().unwrap().remove("message_hashes");
        value.as_object_mut().unwrap().remove("pending_context");
        value
    }
    /// A handoff cannot imply that an uncertain business operation succeeded or failed.
    pub fn switch_blockers(&self) -> Vec<String> {
        let mut blockers = Vec::new();
        if self.state != State::Idle
            && !(self.state == State::Disconnected && self.cleanly_detached)
        {
            blockers.push(format!(
                "session is {}; finish or cancel work and reconcile before switching",
                serde_json::to_value(&self.state).unwrap().as_str().unwrap()
            ));
        }
        if !self.pending_approvals.is_empty() {
            blockers.push("session has unresolved approvals".into());
        }
        if self
            .message_receipts
            .values()
            .any(|r| matches!(r.as_str(), "transport_accepted" | "uncertain"))
        {
            blockers.push(
                "session has pending or uncertain delivery; reconcile before switching".into(),
            );
        }
        blockers
    }
    // Failed/disconnected conversations retain their lease until explicit release.
    pub fn live(&self) -> bool {
        self.state != State::Stopped
    }
    pub fn apply_event(&mut self, event: &Event) -> crate::Result<()> {
        if event.sequence <= self.last_event_sequence {
            return Ok(());
        }
        if let Some(native) = &event.native_session_id {
            if self
                .native_session_id
                .as_ref()
                .is_some_and(|old| old != native)
            {
                return Err("native session identity changed in journal".into());
            }
            self.native_session_id = Some(native.clone());
        }
        self.heartbeat = event.timestamp.clone();
        self.cleanly_detached = false;
        match event.event_type.as_str() {
            _ if self.state == State::Stopped => {}
            "session.detached" => {
                self.state = State::Disconnected;
                self.cleanly_detached = event.data["clean"] == true;
                for receipt in self.message_receipts.values_mut() {
                    if receipt == "transport_accepted" {
                        *receipt = "uncertain".into();
                    }
                }
            }
            "context.enqueued" => {
                if let Some(text) = event.data["text"].as_str() {
                    self.pending_context.push(text.into());
                }
            }
            "turn.started" => self.state = State::Running,
            "approval.required" => {
                self.state = State::AwaitingApproval;
                if let Some(id) = event.data["request_id"].as_str() {
                    self.pending_approvals.insert(id.into(), event.data.clone());
                }
            }
            "approval.resolved" => {
                if let Some(id) = event.data["request_id"].as_str() {
                    self.pending_approvals.remove(id);
                }
                if self.pending_approvals.is_empty() {
                    self.state = State::Running;
                }
            }
            "turn.completed" => self.state = State::Idle,
            "turn.failed" => self.state = State::Failed,
            "session.stopped" => self.state = State::Stopped,
            "session.disconnected" | "session.ended" => self.state = State::Disconnected,
            _ => {}
        }
        if matches!(event.event_type.as_str(), "turn.completed" | "turn.failed") {
            self.pending_approvals.clear();
            for receipt in self.message_receipts.values_mut() {
                if receipt == "transport_accepted" {
                    *receipt = if event.event_type == "turn.completed" {
                        "context_delivered"
                    } else {
                        "uncertain"
                    }
                    .into();
                }
            }
            if event.event_type == "turn.completed" && self.mode == Mode::Managed {
                self.pending_context.clear();
            }
        }
        self.last_event_sequence = event.sequence;
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HandoffRequest {
    pub replacement: StartRequest,
    pub context: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SwitchRequest {
    pub profile: String,
    pub context: String,
    pub credential_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApprovalRequest {
    pub credential_file: String,
    pub request_id: String,
    pub decision: Option<String>,
    pub option_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StartRequest {
    pub version: u32,
    pub profile: String,
    pub role: String,
    pub cwd: String,
    pub credential_file: String,
    #[serde(default)]
    pub primary: bool,
    pub card: Option<u64>,
    pub parent_session_id: Option<String>,
    pub native_session_id: Option<String>,
    pub model: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SendRequest {
    pub version: u32,
    pub message_id: String,
    pub input: String,
    #[serde(default = "peer_kind")]
    pub kind: String,
}
fn peer_kind() -> String {
    "peer_message".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub version: u32,
    pub session_id: String,
    pub event_id: String,
    #[serde(default)]
    pub sequence: u64,
    #[serde(default)]
    pub timestamp: String,
    #[serde(rename = "type")]
    pub event_type: String,
    pub native_session_id: Option<String>,
    pub turn_id: Option<String>,
    pub tool_call_id: Option<String>,
    pub message_id: Option<String>,
    pub trace_id: Option<String>,
    #[serde(default)]
    pub data: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JobRequest {
    pub version: u32,
    pub profile: String,
    pub input: String,
    #[serde(default)]
    pub instructions: String,
    pub output_schema: Option<Value>,
    pub model: Option<String>,
    pub timeout_secs: Option<u64>,
    pub trace_id: Option<String>,
    pub input_revision: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Usage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobResult {
    pub version: u32,
    pub job_id: String,
    pub text: String,
    pub output: Option<Value>,
    pub usage: Usage,
    pub runtime: Runtime,
    pub model: Option<String>,
    pub profile_hash: String,
    pub input_hash: String,
    pub trace_id: Option<String>,
    pub input_revision: Option<String>,
}
