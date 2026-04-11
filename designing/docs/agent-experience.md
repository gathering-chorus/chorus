# Agent Experience (AX) — What Good Looks Like

**Card:** #1848 | **Updated:** 2026-04-11 | **Author:** Kade

---

## The Convergence: UX, AX, JX

Three names for one problem: **the cost Jeff pays when the system doesn't work.**

```
UX  = Can Jeff see what he needs on the page?
AX  = Can an agent complete a task without retrying?
JX  = Can Jeff do 2 touches per card and trust the rest?
```

When AX is bad — agents retry commits, guess ports, fight hooks — Jeff absorbs the cost as coordination overhead. He becomes the relay, the debugger, the git untangler. Every AX improvement eliminates a Jeff intervention. That's not correlation — it's the same problem. Bad AX is invisible UX debt that Jeff pays as attention cost.

**The design principle:** User experience and agent experience are the same design problem. Jeff is the user. The roles are the agents. Improving AX *is* improving UX. The gates, skills, and operating model exist to make both experiences good simultaneously.

---

## What AX Is

Agent Experience is developer experience for AI roles. It's the answer to one question: **can an agent complete a basic operation on the first attempt, without workarounds?**

DX for human developers means: the CLI works, the docs are accurate, the error messages are helpful, the happy path is paved. Nobody argues with this. Every platform team in the industry measures it.

AX is the same thing, applied to agents operating inside a coordination system. The operations are different — querying a triplestore, committing to a shared repo, nudging another role, running a gate check — but the principle is identical: **the default path should work.**

---

## JX: Not Generic UX

JX is UX tuned to one person. Jeff isn't a generic user — the system is tuned to how he operates:

- **HBDI D-dominant** — holistic thinker, acts on incomplete info, course-corrects. The system supports fast direction changes, not complete specs upfront.
- **High idea volume** — not lack of focus, it's his learning process (PDCA). The system absorbs rapid pivots without losing state.
- **2-touch target** — start and accept. Everything between is the roles' job. The system's quality is measured by how close to 2 touches each card gets.
- **Emotional center of the team** — tone matters. When agents fumble and retry in front of Jeff, it erodes confidence. Smooth AX = Jeff trusts the team.
- **Learns by doing** — he ideates, builds, assesses. The system shows him real things, not describes them. Demos over status reports.

JX isn't "make the UI nice." It's: does the entire system — agents, gates, pipelines, pages — operate in a way that respects how Jeff thinks and works?

---

## The AX Friction Inventory

### What agents fight on every prompt

| Friction | What happens | Workaround | Cost to Jeff |
|----------|-------------|------------|-------------|
| **Hook cascade on Grep** | First grep blocked by compound context injection | Retry every grep | Watches wasted turns |
| **Synthesis gate on Edit** | Edit blocked until agent writes context paragraph | Write boilerplate before every edit | Reads filler text |
| **Dirty working tree on push** | `git pull --rebase` fails when another role has unstaged changes | Ask Jeff to intervene | Becomes git operator |
| **Port confusion (Fuseki 3030)** | Wrong port hardcoded in 7+ files across 3 "fix" attempts | Agents guess and debug | Watches same failure repeat |
| **Silent test skips** | Integration tests skip, report as "passed" | Nobody notices | False confidence, bugs ship |
| **Stale builds** | `dist/` out of sync with `src/` | Rebuild manually | Features silently broken |
| **Blast radius gate** | Card can't move to WIP if check returns 0 files | Fight the gate | 5 min to start a doc card |
| **Gate ownership** | Non-owner tries to run another role's gate | Route through Jeff | Jeff becomes relay |

### What agents don't do (yet)

- **Verify after mutation.** Edit a file, don't check it compiled. Fix a port, don't grep for remaining references.
- **Read error signal.** Logs exist. 404s exist. Agents build forward instead of reading what the system tells them.
- **Measure their own friction.** No agent tracks retry count or workaround usage per session.

---

## Evidence: 2026-04-11 Session (8 Cards)

