# Agent identity at the MCP boundary

The HTTP and stdio MCP servers use the same request-scoped identity and tool authorization. Agent identity comes from an existing CSS ES256 bearer token, verified by Chorus API using its existing issuer, JWKS, and model-resolved scopes and role. Token role claims, WebID suffixes, process environment, and `X-Chorus-Role` do not establish verified authority.

## Configuration and rollout

| Setting | Meaning |
| --- | --- |
| `CHORUS_MCP_IDENTITY_MODE=strict` | Every HTTP request and stdio tool invocation needs a verified bearer. Use this before enabling arbitrary agents in production. |
| `CHORUS_MCP_IDENTITY_MODE=legacy-claude` | Temporary compatibility with existing tokenless Claude clients; the default while staged migration is incomplete. |
| `CHORUS_API_URL` | Identity/API origin; defaults to `http://localhost:3340`. Identity requests require HTTPS or loopback HTTP and reject redirects. |
| `CHORUS_SESSION_TOKEN_FILE` | Stdio only: path to this session's CSS token file. Read again before every tool call so atomic token replacement takes effect without restarting MCP. |
| `CHORUS_SESSION_ID` | Stdio session binding. Maps to the `X-Chorus-Session-Id` HTTP header. |
| `CHORUS_AGENT_BINDING_PROFILE` | Optional operator-owned profile name for a stdio bridge created before a Chorus session ID exists (OpenCode server MCP config). Resolves the unique live profile/role session over the supervisor UDS. |
| `CHORUS_ROLE` | Stdio consistency assertion and legacy attribution; never overrides a verified role. |

**Legacy mode is not an authenticated security boundary.** A client can omit both bearer and session headers and select the legacy header-based lane. Enrolling a new session does not close that lane globally. Upgrade Claude clients to send their own bearer, verify their governed operations, then explicitly set the shared daemon to `strict` before exposing it to arbitrary clients. Legacy requests no longer receive the daemon's ambient bearer or token-file credential; governed downstream writes may consequently require client migration before succeeding.

Any supplied Authorization header or session ID selects verification even in legacy mode. Invalid/expired tokens, unavailable identity services, role mismatch, and session mismatch fail closed; there is no retry through legacy authentication. Strict mode accepts authenticated role clients without a session ID for transitional clients; a supplied session ID is always verified.

Deployment order:

1. Deploy Chorus API identity verification and agent session facade.
2. Start the local agent supervisor and enroll sessions using operator-managed CSS token files.
3. Configure each client with its own bearer or per-session stdio bridge, session ID, and matching role assertion. Keep token values out of configuration, command arguments, and logs; reference the protected file/environment name.
4. Verify both Claude and new clients, including token rotation, expired-token refusal, and a forged session ID refusal.
5. Set `CHORUS_MCP_IDENTITY_MODE=strict` on the HTTP daemon and enrolled stdio launch profiles. Verify that a header-only call is rejected before operational use.

No deployment or token minting is performed by these code changes.

## HTTP and stdio contracts

HTTP requests use:

```text
Authorization: Bearer <session CSS token>
X-Chorus-Session-Id: <Chorus session ID>
X-Chorus-Role: <matching role, optional assertion>
```

POST, GET, and DELETE `/mcp` all authenticate before the SDK handles the request. The MCP protocol's `Mcp-Session-Id` response is independent transport metadata; it is not a Chorus identity/session binding.

POST `/api/chorus/identity/verify` returns `{ok:true,principal,role,scopes}` after the existing CSS verifier and model role lookup succeed. It returns no bearer. When a Chorus session ID is supplied, MCP also GETs `/api/chorus/agent-sessions/:id` using that same bearer. The returned ID, principal, and role must exactly match and the session state must be `idle`, `running`, or `awaiting_approval`. Missing sessions, stopped/disconnected/failed sessions, mismatches, malformed responses, and service outages refuse the request.

This does not create a circular enrollment dependency: supervisor enrollment calls only the identity endpoint. The MCP binding lookup calls the API facade, which reads supervisor session metadata over the local socket. It does not call MCP again.

The stdio bridge verifies once at startup and again per tool invocation, rereading the token file each time. Expiration/rotation errors refuse operations; an already-running bridge never silently adopts a daemon credential. The file should be owned by the session operator and mode 0600. There is no automatic token renewal in this bridge; the identity/token owner must refresh the file.

For an OpenCode server created before session enrollment, the generated stdio configuration can supply `CHORUS_AGENT_BINDING_PROFILE` and `CHORUS_ROLE` instead of an initial session ID. The bridge POSTs `{profile,role}` to local `/v1/profile-binding`; the supervisor must return exactly one live session, never select newest or primary as a substitute. Startup pins its session ID, principal, and role, then verifies the CSS token against that session through the API. Each tool rechecks the binding and credential. A changed session/principal/role refuses with `profile-binding-changed-restart-bridge`; explicitly stop and restart the bridge for a replacement conversation. The OpenCode endpoint is restricted to one live session so server-scoped MCP config cannot ambiguously act for concurrent conversations.

## Authorization and child execution

AsyncLocalStorage carries identity independently for concurrent requests. Shared HTTP startup removes ambient `CHORUS_IDENTITY_TOKEN`, `CHORUS_SESSION_TOKEN_FILE`, `CHORUS_SESSION_ID`, and `CHORUS_ACTOR_WEBID`. Child environments are rebuilt per request and receive only that verified caller's bearer/principal/session. The daemon does not mint its own service identity on behalf of MCP callers. Explicit builder/worktree role routing is separate from the verified actor; for example, Wren accepting Silas's work retains Wren's accepting identity.

Mutation actor parameters must match the verified role, except authorized human routing and the existing Wren/Jeff acceptance semantics. Read-only role filters can select peer roles. `chorus_card_add_jeff` requires verified Jeff identity. `werk-accept`, `chorus_cards_done`, and the `chorus_werk` GO path require Jeff or Wren; GO's `accepter` must match the caller. Underlying card gates, grants, and verb policy remain in force.

Request bearers are forwarded only to the configured API origin; authenticated fetches reject redirects. Model providers, Loki, and Pulse do not receive CSS bearers. Pulse uses its separate local shared-secret transport.

## Delivery semantics and limitations

`chorus_nudge_message` accepts optional `target_session_id`. Omission selects the recipient role's primary session. The response states that the message was sent to the delivery queue; it is not a context-delivery or reply receipt. See [Pulse agent delivery](../pulse/AGENT-DELIVERY.md).

The authenticated remote session facade filters visibility/control by both principal and role; Jeff may administer sessions. Human-input envelopes require Jeff. Direct sends to native sessions return HTTP 409 `native_requires_pulse_inbox` with `persisted:false`; use the Pulse-backed MCP nudge with an explicit target. Managed sends preserve `transport_accepted` without converting it to delivered. There is no remotely supplied credential-file or profile/executable injection through this facade.

Session binding is checked at request admission. Long-running operations are not continuously reauthorized after admission. Global strict mode protects MCP; it is not a replacement for separate authentication on historical operational HTTP routes or a sandbox against arbitrary local processes owned by the same OS account.

Tests cover actual ES256 verification, concurrent identity isolation, forged role/session IDs, strict transport methods, human-only refusal, daemon credential stripping, and accepting-actor attribution through a real scratch child process.
