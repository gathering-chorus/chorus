# Chorus agent supervisor

The Rust `chorus-agentd` service owns enrolled sessions and leases. `chorus-agent`
is its local operator CLI and a standalone bounded text-job runner. The rollout
is opt-in: neither installing this crate nor generating configuration changes a
role's runtime. See [the migration runbook](../../../docs/agent-runtime-migration.md)
for installation, validation, and rollback.

## Configuration and identity

`CHORUS_AGENT_CONFIG` defaults to `~/.chorus/agent-profiles.json`.
`CHORUS_AGENT_STATE_DIR` defaults to `~/.chorus` and `CHORUS_AGENT_SOCKET` to
`<state>/run/chorus-agent.sock`. The socket is owner-only, with an exclusive daemon
lock. It is an OS-user trust boundary, not isolation between programs running as
the same user. Remote users enter through authenticated Chorus API routes.

`CHORUS_API_URL` selects existing CSS identity verification (default
`http://127.0.0.1:3340`). Enrollment requires an owner-only regular token file;
symlinks are refused. The daemon rereads and verifies it on session commands.
Rotate the file atomically using the existing credential issuer. The supervisor
does not mint identities, change acting roles, or store bearer tokens in records.

Profile selection is explicit `--profile`, `CHORUS_AGENT_PROFILE`, then
`roles[role]`. `chorus-awake` delegates only when one of these selects a profile;
otherwise its existing Claude behavior remains. `role_workspaces[role]` supplies
the session anchor. Card enrollment requires a matching `worktree_base/<role>-<card>`
directory, supplied explicitly. No command chooses the newest worktree or history.

The [example](../../config/agent-profiles.example.json) has placeholder paths/model
IDs and an empty `roles` map. Replace placeholders and remove unused profiles.
Examples deliberately declare trusted capability gaps. They are not production
certifications. A verified profile requires a hash-matched, operator-installed
JSON conformance report with `passed:true`, matching runtime/adapter versions and
capabilities, including pre-tool interception and history recovery (and managed
cancellation). Empty gap lists alone do not certify an adapter.

## CLI

All structured requests use JSON stdin; stdout is a JSON response except the
explicit `run --text` path and an attached native client. Error status is nonzero.

| Command | Behavior |
| --- | --- |
| `doctor [profile]` | Version/capability probes; no model task. This is not a coding certification or proof of provider authentication. |
| `launch role [--profile name] [--cwd path]` | Enroll one primary; attach a native CLI or leave a managed session ready for input. Requires `CHORUS_SESSION_TOKEN_FILE`. |
| `start` / `register` | Enroll a JSON `StartRequest`; managed Claude/Codex start their process on first send. |
| `status [id]` / `events id` | Inspect public state, switch readiness/blockers, or a cursor page of normalized events. |
| `profiles` | Inspect loaded profile metadata and role defaults without exposing adapter configuration or credential paths. |
| `reload` | Validate and load the configured profile file without restarting sessions. Refuses changes to live profiles, role/card workspace bindings, or the concurrency limit. |
| `send id` | Submit typed input with a stable message ID. Native/busy `queued,persisted:false` is a refusal of immediate delivery: use Pulse for durable queuing. |
| `resume id` | Resume this exact native conversation. Managed adapters resume their handles; native Claude/Codex attach their CLI. Other native clients reconnect in their own UI. |
| `cancel id` | Cancel an owned managed turn. Native client cancellation must use client controls. |
| `stop id` | Release the registration/lease and stop owned workers. It does not kill an independently owned native app. |
| `disconnect id` | Stop the owned transport, retaining the role lease, native history, and receipts. An intentional idle detach can switch directly; interrupted delivery remains uncertain. |
| `context id` | JSON `{text}`; durably queue initial handoff context while idle, without starting a model turn. |
| `handoff id` | JSON `{replacement: StartRequest, context: string}`; idle or cleanly detached only, same role/principal, fresh native conversation. Context must describe task state, evidence and open obligations. |
| `switch id` | JSON `{profile,context,credential_file?}`; preserves role, card, worktree, parent and credential reference; transfers the primary lease and any undelivered handoff context. Busy or uncertain sessions are refused. |
| `approve id` | JSON `{credential_file,request_id,decision?,option_id?}`; requires verified human identity and a pending managed permission request. |
| `run [--text]` | Run a JSON no-tools job, schema-validate output, record provenance. Shared file locks bound concurrent CLI jobs. |