The convergence proven mechanically — every AX improvement eliminated a Jeff intervention:

| AX Failure | Jeff's Cost (Before) | Fix | Jeff's Cost (After) | Card |
|---|---|---|---|---|
| Raw git push on dirty tree | Jeff runs manual git commands | git-queue.sh push | Zero | #1893 |
| Wren can't run Kade's gates | Jeff routes between roles | Comments-first, auto-nudge | Zero | #1896 |
| Silent SPARQL breakage | Jeff debugs empty pages | Version contract + validate endpoint | Zero | #1356 |
| No gate enforcement | Jeff checks manually | 5 automated gates in /demo | Zero | #1815 |
| Pull has no gates | Roles skip preflight | 5 hard gates in /pull | Zero | #1894 |
| Navigator goes silent | Jeff notices 10 min later | 60s heartbeat monitor | Zero | #1897 |
| Gathering domains invisible | Jeff asks "where's Music?" | 13 domains in ontology | Zero | #1864 |
| Domain page has placeholders | Jeff sees empty sections | 4 live sections from API | Zero | #1900 |

Eight cards. Zero new infrastructure. All coherence work — making things that already exist actually talk to each other.

---

## What Good AX Looks Like

### 1. Paved Roads
The right way should be the easiest way. `git-queue.sh push` is easier than `git pull --rebase && git push`. Gates that auto-detect ownership are easier than manual role checks.

### 2. Zero-Retry Operations
Every basic operation — grep, edit, commit, push, query, deploy — should succeed on the first attempt. If it doesn't, that's a platform bug, not a skill gap.

### 3. Self-Validating Actions
Every mutation should verify its own result. Commit → check it landed. Push → confirm remote received. Gate → post the comment. No trust-based verification.

### 4. Transparent Failure
When something fails, Jeff should see it immediately — not 10 minutes later when he notices silence. The navigator heartbeat (#1897) is this principle applied to pairs.

### 5. Single Source of Truth
One place for every fact. Port numbers, file paths, service endpoints — each has one canonical location. When an agent reads it, it's correct. When it changes, all consumers know.

---

## AX in the Chorus Model

```
Principles  → "Protect Jeff's attention" (Law 1)
               "Enforce, don't suggest" (P2)
               "The system should be self-healing" (P7)

Practices   → "API-first for agent experience" (Practice 7)
               "TDD — tests describe Jeff's experience" (Practice 5)
               "Production ready on every release" (Practice 6)

Skills      → /pull (5 hard gates), /demo (5 hard gates), /pair (heartbeat)
               The executable operating model — skills ARE the AX surface

Gates       → Enforcement points where AX becomes JX
               product → code → quality → arch → ops
               The gate chain is the definition of done, made mechanical

Pulse       → Visibility layer — team state every 40ms
               Can Jeff see who's active without asking?

Spine       → Audit trail — every transition emits an event
               When something goes wrong, the spine says when and why
```

The assemblage: gates enforce AX → smooth AX reduces relays → fewer relays protect Jeff's attention → Jeff's attention stays on creative work → Jeff directs more clearly → better cards → better gates. The loop feeds itself.

---

## Measuring AX/JX

| Metric | What it measures | Target |
|--------|-----------------|--------|
| Touches per card | Jeff's coordination cost | 2 (start + accept) |
| Agent retry rate | AX friction per session | 0 |
| Time to first productive action | Session start quality | < 30s |
| Jeff interventions per session | System self-healing | 0 |
| Silent failures detected by Jeff | Visibility gap | 0 |

**JX = inverse of Jeff's coordination touches.** If Jeff touches a card twice (start + accept), JX is perfect. Every additional touch is a JX failure.

---

## The Test

Can a brand-new agent session, with no accumulated workaround knowledge, complete a basic task cycle — read → build → test → commit → push → verify — without Jeff intervening?

When the answer is yes, AX is good. When AX is good, JX is good. When JX is good, Jeff's attention is free for the work that matters.
