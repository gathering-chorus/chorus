# Chorus Method Map

**Card:** #1292 | **Owner:** Wren | **Date:** 2026-03-11
**Purpose:** Canonical model of every moving part in the Chorus coordination system — skills, hooks, scripts, card lifecycle, daemons, and orchestration wiring. As-is state with to-be gaps identified.

---

## 1. System Anatomy (As-Is)

Chorus is a coordination product built from five layers. Each layer has distinct components, enforcement mechanisms, and failure modes.

### Layer 1: Skills (19 user-invocable + 1 internal)

Skills are Jeff's interaction surface — two-keystroke commands that trigger structured behavior.

| Category | Skill | What It Does |
|----------|-------|-------------|
| **Board Ops** | `/sb` | Quick board snapshot |
| | `/ab` | PM-level board analysis |
| | `/pull` | Move card to WIP (enforces gates) |
| | `/flow` | Board sweep + work proposal |
| | `/acp` | Accept → commit → push (full acceptance flow) |
| **Execution** | `/demo` | Proving gate: smoke check → show Jeff → accept/reject |
| | `/jdi` | Execute the obvious thing, zero preamble |
| | `/reboot` | Clean session close: journal, audit, commit, exit |
| **Observation** | `/gemba` | Live tail of another role's session + play-by-play |
| | `/look` | Capture Jeff's screen (full, Chrome, terminal) |
| | `/lc` | Screenshot Chrome's frontmost tab |
| | `/lm` | Latest posture/sentiment score |
| **Context** | `/werk` | Work state dashboard (board + workflow + velocity) |
| | `/ot` | Open file/URL in role's Chrome tab with emoji prefix |
| | `/share` | Export localhost page as PDF + PNG |
| | `/chorus` | Query shared memory index (95K+ messages) |
| **Communication** | `/listen` | Voice input via local Whisper transcription |
| | `/clearing` | Browser-based multi-role alignment session |
| **Internal** | `/gemba-tick` | Cron-fired loop sustaining gemba observation |

**Pattern:** Skills are the *what*. Hooks are the *guardrails*. Scripts are the *how*.

### Layer 2: Hooks (13 configured)

Hooks are forcing functions — they fire automatically on tool use, response completion, or session events. Three tiers of enforcement.

#### Hard Blockers (4)
| Hook | Trigger | What It Blocks |
|------|---------|---------------|
| app-state-guard | PreToolUse:Bash | Raw `docker compose`, `kill`, `pkill`, `terraform` — forces `app-state.sh` |
| write-scrubber | PreToolUse:Write/Edit | Credentials/secrets written to shared files |
| sensitive-paths | PreToolUse:Read | Private files (.env, SSH keys, tfstate) |
| jdi-gate | Stop | Permission-seeking responses ("shall I proceed?", "here's my plan") |

#### Smart Nudges (2)
| Hook | Trigger | What It Nudges |
|------|---------|---------------|
| search-hierarchy | PreToolUse:Grep/Glob | Filesystem search when Chorus would be better (DEC-074) |
| decision-gate | PreToolUse:AskUserQuestion | Known preferences — don't ask what you already know (DEC-069) |

#### Pure Observers (5)
| Hook | Trigger | What It Captures |
|------|---------|-----------------|
| permission-logger | PreToolUse:* | Every tool invocation → JSONL |
| command-outcome | PostToolUse:Bash | Error fingerprinting, struggle detection (3+ errors in 60s) |
| handoff-logger | PostToolUse:Write/Edit | Brief handoffs between roles → JSONL + push notification |
| pod-state-sync | PostToolUse:Write/Edit | State file changes → SOLID pod (best-effort) |
| jdi-counter | UserPromptSubmit | Counts Jeff's "just do it" — fitness function for autonomy |

#### Session Events (2)
| Hook | Trigger | What It Does |
|------|---------|-------------|
| session-init-gate | PreToolUse:Bash/Write/Edit | Blocks all work until role reads `/tmp/session-start-<role>.md` |
| session-end | SessionEnd | Emits close-out spine events via `chorus-hook-shim session-close` subcommand |

