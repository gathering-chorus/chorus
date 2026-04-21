# Spine Architecture — Chorus Coordination Infrastructure

Last updated: 2026-02-25 | Werk v1.3.20

## Overview

The spine is the Chorus coordination infrastructure — everything that makes the three-role team (Wren, Silas, Kade) operate without Jeff being the relay. It comprises **12 components** organized in **6 functional layers**, spanning ~35 scripts, 6 hooks, 5 skills, and a real-time chat service.

Design principle: Jeff moves a card to Now → the system takes over → roles pick up work → flows to Done. Jeff is the pull signal, not the relay.

## Functional Layers

### Layer 1 — Session Lifecycle (enforcement)

| Component | Function | Trigger | Key Files |
|-----------|----------|---------|-----------|
| SessionStart hook → `chorus-hook-shim session-start` | Parallel board reads, brief checks, workflow steps, CLAUDE.md staleness. The SessionStart hook (configured in settings.json) invokes the `chorus-hook-shim session-start <role>` subcommand, which injects boot context directly into the role's first turn (plus writes companion `/tmp/session-start-<role>.md` for recovery / stale-session rescue) | SessionStart hook (automatic) or manual | Rust binary: `chorus-hook-shim` (subcommand `session-start`) |
| `session-init-gate.sh` | Blocks Write/Edit/Bash until role reads context file. Marker lifecycle: `.pending` → `.done` | PreToolUse hook (every tool call) | `messages/scripts/session-init-gate.sh` |
| `chorus-audit.sh` | Gate health, disk budget (C1/C2), container health, uncommitted files, retroactive card detection | Session start/close, manual | `messages/scripts/chorus-audit.sh` |

**Session boot parallel operations (SessionStart hook → `chorus-hook-shim session-start`):**
1. `board-ts audit-start` (both boards)
2. `workflow.sh pending <role>` (waiting steps)
3. `handoff-check.sh` (stale handoffs)
4. `claudemd-gen.sh --check` (CLAUDE.md drift)
5. `chorus-capture.sh --count` (intake queue)
6. SessionStart hook injects assembled boot context into the role's first turn

Latency: ~3-5s for full context load.

### Layer 2 — Work Tracking (state)

| Component | Function | Trigger | Key Files |
|-----------|----------|---------|-----------|
| `cards` | TypeScript Vikunja client — card CRUD, audit snapshots, staleness detection (Now >48h, Next >7d), WIP limiting | Manual, `chorus-hook-shim session-start` | `directing/products/cards/src/cli.ts`, `directing/products/cards/src/client.ts` |
| Workflow engine | Multi-step state machine — JSON manifests, auto-handoff briefs, swim lane dashboard | Auto on `board-ts move <id> Now`, manual via `workflow.sh` | `messages/workflow-engine/src/engine.ts`, `messages/scripts/workflow.sh` |
| Briefs protocol | Structured files in recipient's `briefs/` dir, staleness flagging, activity.md audit trail | Manual write, workflow auto-generation | Role `briefs/` directories, `messages/activity.md` |

**Workflow manifest structure (JSON):**
- `id`, `decision`, `source`, `card`, `status`
- `steps[]`: seq, role, action, status, artifacts, brief, timestamps
- `history[]`: event log with timestamps and role attribution
- Storage: `messages/workflows/active/` → `archive/` on completion

