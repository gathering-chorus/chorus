# Text generation providers

Clearing's chat roles use a text-generation transport, separate from operational agent runtimes. With no configuration changes the provider remains Anthropic and the model remains `claude-haiku-4-5-20251001`.

Environment configuration:

| Variable | Meaning |
| --- | --- |
| `CLEARING_PROVIDER` | `anthropic` (default), `openai-compatible`, or `openai-responses` |
| `CLEARING_MODEL` | Explicit model identifier for the chosen provider |
| `CLEARING_BASE_URL` | Optional API root, e.g. `https://example.com/v1`; no endpoint suffix |
| `CLEARING_API_KEY_ENV` | Name of the environment variable containing the credential; never the credential value |
| `CLEARING_TIMEOUT_MS` | Whole-request deadline, default 60000 |
| `CLEARING_MAX_TOKENS` | Output budget; existing default 300 |

Default credential names are `ANTHROPIC_API_KEY` and `OPENAI_API_KEY`. An explicitly configured missing credential reference fails before the request. Custom unauthenticated local endpoints may omit credentials. Chat Completions uses `/chat/completions`; Responses uses `/responses`; they are separate protocols.

The client sends no tool definitions, handles nonstreaming and incremental SSE text, propagates cancellation, and sanitizes provider error bodies. Broken/truncated streams fail rather than being treated as successful replies. Usage remains null when unreported; transcript totals then remain unknown. Models without an explicit price in the existing table return a null estimate rather than inheriting a Claude price.

The worker `run` path reuses this module after `npm ci && npm run build`. JSON schema validation belongs to the calling supervisor. No transcript or output files are written by the client itself.

Clearing follows enrolled roles through the primary lease in `~/.chorus/sessions/v2` and its `agent-events/<session>.jsonl`, honoring `CHORUS_AGENT_STATE_DIR`. It does not choose the newest transcript. Enrolled roles with missing, ambiguous, disconnected, or corrupt evidence show a visible gap; they never fall back to Claude transcripts. Roles with no registry entry retain the legacy tailer.

Canonical cursors and pending reply candidates persist in `agent-event-cursors/clearing.json` (`CLEARING_AGENT_CURSOR_FILE` overrides). The first binding starts at the current complete EOF; subsequent starts replay entries after the saved cursor. Set `CLEARING_AGENT_REPLAY_HISTORY=1` only to deliberately import existing history on an initial binding. `turn.completed` confirms the final response; earlier assistant narration remains commentary. Partial JSON and UTF-8 writes are retained for the next read. No heartbeat age is assumed; optional `CLEARING_SESSION_LEASE_MS` requires a deployment that renews leases while idle.