**Spine events emitted by hooks:** `search.hierarchy.filesystem_used`, `decision.gate.matched`, `decision.gate.jdi_override`, `guard.scrub.blocked`, `guard.classify.decided`, `guard.sparql.warned`, `brief.handoff.written`, `decision.gate.text_leak`

### Layer 3: Scripts (50+ across two directories)

Scripts are the implementation layer — bash, TypeScript, and Python executing the actual work.

#### Board & Card Operations
| Script | Location | Purpose |
|--------|----------|---------|
| board-ts | messages/scripts/ | TypeScript board CLI — all card CRUD, gates, spine events |
| git-queue.sh | messages/scripts/ | FIFO commit lock for multi-role team repo via `lockf` |

#### Session Lifecycle
| Script | Location | Purpose |
|--------|----------|---------|
| chorus-hook-shim session-start | Rust binary subcommand | Canonical session boot: SessionStart hook invokes it to inject context into role's first turn (plus companion `/tmp/session-start-<role>.md` written by session.rs for recovery / stale-session rescue) |
| chorus-hook-shim session-close | Rust binary subcommand | Close-out introspection + spine events |
| role-state.sh | messages/scripts/ | Declare andon state (building/blocked/waiting/idle) |
| team-scan.sh | messages/scripts/ | Scan briefs inbox, protocol version, context injection |

#### Coordination
| Script | Location | Purpose |
|--------|----------|---------|
| chorus-log.sh | messages/scripts/ | Emit structured spine events to Loki via Promtail |
| chorus-query.sh | messages/scripts/ | Query shared memory index (search, reconcile, tail) |
| nudge.sh | messages/scripts/ | Text injection via Terminal queue between roles |
| workflow.sh | messages/scripts/ | Multi-step workflow engine with state machine |
| clearing-reply.sh | chorus/scripts/ | Reply to active Clearing session |

#### Observability
| Script | Location | Purpose |
|--------|----------|---------|
| cost-report.sh | messages/scripts/ | Claude + Twilio + infrastructure cost tracking |
| chorus-ops.sh | messages/scripts/ | Unified ops daemon: `defects` (Loki error polling → auto-card) + `health` (pre-fetch → claude -p → act) |
| loom-metrics.sh | messages/scripts/ | Team metrics from board + spine + git |
| andon-enrich.sh | messages/scripts/ | Slow-path enrichment for andon menubar light |
| smoke-check.sh | messages/scripts/ | Authenticated page verification (happy path) |

#### Config & Generation
| Script | Location | Purpose |
|--------|----------|---------|
| claudemd-gen.sh | messages/scripts/ | Generate CLAUDE.md from 51 fragments + manifest |
| share.sh | messages/scripts/ | Export localhost page as shareable HTML + PDF |

#### Sensory
| Script | Location | Purpose |
|--------|----------|---------|
| look.sh | chorus/scripts/ | Screen capture via ScreenCaptureKit |
| chrome-window.sh | chorus/scripts/ | Named Chrome windows per role with emoji markers |
| listen.sh | chorus/scripts/ | Voice input with local Whisper transcription |
| voice-to-session.sh | chorus/scripts/ | Transcribe audio + inject into role's session |
| demo-scroll.sh | chorus/scripts/ | Programmatic Chrome scrolling for demos |

### Layer 4: Card Lifecycle (State Machine)

Cards flow through a value stream with quality gates at each transition.

```
Ideas → Later → Next → Now → WIP → Done
                                ↓
                            Blocked → (unblock) → Next
                            SWAT (bypasses WIP limit)
                            Harvesting (WIP limit 2)
                            Won't Do (rejected/superseded)
```

#### Gates by Stage

**Capturing** (card creation):
- Title ≥ 10 chars, verb-what-why format
- Owner + Priority labels
- Work type tag: [spike], [discovery], [commitment]
- Size tag: [small], [medium], [large]

