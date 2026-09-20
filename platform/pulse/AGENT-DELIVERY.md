# Agent delivery through Pulse

Pulse remains the durable owner of nudge and human-input message bodies in `messages.db`. The local agent supervisor owns enrolled session metadata and delivery receipts. Admission to a transport is not delivery to model context, and delivery to context is not a peer reply.

## Configuration and routing

| Setting | Meaning |
| --- | --- |
| `MESSAGING_PORT` | Pulse listener port, default `3475`. |
| `CHORUS_AGENT_SOCKET` | Local supervisor HTTP-over-UDS socket, default `~/.chorus/run/chorus-agent.sock`. |
| `CHORUS_AGENT_STATE_DIR` | Supervisor state root, default `~/.chorus`; also controls the default socket root. |
| `CHORUS_PULSE_SECRET` | Local caller shared secret, normally resolved from the protected secret file instead. |
| `CHORUS_PULSE_SECRET_FILE` | Shared-secret file, default `~/.chorus/pulse-nudge.secret`. Keep its contents out of logs/config. |
| `CHORUS_PULSE_URL` | MCP's full nudge endpoint, default `http://localhost:3475/api/nudge`. Boundary consumers must use its origin plus the inbox paths, not append to `/api/nudge`. |

The supervisor transport is preferred only for a registered v2 primary or an explicitly addressed session. Role-directed messages select the unique primary, never the newest session. Both `/api/nudge` and `/api/jeff-input` accept optional `target_session_id`; MCP `chorus_nudge_message` exposes the same field. A secondary receives only messages explicitly addressed to it. Recipient role and explicit session must agree.

Outbound nudges also persist nullable `source_session_id`. MCP derives this internal envelope field only from its verified request identity, never tool arguments. Pulse requires the real shared secret even when legacy direct-post compatibility is enabled and validates that the source session is available and belongs to the sender role. Session-scoped respond-first checks can therefore match delivered inbound `delivery_session_id` and outbound `source_session_id` without borrowing a sibling session's reply. Legacy/tokenless attribution remains null; sender session is part of duplicate detection.

No registered primary means the existing Claude delivery transport remains available during migration. An explicit target never falls back. An enrolled primary that is disconnected/stopped/failed or an ambiguous primary does not fall back to a terminal. If the daemon socket is missing, persisted `sessions/v2/*.json` records are read only to block legacy fallback; they cannot prove liveness or delivery. Credential-file references are not exposed by Pulse's session projection. An unreadable/corrupt registry fails closed.

The macOS TCC startup probe remains for installations with legacy roles. When all three AI roles have enrolled primaries, Pulse can start without a legacy terminal-injection probe.

## Durable states and receipts

Pulse sends the stable ID `pulse:<messages.db row id>` to:

```text
POST /v1/sessions/:id/send
{version:1,message_id,input,kind:"peer_message"|"human_input"}
```

| Supervisor result | Pulse state | Next action |
| --- | --- | --- |
| `queued`, native session | `queued`, reason `native-boundary` | Native hook claims at its next safe boundary. No autonomous wake is implied. |
| `queued`, busy managed session | `queued`, reason `agent-busy` | Retry the same bound session when it becomes idle. |
| `transport_accepted` | `queued`, reason `transport-accepted` | Poll receipts; never call this delivered. |
| `uncertain` or interrupted send | `queued`, reason `uncertain` | Poll receipts; never blindly resend. |
| `context_delivered` | `delivered` | Emit the existing `nudge.surfaced` or `jeff.input.surfaced` event. |
| Supervisor unavailable | `queued`, reason `supervisor-unavailable` | Preserve the row and binding until service/session recovery. |
| Explicit missing or ambiguous target | `failed` | Visible refusal; no replacement-session selection. |

Every two seconds Pulse reconciles queued managed receipts through GET `/v1/sessions/:id/receipts`. It never replays accepted/uncertain sends, including duplicate stale worker enqueues. Delivery remains bound to the selected session; a later primary replacement does not silently inherit already-bound rows. Uncertainty and stranded session bindings require operator investigation/reconciliation, not automatic reassignment.

The legacy `/drain` path releases only legacy `target-busy` rows. It cannot release native claims, managed admissions, or uncertain outcomes. Message-state schema changes are additive; claims, target bindings, and the native delivery-event outbox survive Pulse restart.

## Native inbox boundary

Both endpoints below require the actual `X-Chorus-Pulse-Secret`. The new inbox deliberately does not inherit historical nudge fail-open behavior or `PULSE_ALLOW_DIRECT_POST=1`. Missing/unresolvable secret is a refusal. Pulse verifies the current role/session binding against the local supervisor on each claim/ack. Only available native sessions can use this path; secondary sessions remain limited to explicit targets.

```text
POST /api/agent-inbox/claim
{role,session_id,limit?:1..100}

{ok:true,messages:[{
  id:123,message_id:"pulse:123",from:"silas",content:"...",kind:"peer_message"
}]}

POST /api/agent-inbox/ack
{role,session_id,ids:[123]}

{ok:true,acknowledged:1,status:"context_delivered"}
```

Claims atomically bind rows in SQLite. A repeated claim returns the same unacked messages. A claim does not mark anything delivered. The native adapter renders the validated messages as context at a supported safe boundary, preserving peer versus human provenance, and acknowledges only after successfully writing the context response. Acknowledgment is atomic: one message with the wrong claim/role/session rejects the entire batch. Repeating a valid acknowledgment returns `acknowledged:0`.

Successful acknowledgment sets delivery state and persists an outbox record for the canonical surfaced event. If the spine sink is unavailable, periodic reconciliation retries that event without resending the message body. Surfaced-event consumers must tolerate duplicate events after a crash between emission and clearing the outbox.

## Rollout and limits

Deploy the supervisor contract, Pulse schema/routes, and native boundary adapter together before enrolling a role. Verify the generated session ID, native runtime binding, token identity, and primary assignment. Enroll Claude alongside Codex first; keep legacy terminal delivery only for roles not yet enrolled. Switch the shared MCP server to global strict identity before arbitrary production clients are admitted; [MCP identity](../mcp-server/AGENT-IDENTITY.md) describes why per-session enrollment alone does not close the legacy header-only lane.

Remote `/api/chorus/agent-sessions/:id/send` refuses native sessions with `native_requires_pulse_inbox` and `persisted:false`. Use Pulse's canonical MCP nudge path for a durable native queue. Managed API sends preserve their supervisor transport receipt.

This is not exactly-once model execution. A process may crash after context is written but before the ack reaches Pulse, causing the next boundary to repeat a claimed message. Conversely, accepted managed work may have an uncertain execution outcome after a crash. Stable message IDs, idempotent claims/acks/receipts, explicit uncertainty, and no blind replay make these states observable; they do not eliminate the crash windows. A `context_delivered` receipt witnesses adapter handoff and does not prove comprehension, action, or reply.

Tests cover admission without false delivery, duplicate stale enqueue, session-targeted secondary claims, secret and ownership refusals, atomic/idempotent acks, managed receipt reconciliation, uncertain-no-replay, event-sink failure, missing-daemon fallback prevention, and SQLite restart persistence.