The operator setup frontend uses `switch` and `reload`; callers do not need to
reconstruct session records. Add a new profile before switching and retain the old
profile until its session stops. Editing a profile in place while it is live is
refused, including disconnected and failed sessions that still hold a lease.
Changing a default affects future admissions only. Reload does not cancel work,
release leases, or resume conversations. OpenCode replacements require a distinct
server endpoint while the previous session is live. Native clients remain owned
by their client process: close the old client and attach the replacement through
its supported boundary; switching registration does not remotely control a UI.

Example start request (paths are explicit; there is no shell expansion in JSON):

```json
{"version":1,"profile":"codex-managed","role":"wren","cwd":"/absolute/chorus/roles/wren","credential_file":"/absolute/private/wren.token","primary":true}
```

Example input:

```json
{"version":1,"message_id":"operator:review-42","kind":"peer_message","input":"Review card 42. Preserve the current worktree binding."}
```

Example bounded job:

```json
{"version":1,"profile":"gate-chat","input":"Evidence to review","instructions":"Return a verdict supported by the evidence.","output_schema":{"type":"object","required":["verdict"],"properties":{"verdict":{"enum":["pass","fail"]}},"additionalProperties":false},"trace_id":"review-42","input_revision":"COMMIT_SHA"}
```

## Storage and recovery

- `sessions/v2/<id>.json`: private snapshots, credential **references**, explicit
  primary lease, pending context/approvals, stable input hashes and receipts.
- `agent-events/<id>.jsonl`: authoritative ordered event journal. Event IDs dedupe
  replay; conflicting IDs and torn/out-of-order records are refused. Snapshots
  carry a replay cursor. Interrupted admissions become uncertain after restart.
- `sessions/v2/handoff.pending`: recoverable two-snapshot handoff transaction.
  Startup replays it before making leases available.
- `agent-jobs/`: private, sanitized job provenance; prompts/outputs are not copied
  into the audit record. Detailed data stays with the workload owner.
- `chorus.log`: best-effort operational projections of session events and bounded
  job outcomes. The detailed session journal is authoritative.

An intentional idle `disconnect` records `cleanly_detached:true`, so closing the
operator terminal does not force a throwaway resume before switching profiles.
Only this explicit checkpoint enables detached handoff; crashes and interrupted
turns do not qualify. Resume preserves the exact native ID and clears the flag.
Uncertain receipts prevent new work until the operator reconciles the prior run.

Daemon restart marks live sessions disconnected and retains their primary lease.
Explicit resume reconciles them; it never replays an uncertain mutating input.
Journal damage fails the affected session and remains visible. Back up the
journal, determine the valid complete prefix, reconcile external effects, and
restore the journal under operator control before resuming. Do not truncate a
journal or clear receipts as a substitute for reconciliation.

## Adapter protocol

Managed vendor workers use bounded newline-framed JSON on stdio, major version 1.
Each request has a unique `id`, `method`, and `params`; stdout is protocol-only.
The Rust reader limits frames to 4 MiB, bounds queues, correlates responses,
times out input writes as well as reads, and reports worker EOF as disconnection.
Only child process groups started by the supervisor are terminated. An event
notification is not a business-operation retry signal.

See [worker contract and upstream sources](../../agent-adapters/README.md) and
[`contracts/protocol-v1.schema.json`](contracts/protocol-v1.schema.json).

The local UDS API is versioned under `/v1`: sessions, explicit resume/send/stop,
native boundary/ack, native/profile binding, event ingestion, approval, jobs and
profile probes. The authenticated HTTP front door exposes session listing,
status, events, send and cancellation/stop; process enrollment and human approval
remain local operator operations. Unsupported native wake or approval paths
return an error or queued state, never a successful receipt.