**Werk auto-trigger (C#52):** `board-ts move <id> Now` → WorkflowEngine.createManifest() → 2-step workflow (owner builds, reviewer verifies) → brief auto-written to next role's inbox.

### Layer 3 — Shared Awareness (memory)

| Component | Function | Trigger | Key Files |
|-----------|----------|---------|-----------|
| Chorus index | SQLite FTS5 — 31,523 messages across Claude sessions, Clearing transcripts, briefs, ADRs, decisions | Ambient daemon (fswatch, <3s latency) | `chorus/scripts/chorus-index-*.sh`, `~/.chorus/index.db` |
| `/chorus` skill | Query interface — reconcile, search, role activity, stats | Manual invocation | `chorus/scripts/chorus-query.sh` |
| Chorus HTTP API | REST endpoints — search, reconcile, refs, stats, manual index trigger | HTTP on port 3340 | `chorus/api/src/server.ts` |
| Activity.md | Audit trail — who sent what, when, what happened next | Manual append by each role | `messages/activity.md` |

See `chorus-clearing-architecture.md` for full Chorus + Clearing deep dive.

### Layer 4 — Safety (guard rails)

| Component | Function | Trigger | Key Files |
|-----------|----------|---------|-----------|
| `write-scrubber-hook.sh` | Hard-deny credentials (passwords, API keys, tokens) in shared files. Warn on internal patterns (IPs, ports) | PreToolUse on Write/Edit | `messages/scripts/write-scrubber-hook.sh` |
| `sensitive-paths-hook.sh` | Three-tier data classification (Public/Internal/Private) with per-role `.sensitive-paths` manifests | PreToolUse on Read | `messages/scripts/sensitive-paths-hook.sh` |
| `permission-logger-hook.sh` | Logs every permission decision. 2,258 prompts tracked, 74% auto-allows | PreToolUse on all tools | `messages/scripts/permission-logger-hook.sh` |
| `handoff-logger-hook.sh` | Captures multi-role brief handoffs (sender, recipient, artifact path) | PostRunResult hook | `messages/scripts/handoff-logger-hook.sh` |

**Concentric trust model:**
- Outer ring (Chorus/building): cloud AI fine
- Middle ring (Gathering/collections): hybrid
- Inner ring (Self/relationships/journals): local AI only, never leaves Macs

### Layer 5 — Interaction (real-time)

| Component | Function | Trigger | Key Files |
|-----------|----------|---------|-----------|
| The Clearing | Browser-based multi-party chat (Jeff + 3 Haiku roles). DECISION markers, auto-transcript indexing, auto-capture+route (#275) | `/clearing` skill | `chorus/clearing/src/server.ts` |
| `/look` skill | Screen capture for visual context sharing | Manual invocation | `~/.claude/skills/look/` |
| `/werk` skill | Workflow dashboard — swim lanes, role colors, activity log | Manual invocation | `messages/scripts/workflow.sh visualize` |

See `chorus-clearing-architecture.md` for full Clearing deep dive.

### Layer 6 — Configuration (identity)

| Component | Function | Trigger | Key Files |
|-----------|----------|---------|-----------|
| `claudemd-gen.sh` | Fragment assembly → role-specific CLAUDE.md files. Auto-patch versioning on any input change | Manual, session-start staleness check | `messages/scripts/claudemd-gen.sh` |
| Manifest + fragments | 12 shared + per-role fragments, variable substitution (`{{ROLE_NAME}}`, `{{WERK_VERSION}}`), schema validation | claudemd-gen.sh | `messages/claudemd/manifest.json`, `messages/claudemd/shared/`, `messages/claudemd/roles/` |

**Werk version semantics:**
- Major: fundamental system redesign (rare)
- Minor: new sections, new role responsibilities
- Patch: auto-bumped on fragment/variable/settings change (checksum comparison)
- Current: v1.3.20

## Data Flow — The Piece Flow

```
Jeff moves card to Now
  → board-ts auto-creates workflow manifest (C#52)
    → step 1 marked ready for assigned role
      → next session-start detects pending step
        → role works → workflow.sh advance
          → handoff brief auto-generated to next role's briefs/
            → next role's session-start picks it up
              → cycle continues → all steps done → archive

Meanwhile:
  fswatch daemon indexes all session JSONL → chorus index (<3s)
  chorus-audit.sh verifies compliance at session boundaries
  chorus.log captures events → Promtail → Loki → Grafana dashboards
  Permission hooks enforce safety on every tool call
```

## Spine Event System (DEC-053)

Four vertebrae classify all coordination events. Schema at `messages/schemas/spine-events.json` (v1.1.0).

| Vertebra | Role | Example Events |
|----------|------|----------------|
| **Capturing** | Wren | `card_created`, `quality_gate_warn`, `card_moved`, `workflow_created`, `brief_written` |
| **Directing** | Wren | `card_moved`, `card_blocked`, `card_updated`, `card_commented`, `decision` |
| **Building** | Kade | `tsc_compile`, `test_run`, `commit`, `git_push`, `pre_commit_timed` |
| **Proving** | Silas | `deploy_*`, `health_check`, `app_start/restart`, `verification_complete` |
| **System** | (cross-cutting) | `session_start/end`, `alert_*`, `ops_agent_run`, `defect_detected` |

Events flow: `chorus-log.sh` → `chorus.log` → Promtail → Loki → Grafana + /werk spine view.

**Auto-capture pipeline** (#275, 2026-02-25): Clearing sessions now synchronously capture decisions/commitments/actions on close, auto-route to roles via workflows and briefs. Intake queue at `~/.chorus/intake/`.

## Event Streaming

All coordination events flow to structured JSON logs:

| Log | Content | Size | Destination |
|-----|---------|------|-------------|
| `chorus.log` | Card moves, briefs, commits, decisions, deploys, gate results | ~500KB | Promtail → Loki → Grafana |
| `handoffs.log` | Multi-role handoff tracking (sent/received) | ~7KB | Promtail → Loki |
| `permission-prompts.log` | Every permission decision across all tools | ~770KB | Local analysis |

Event emitter: `chorus-log.sh` — structured JSON with timestamp, role, event type, appName, component, domain, message.

## Gate Registry

209 operational rules documented. Current enforcement:

| Tier | Count | Mechanism |
|------|-------|-----------|
| Gate (blocks) | 6 active (G1-G6) | Hooks, pre-commit, CI |
| Checklist (verifies) | 4 session checklists (C1-C4) | chorus-audit.sh |
| Fitness function (measures) | 5 functions (F1-F5) | chorus-audit.sh |
| Doc-only | 191 rules | CLAUDE.md, role context |

Gap: 191 doc-only rules work because roles read CLAUDE.md. If a session bypasses the init gate, those rules are invisible.

Full registry: `architect/chorus/gate-registry.md`

## Integration Map

| Component | Depends on | Used by |
|-----------|-----------|---------|
| SessionStart hook → `chorus-hook-shim session-start` | board-ts, workflow.sh, handoff-check.sh, claudemd-gen.sh | session-init-gate, role startup |
| session-init-gate | boot-context injection marker | Every Write/Edit/Bash call |
| board-ts | Vikunja API (localhost:3456), WorkflowEngine | session boot, audits, manual card ops |
| workflow-engine | board-ts (card linking), briefs/ directories | board-ts (auto-trigger), workflow.sh CLI |
| chorus index | session JSONL, artifact files | /chorus skill, reconciliation, HTTP API |
| claudemd-gen.sh | manifest.json, fragments, settings.local.json | session-start (--check), chorus prompt |
| The Clearing | Anthropic API, chorus index (post-session) | /clearing skill |
| write-scrubber | credential patterns | Every Write/Edit to shared files |
| sensitive-paths | .sensitive-paths manifests | Every Read of classified files |
| chorus-audit.sh | board-ts, git, docker, activity.md | Session start/close |

## Concerns (Active)

### 1. Workflow state is last-write-wins
No locking or conflict detection on workflow JSON files. Safe at current scale (1 human + 3 serial AI sessions) but if two roles advance the same workflow concurrently, state corruption is possible. Low risk today, architectural debt for later.

### 2. Clearing voice quality is thin (C#37)
Haiku roles sound generic. Root causes: thin system prompts (~5 sentences), no /chorus context injection, no temperature control, no stop detection, escalating token counts (full transcript sent each turn). Highest-bandwidth alignment tool underperforming.

### 3. 209 rules, 18 enforced
Gate registry documents 209 rules but only 18 are machine-enforced. The other 191 work because roles read CLAUDE.md. The init gate ensures context is read, but doesn't verify every rule is internalized. Closing this gap is incremental — convert doc-only rules to hooks/checks as the system matures.

### 4. Index lock is fragile
If the ambient watcher crashes mid-index, the lock file stays. Stale-PID check exists in `chorus-query.sh` but not in the watcher itself. Should add self-healing — check PID validity before waiting.

### 5. No autonomous activation (C#15)
The spine is pull-based — roles only process briefs when Jeff starts a session. Claude Code doesn't support timer hooks. Urgent handoffs can sit unread for hours. The "nudge" interaction pattern (cubicle tap → mid-session attention) has no implementation.

### 6. Pre-commit hooks local-only
Team repo pre-commit hook validates TypeScript, tests, secrets — but `.git/hooks/` doesn't travel with the repo. Needs `core.hooksPath` or shared hooks directory for portability.

### 7. Board consolidation freshness (v1.3.4)
Unified board with product labels just landed. `chorus-hook-shim session-start` still runs `--chorus` audit. Scripts may need a verification pass to confirm drain-period routing works correctly.

## Performance

| Operation | Latency |
|-----------|---------|
| Session boot (full context via SessionStart hook → `chorus-hook-shim session-start`) | ~3-5s |
| board-ts audit-start | <1s |
| workflow.sh advance | <1s |
| chorus FTS5 search | ~50ms |
| chorus reconcile | ~200ms |
| Ambient index (session write → searchable) | <3s |
| Clearing token stream | <10ms per token |

## Related Documents

- `chorus-clearing-architecture.md` — Chorus index + Clearing deep dive
- `system-architecture.md` — Full system architecture
- `chorus/gate-registry.md` — Gate registry (209 rules)
- `infrastructure-constraints.md` — Two-machine topology, hard constraints
- `../messages/team-architecture.md` — Team operating model