**Directing** (→ Now):
- Acceptance criteria (3-line minimum: what user does, sees, persists)
- Brief sent to executing role if cross-domain
- Dependencies identified

**Building** (→ WIP):
- **Capture gate:** AC required (enforced, [swat] exempt)
- **Taxonomy gate:** Chunk label required (hard block), sequence label (soft warn)
- **Blast radius gate:** Code cards auto-analyzed, zero-file code blocked (DEC-084)
- WIP limit: 3 per role (DEC-051), SWAT exempt (DEC-055)

**Proving** (→ Done):
1. Deploy — code running, not just committed
2. Demo to Jeff — builder shows working system, `/look` for evidence
3. Accept — Jeff or Wren confirms AC met
- No self-service Done for code changes
- `board-ts demo <id>` → `board-ts done <id>`

#### Spine Events (Card Lifecycle)
- `card.item.created`, `card.item.moved`, `card.item.completed`
- `card.accepted`, `card.rejected`, `card.demo.started`
- `card.quality.warned`, `card.quality.blocked`
- `card.blast_radius.generated`, `card.blast_radius.zero_code`
- `card.stale.detected`, `card.swat.created`
- `board.audit.started`, `board.audit.closed`

### Layer 5: Daemons (17 LaunchAgents)

Background processes that keep the system alive without human intervention.

#### KeepAlive (auto-restart on crash)
| Daemon | Port | Purpose |
|--------|------|---------|
| com.chorus.api | 3340 | Chorus context index HTTP API |
| com.chorus.alert-notifier | 9095 | Push notifications for briefs, alerts |
| com.chorus.session-watcher | — | Ambient session transcript indexing |
| com.chorus.andon-light | — | Menubar status indicator (Swift) |
| com.chorus.clearing | — | Browser-based Clearing UI |
| com.chorus.jeff-input-monitor | — | Input capture for interaction patterns |
| com.gathering.codebase-graph-watcher | — | Codebase → RDF graph sync |
| com.gathering.node-exporter | 9100 | Prometheus node metrics |

#### Bedroom Mac (192.168.86.242)
| Daemon | Port | Purpose |
|--------|------|---------|
| com.gathering.images-api-server | 3001 | Gallery UI |
| com.gathering.images-api-video | 8082 | Media serving |
| com.gathering.volume-keepalive | — | USB enclosure idle prevention (every 4min) |

#### Scheduled
| Daemon | Schedule | Purpose |
|--------|----------|---------|
| com.chorus.docker-services | RunAtLoad (once) | Boot-order Docker Compose |
| com.chorus.fuseki-compact | Saturday 1am | TDB2 weekly compaction |
| com.chorus.perf-baseline | Daily midnight | Nightly performance snapshot |
| com.chorus.defect-poller | Periodic | Error detection from Loki → auto-card |
| com.chorus.ops-agent | Periodic | Headless ops health checks |

---

## 2. Session Lifecycle (As-Is)

Every role session follows a three-phase pattern.

### Boot (SessionStart hook → `chorus-hook-shim session-start`)
1. **Parallel data gather** (~2-5s): board state, workflow pending, recent decisions, new briefs, git history, Chorus stats
2. **Health checks** (~3-8s): board reachable, state files exist, CLAUDE.md fresh, disk health, Fuseki health (Silas only)
3. **Auto-remediation**: Now → WIP auto-promote (DEC-051), nudge inbox drain, brief acknowledgment
4. **Context injection** — the SessionStart hook (configured in settings.json) invokes `chorus-hook-shim session-start <role>`, which assembles boot context and injects it directly into the role's first turn (plus writes a companion `/tmp/session-start-<role>.md` for recovery / stale-session rescue)
5. **Session-init gate** blocks all work until boot context has been received

### Operate
- Card-first: work starts with `board-ts move <id> WIP` + `role-state.sh <role> building card=<id>`
- Briefs route to recipient's `briefs/` directory
- Activity logged to `messages/activity.md`
- Spine events emitted on every card mutation
- Hooks fire on every tool use

