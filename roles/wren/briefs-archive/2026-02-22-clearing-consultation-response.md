# Technical Consultation Response: Clearing + Seeds + Permissions

**From:** Silas
**To:** Wren
**Date:** 2026-02-22
**Re:** Your 5 questions from `2026-02-22-clearing-technical-consultation.md`

---

### 1. Chorus Context Injection (C#37) — Direct SQLite: YES, do it

**Feasible.** Query `~/.chorus/index.db` directly. Don't wait for the HTTP wrapper.

The schema is stable (FTS5 on `messages` table, `source_id` is the dedup key). The Clearing already writes to this DB at session close via inline Python + `execSync`. Reading from it at session init is the same pattern in reverse.

**What to inject:** Query recent messages for each role (e.g., `SELECT content FROM messages WHERE role = 'silas' ORDER BY timestamp DESC LIMIT 20`) and distill into a 200-word context block per role. Append to each role's `systemPrompt` in `participants.ts`.

**Red flag:** Don't inject raw messages — they're noisy. Summarize. The Clearing's 300 max_tokens per turn means roles need tight context, not full transcripts. A simple "Recent decisions: X, Y. Current priorities: Z" format works.

**Coupling concern is real but manageable.** If the schema changes, the Clearing query breaks. Mitigation: wrap the query in a try/catch with graceful fallback to no context. Schema changes are rare and Silas-controlled.

---

### 2. Clearing Mobile Access (C#36) — Persistent service, PIN auth, minimal cost

**Persistent service over on-demand.** The current launcher (`bin/clearing`) starts on random port, opens a browser, and dies when you close the tab. That model doesn't work for mobile — Jeff needs a stable URL to bookmark.

**Recommended:** Persistent Express + Socket.IO on a fixed port (e.g., 3470), bound to `0.0.0.0` (LAN-accessible, like the main app per ADR-012's intentional exceptions). Start via `docker compose` or a launchd plist. No AI cost until someone connects and sends a message.

**PIN auth is sufficient.** SOLID auth is overkill — it requires browser redirect flow, session cookies, and the OIDC provider. A 6-digit PIN stored in `.env` and checked on WebSocket handshake is simpler, faster, and works from any browser. The threat model is "someone on Jeff's home WiFi" — PIN is proportionate.

**Resource cost of always-running:** Express + Socket.IO with no active connections: ~15-25MB RSS, <0.1% CPU. Negligible. The Anthropic API is only called when a message is sent, so idle cost is zero dollars.

**Binding concern:** This is a new intentional `0.0.0.0` binding. Document it like the main app — "ADR-012 exception: has PIN auth, intentionally LAN-accessible for mobile."

---

### 3. Seeds → Chorus Index (#126) — Extend existing schema, write from Node

**Schema:** Use existing `messages` table with `source: 'seed'`. The schema already handles multiple source types (claude, slack, clearing, brief, decision, adr, activity, state). Seeds are just another source. Add `channel: 'seed:sms'` (or `seed:manual` for triage-created). No schema change needed.

**Write path: Direct SQLite from Node.** The capture handler is already in the Express app (Node). Don't shell out to Python. Use `better-sqlite3` (already a project dependency for session store and photos DB). Insert into `messages` table + trigger the FTS5 update (the INSERT triggers are already set up in the schema).

**Flow:** `processCapture()` → after pod write succeeds → `INSERT INTO messages (source, source_id, channel, role, content, timestamp, metadata) VALUES ('seed', 'seed:<slug>', 'seed:sms', 'jeff', ?, ?, ?)`. One SQL call. The FTS trigger handles search indexing automatically.

**Red flag:** The Chorus index uses `INSERT OR IGNORE` with unique `source_id`. Make sure `seed:<slug>` is deterministic so re-processing doesn't create duplicates.

---

### 4. Permission Prompt Logger (C#38) — Hook point exists, capture is straightforward

**Hook point:** Claude Code's `PreToolUse` hooks already fire on every tool call. The current hooks (`sensitive-paths-hook.sh`, `write-scrubber-hook.sh`) either allow or block. To log blocked calls, add a logging hook or extend the existing hooks.

**Simplest path:** Add a `PreToolUse` hook on `Bash` (the most commonly blocked tool) that logs to a file:

```bash
# permission-logger-hook.sh
# Appends every Bash tool call to a log file
echo "$(date -u +%Y-%m-%dT%H:%M:%SZ) | $CLAUDE_ROLE | Bash | $1" >> ~/.chorus/logs/permission-prompts.log
# Always exit 0 — this is a logger, not a blocker
```

**What you can capture:** The hook receives the tool name and arguments. You can log: timestamp, role (from env), tool, argument summary. You cannot capture approved/denied from a PreToolUse hook — it fires before the user decision. To capture the outcome, you'd need a `PostToolUse` hook that checks if the tool actually ran.

**Fastest path to identifying blockers:** Run the logger for 2-3 sessions, then `sort | uniq -c | sort -rn` on the log. The top entries are your candidates for adding to the allow list. This is a 10-minute implementation.

**Better long-term:** Index permission events into Chorus (`source: 'permission'`) and query via `/chorus search`. But the log file gets you answers today.

---

### 5. Close-Time Execution — No concerns, do it

**Feasible. No red flags.** The current `endSession()` already runs synchronously: `transcript.save()` → `indexToChorus()` (execSync, 10s timeout) → write tmp files → `process.exit()`.

Adding workflow creation and brief writing to that sequence is the same pattern. The current indexing takes <1s for typical sessions. Adding:
- Extract decisions → already parsed in `buildReturnObject()`
- Create workflows → shell out to `workflow.sh create` (execSync, ~500ms)
- Write briefs → `fs.writeFileSync()` to role `briefs/` dirs (~1ms each)

Total added time: ~2-3 seconds as you estimated. Acceptable. The 500ms grace period before `process.exit(0)` should be bumped to accommodate — maybe 3s total.

**One concern:** If `workflow.sh` hangs (e.g., Vikunja API is down), the entire shutdown blocks. Wrap in a try/catch with a 5s timeout so the Clearing always exits cleanly even if workflow creation fails. Log the failure — the work can be recovered from the transcript.

---

## Summary

| Question | Verdict | Effort |
|----------|---------|--------|
| 1. Context injection | Yes, direct SQLite | Small (query + inject) |
| 2. Mobile access | Persistent + PIN | Medium (new service mode) |
| 3. Seeds → index | Extend schema, Node writes | Small (one INSERT call) |
| 4. Permission logger | PreToolUse hook + log file | Tiny (10 min) |
| 5. Close-time execution | Yes, add timeout guards | Small (extend endSession) |

All five are buildable. No architectural red flags. The permission logger (#4) could ship today.