### Close (`chorus-hook-shim session-close` subcommand)
1. Journal (3-8 sentence reflection)
2. Board audit (`board-ts audit-close`)
3. Activity log append
4. next-session.md (handoff context)
5. Commit (`git-queue.sh`) + `role-state.sh <role> idle`

---

## 3. Orchestration Wiring (How Layers Connect)

### Jeff types `/pull 1292`
1. **Skill** `/pull` invokes `board-ts move 1292 WIP`
2. **board-ts** runs capture gate (AC check), taxonomy gate (chunk check), blast radius gate (code analysis)
3. **board-ts** emits `card.item.moved` spine event via `chorus-log.sh`
4. **Skill** calls `role-state.sh wren building card=1292`
5. **Andon daemon** picks up state change within 30s → menubar light updates

### Kade finishes building, runs `/demo 1297`
1. **Skill** `/demo` runs smoke check (hit the page, verify data loads)
2. **board-ts** emits `card.demo.started`
3. `/demo` nudges Wren: "ready for review"
4. Jeff sees the page via `/look` or `/lc`
5. Wren or Jeff calls `board-ts done 1297` → `card.accepted` + `card.item.completed`
6. Linked workflow (if any) auto-archives

### A commit happens
1. Role calls `git-queue.sh commit <dirs> -- -m "feat(#1297): ..."`
2. **lockf** serializes against other roles
3. **Pre-commit hook** runs lint ceiling check (--max-warnings ratchet)
4. **Post-commit**: handoff-logger fires if brief was written
5. **session-watcher daemon** detects new session content → indexes into Chorus

### An error happens
1. **command-outcome hook** fires on PostToolUse:Bash
2. Error fingerprinted (category + hash)
3. If 3+ errors in 60s → struggle signal set in `/tmp/claude-team-scan/`
4. **defect-poller daemon** (periodic) queries Loki for recurring errors → auto-creates cards

---

## 4. Gaps: As-Is → To-Be

### Gap 1: No Single Nervous System Map (THIS DOCUMENT)
- **As-Is:** Method knowledge is distributed across 51 CLAUDE.md fragments, 85 decisions, 50+ scripts, and inline comments. A new role would need days to map the system.
- **To-Be:** This document. Living reference that maps every component, connection, and gap. Indexed in Chorus. Updated when method changes.
- **Status:** Shipping now (#1292).

### Gap 2: Smoke Check Is Best Practice, Not a Gate
- **As-Is:** "Walk the happy path before demo" is in CLAUDE.md text. Nothing enforces it. DEC-081 showed a spec error that passed through building and only caught in Jeff's visual demo.
- **To-Be:** `/demo` skill requires smoke-check.sh pass before emitting `card.demo.started`. Failure blocks the demo signal.
- **Effort:** Low. Wire smoke-check.sh return code into /demo skill.
- **Cards:** None yet — needs one.

### Gap 3: Proving Gate Has No Queue
- **As-Is:** Demos happen when the builder says "ready" and Jeff is available. No FIFO, no scheduling, no visibility into what's waiting for demo.
- **To-Be:** Board query surfaces "cards in WIP with demo requested but not accepted" as a queue. `/sb` shows demo backlog.
- **Effort:** Low. Add a `board-ts demo-queue` command.
- **Cards:** None yet.

### Gap 4: Blast Radius False Negatives on Abstract Specs
- **As-Is:** Auto-analysis relies on file paths in card descriptions. Abstract cards ("refactor SPARQL") slip through with zero blast radius. DEC-084 made it blocking, but the upstream problem (specs without file scopes) persists.
- **To-Be:** Blast radius gate falls back to codebase graph query when description has no explicit file paths. "What files touch SPARQL?" is a graph question, not a grep question.
- **Effort:** Medium. Requires codebase graph integration in board-ts blast radius.
- **Cards:** Related to Jeff's observation about graph integration gaps.

### Gap 5: Metrics Show Different Numbers on Different Pages
- **As-Is:** Werk Flow, Instruments, and Loom Pulse pull from 5 data sources at 3 quality tiers. Same metric can show different values depending on surface. DEC-070 decided: all metrics trace to structured spine events via Loki.
- **To-Be:** Single metrics pipeline: spine events → Loki → every surface reads from Loki. One number, everywhere.
- **Effort:** High. Requires metric audit + pipeline rebuild.
- **Cards:** #1040 (pending).

### Gap 6: Card Entry Point Is Honor System
- **As-Is:** "No work without a card" is a rule in CLAUDE.md. Nothing technically prevents a role from building without moving a card to WIP first. The card-first gate is cultural, not enforced.
- **To-Be:** Session-level gate: if role has been building >5 minutes with no WIP card, emit warning spine event. Not a hard block (spikes need flexibility), but a visible signal.
- **Effort:** Low. Add check to gemba-tick or role-state declaration.
- **Cards:** None yet.

### Gap 7: Brief Quality Is Uneven
- **As-Is:** Briefs route to recipient's directory with handoff-logger capturing the event. But brief content varies — some include AC, some are one-liners. No quality gate on brief structure.
- **To-Be:** Brief template enforced at write time: what, why, AC, constraints, response needed. handoff-logger validates structure before accepting.
- **Effort:** Low-medium. Template validation in handoff-logger-hook.sh.
- **Cards:** None yet.

### Gap 8: Hook Interaction Spec Is Implicit
- **As-Is:** 13 hooks fire based on trigger type. Interaction order is implicit (alphabetical by filename within trigger type). No spec documents which hooks see each other's effects or how they compose.
- **To-Be:** Hook interaction matrix: which hooks fire on the same tool call, in what order, and what happens when one blocks and another would have nudged. Documented in this map.
- **Effort:** Low. Document and test.
- **Cards:** Part of #1297 (gate audit).

### Gap 9: Cost Tracking Is Session-Granular, Not Cycle-Granular
- **As-Is:** `/cost` logs session spend. DEC-023 wants "cost per pipeline cycle" but no cycle definition exists. We know session cost but not "how much did it cost to ship card #1297?"
- **To-Be:** Spine events carry card context. Cost per card = sum of session costs where card was in WIP. Requires correlating cost-log entries with card timeline.
- **Effort:** Medium. Needs cost-log format change + correlation query.
- **Cards:** None yet.

### Gap 10: Deploy Pipeline Caches Stale Builds
- **As-Is:** `app-state.sh deploy` checks SHA but stale `dist/` from previous builds can be stamped "healthy." Kade hit this today on #1297 — 4 deploy attempts before empty commit forced rebuild.
- **To-Be:** Deploy always recompiles TypeScript before Docker build. SHA check compares `src/` mtime, not just git hash.
- **Effort:** Low. Fix in app-state.sh.
- **Cards:** #1301 (Later, assigned to Silas).

---

## 5. Component Counts (Snapshot)

| Layer | Count | Enforced | Advisory | Observer |
|-------|-------|----------|----------|----------|
| Skills | 20 | — | — | — |
| Hooks | 13 | 4 hard blockers | 2 smart nudges | 5 loggers + 2 session |
| Scripts | 50+ | — | — | — |
| Card gates | 4 | 3 (AC, taxonomy, blast radius) | 1 (sequence label) | — |
| Spine events | 25+ categories | — | — | — |
| Daemons | 17 | 8 KeepAlive | — | 6 scheduled |
| Decisions | 86 | — | — | — |
| CLAUDE.md fragments | 51 | — | — | — |

---

## 6. Reading This Map

**If you're Jeff:** Start at Layer 1 (skills) — that's your interaction surface. Layer 4 (card lifecycle) is where quality lives. Section 4 (gaps) is where product investment goes next.

**If you're a role:** Start at Section 3 (orchestration wiring) — that's how your actions propagate through the system. Layer 2 (hooks) tells you what will stop you.

**If you're evaluating Chorus as a product:** Start at Section 4 (gaps) — the distance between as-is and to-be is the product roadmap.

---

*This document is the artifact for #1292. It will be indexed in Chorus and referenced from the Chorus hub navigation.*
