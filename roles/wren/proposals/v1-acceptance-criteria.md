# V1 Acceptance Criteria — Gathering + Chorus

**From:** Wren | **Date:** 2026-03-07 | **Status:** Draft for Jeff review

---

## What V1 Means

V1 is the answer to: **"Can Jeff think with his own data, supported by a team that watches itself work?"**

Not feature-complete. Not public. Not perfect. V1 is the system working well enough that Jeff trusts it as a daily tool — for memory, for self-awareness, for creative work, for building what comes next. Two products, one outcome: a person who can see his own life clearly and a method that helps him build on it.

---

## Gathering V1 — "The graph knows me"

### The Graph (bones)

| # | Criterion | How to verify |
|---|-----------|---------------|
| G1 | **All personal domains are in the graph.** Music, Photos, Stories, Blog, Notes, Captures, Social, Codebase — each with its own named graph, typed entities, and domain-specific properties. | `SELECT DISTINCT ?g` returns graphs for all domains. Currently: 37K+ graphs, 16M+ triples. |
| G2 | **Cross-domain connections exist.** Stories reference photos. Blog posts mention tracks. Captures link to ideas. The graph is not siloed — entities connect across domains via `jb:mentions` and `dcterms:references`. | Cross-domain link count > 50. Currently: 34 (shipping more via #1123). |
| G3 | **The Self node is the hub.** Jeff's identity connects to practices, values, intentions, stories. The Self domain is navigable, not just metadata. | Navigate to Self page → see connected practices, values, stories. Self as the integrating node for all domains. |
| G4 | **The ontology models feeling, not just structure.** Concept enrichment vocabulary is defined: emotional annotation (Onyx/MFOEM), somatic markers, vedana (pleasant/unpleasant/neutral), philosophical concepts (assemblage, bricolage). | Turtle file contains enrichment classes and properties. At least one domain has enrichment triples (e.g., a story annotated with emotion + vedana). |
| G5 | **Music data is coherent.** Albums, artists, tracks link correctly. Play counts, source paths, album art are present. Browsing Music page shows accurate, navigable data. | Music page loads < 2s. Artist → albums → tracks navigation works. No orphaned tracks. |
| G6 | **Photos data is coherent.** GPS locations, face detections, dates, albums render correctly. Gallery navigation works. | Photos page loads, filters by date/location/person. Face clusters visible. |

### Search (muscle)

| # | Criterion | How to verify |
|---|-----------|---------------|
| G7 | **Dual-engine search works.** FTS (keyword) and semantic (embedding) search return relevant results across all domains. Hybrid mode combines both via RRF ranking. | `/search?q=resilience` returns stories, blog posts, and notes — not just keyword matches. Response < 500ms. |
| G8 | **Embeddings cover all text content.** Stories, blog posts, notes, captures — all text-bearing entities are embedded in LanceDB via nomic-embed-text. | Embedding count ≈ text entity count. Semantic search returns results from every text domain. |

### Navigation (skin)

| # | Criterion | How to verify |
|---|-----------|---------------|
| G9 | **Mind map home is the front door.** Interactive mind map shows all domains as navigable nodes. Clicking a domain opens its browse view. | Load `/`. See mind map. Click Music → Music page. Click Self → Self page. All domains reachable. |
| G10 | **Knowledge graph visualization works.** D3 interactive view of the full RDF graph with domain filtering, zoom, and click-through to entities. | Load visualization page. Filter by domain. Click a node → navigate to entity detail. Graph renders without hanging. |
| G11 | **System/About is complete.** All architectural and product docs are rendered as browsable HTML pages under System/About with shared chrome (navbar, footer, PDF/share). | Navigate System/About. Every doc loads. Nav works between docs. |
| G12 | **Navbar dropdowns reflect the mind map hierarchy.** Spoke pages are reachable from the global nav without knowing URLs. | Hover navbar → dropdown appears → click spoke page → it loads. |

### Self & Reflection

| # | Criterion | How to verify |
|---|-----------|---------------|
| G13 | **Reflect works.** Local LLM (Mistral on Bedroom Mac) has conversational access to Jeff's stories, practices, and values from the graph. Not a generic chatbot — it knows Jeff's context. | Ask Reflect "What have I written about resilience?" → get an answer grounded in Jeff's actual stories, not hallucinated. |
| G14 | **Stories are first-class.** 86+ stories in the graph, each with title, date, text. Searchable, browsable, connected to the Self domain. Stories feed Reflect's context. | Navigate Stories page. Search for a story by keyword. Story detail shows full text + connections. |

---

## Chorus V1 — "The method watches itself"

### The Spine (nervous system)

| # | Criterion | How to verify |
|---|-----------|---------------|
| C1 | **Spine events flow end-to-end.** Emit → chorus.log → index → queryable via API. Card events, interaction patterns, session lifecycle, brief handoffs all emit. | `chorus-log.sh card.accepted wren card=X` → appears in `chorus search "card.accepted"` within 60s. |
| C2 | **Chorus index is comprehensive.** All Claude session transcripts, spine events, briefs, and decisions are indexed. Hybrid search (FTS + semantic) across all content. | `chorus search <term>` returns results from sessions, spine events, and briefs. 57K+ messages indexed. |
| C3 | **Context loads automatically.** SessionStart hook generates `/tmp/session-start-<role>.md` with active cards, board state, boot checks, and auto-fix commands. Roles boot informed. | Start a session → first read of session-start file provides full context without manual queries. |

### The Board (flow)

| # | Criterion | How to verify |
|---|-----------|---------------|
| C4 | **Card lifecycle is gated.** Capture → Direct → Build → Prove → Accept. Cards require AC before Building. WIP entry checks blast radius. Proving gate: deploy, demo to Jeff, accept. No self-acceptance for code changes. | Create a card without AC → gate blocks entry to Building. Complete a card → `board-ts done` only works after demo signal. |
| C5 | **Board is the single source of truth.** All work items tracked via `board-ts`. Cards have title, owner, priority, status. Spine events fire on every mutation. | `board-ts list` shows current state. Every `move`/`done`/`add` produces a spine event in chorus.log. |
| C6 | **Pull order is deterministic.** Equal priority → smallest first (WSJF). WIP limit 3. Harvesting lane WIP 2. SWAT lane bypasses limits. | Three P2 cards in Next → smallest gets pulled first. Fourth WIP card → blocked by limit. |

### Jeff's State (the mood ring)

| # | Criterion | How to verify |
|---|-----------|---------------|
| C7 | **Andon shows Jeff's composite state.** Keyboard, mouse, posture, prompt sentiment — four signals combine into a legible state indicator. Each role's stat panel shows andon color. | Look at Session Tempo dashboard → Jeff's panel has a color. That color reflects real signal, not default. |
| C8 | **Session tempo is visible.** Prompt rate, turn duration, and role activity render as a time-series chart. Jeff can see the shape of a session — ramp-up, plateau, gaps. | Load Session Tempo dashboard → see Team Rhythm chart with all four participants. Verify against actual session activity. |
| C9 | **Idle and breaks are tracked.** Jeff's idle duration and last break time are visible. The system distinguishes between idle (away) and active (typing/mousing). | Step away for 5 minutes → return → andon shows idle duration and last break. |
| C10 | **Jeff operations dashboard exists.** A single surface showing Jeff's state, energy, connections, practices, and attention cost. The mood ring made real. | Load Jeff ops dashboard → see current state, recent activity, practice schedule, composite energy indicator. |

### Process Quality

| # | Criterion | How to verify |
|---|-----------|---------------|
| C11 | **Decision gate suppresses known-answer questions.** PreToolUse hook on AskUserQuestion checks against jeff-preferences.json. Roles don't ask what they already know. | Role attempts a question matching a known preference → hook suppresses it or logs the suppression. |
| C12 | **Re-prompt rate is tracked and visible.** When Jeff repeats himself, it's measured. Analytics show re-prompt, approval, and correction rates per role. | Load re-prompt analytics → see per-role breakdown with proportional bars and percentages. |
| C13 | **Blast radius is assessed before work starts.** When a card enters WIP, the codebase graph generates a what-it-touches list. | Move a card to WIP → blast radius output appears (files, domains, dependencies). |

### Legibility

| # | Criterion | How to verify |
|---|-----------|---------------|
| C14 | **Decisions are browsable.** All 69+ decisions rendered in a legible, auditable surface — not just a markdown file. Searchable, filterable. | Load decision log surface → browse by date, topic, or role. Click a decision → see full context and rationale. |
| C15 | **The next-sequence pipeline is the v1 scoreboard.** All slices through 4b show shipped (green) status. Visual finish line between 4b and 5 marks the v1 boundary. | Load next-sequence page → Slices 1-4b all green. Slice 5+ clearly marked as v2. |

### Team Coordination

| # | Criterion | How to verify |
|---|-----------|---------------|
| C16 | **Briefs flow without Jeff as relay.** Roles write briefs to each other's directories. Handoffs are logged in activity.md. Jeff doesn't carry context between sessions. | Wren writes brief to `architect/briefs/` → Silas reads it on next session start → work proceeds without Jeff restating. |
| C17 | **Gemba works as shared observation.** Background tail + 30-second commentary loop. Jeff and the observing role discuss what they see. Play-by-play, not raw output. | `/gemba silas` → see what Silas is doing → discuss with Jeff → act on observations without interrupting Silas. |
| C18 | **CLAUDE.md is generated, not hand-edited.** 43+ fragments compose into role-specific CLAUDE.md files via `claudemd-gen.sh`. Changes to shared protocol flow to all roles. | Edit a fragment → run generator → all three CLAUDE.md files update consistently. |

---

## What V1 Is NOT

- **Not public.** No external users. No Cloudflare tunnel required. Localhost is the deployment target.
- **Not voice-first.** /listen and /talk are v2. V1 is keyboard + mouse + screen.
- **Not a product for sale.** Chorus SDK is v2. The method works for Jeff's team first.
- **Not feature-frozen.** V1 is a milestone, not a release. Work continues — but it continues on a foundation that's proven, not aspirational.

---

## Current State vs V1

| Area | Status | Gap |
|------|--------|-----|
| Graph data (G1-G2) | 16M triples, 37K graphs | Cross-domain links need #1123 (link inference) |
| Self node (G3) | Shipped (#1104) | — |
| Concept enrichment (G4) | Proposal accepted (#1121) | Turtle formalization + first annotations needed |
| Music coherence (G5) | WIP (#1092) | Import interrupted, blocked on Mac reboot recovery |
| Photos coherence (G6) | WIP (#1093) | Kade active |
| Search (G7-G8) | Shipped | — |
| Navigation (G9-G12) | Mostly shipped | #1103 (navbar dropdowns) in Next |
| Reflect (G13) | Shipped | — |
| Stories (G14) | Shipped (86 stories) | — |
| Spine (C1-C3) | Shipped | — |
| Board gates (C4-C6) | Mostly shipped | #1085, #1086, #1083 in Now/WIP |
| Jeff's state (C7-C10) | Partially shipped | #1105 (ops dashboard) not started, #1127 (practice overlay) new |
| Process quality (C11-C13) | Shipped | — |
| Legibility (C14-C15) | Not started | #1126 (decision surface), #1106 (journaling) in Next |
| Team coordination (C16-C18) | Shipped | — |

**Estimated gap:** ~10 cards between current state and V1. Most are small or medium. The big unknowns are #1092 (Music, blocked on import) and #1105 (Jeff ops dashboard, design-heavy).

---

## The V1 Story

A year ago, Jeff drew domains on graph paper. Today, 16 million triples model his music, photos, stories, blog, notes, relationships, and codebase. The system knows what he has, how he relates to it, and — with the enrichment layer — how it feels.

Three AI roles coordinate through a protocol that watches itself: spine events, proving gates, re-prompt tracking, blast radius assessment. The team's rhythm is visible on a dashboard. Jeff's state — keystrokes, posture, sentiment — is the andon light.

V1 is when the scoreboard goes green. Not everything built. Everything that matters for Jeff to trust the system as a thinking partner.

---

## Fitness Functions — How We Know It's Still True

The AC says what should be true. Fitness functions say how we know it's *still* true after V1 ships. These are the regression guards. Card: #1134.

### Graph Fitness (bones stay healthy)

| ID | Metric | Target | Direction | Source |
|----|--------|--------|-----------|--------|
| F1 | Cross-domain link count | Monotonically increasing. Never regresses. | UP | SPARQL count jb:mentions + dcterms:references |
| F2 | Orphaned entity rate | < 1% (tracks without albums, photos without dates) | DOWN | SPARQL orphan queries per domain |
| F3 | Search recall | "resilience" returns stories + blog + notes, not siloed | STABLE | Search benchmark harness (17 tests) |
| F4 | Query response p95 | < 500ms search, < 2s browse | CEILING | Grafana / Loki |
| F5 | Triple count | Never decreases except deliberate dedup. Currently 16M+. | UP | Fuseki stats |

### Process Fitness (method stays honest)

| ID | Metric | Target | Direction | Source |
|----|--------|--------|-----------|--------|
| F6 | Re-prompt rate per role | Trending down. Jeff repeating himself = system failure. | DOWN | Re-prompt analytics / spine |
| F7 | Approval tax | % of Jeff prompts that are "yes"/"go ahead" → toward zero | DOWN | Spine interaction patterns |
| F8 | Card cycle time by size | Small < 1 session, Medium < 2, Large < 3 | CEILING | Board timestamps |
| F9 | WIP age | No card in WIP > 3 sessions without comment | CEILING | board-ts audit |
| F10 | Spine event coverage | Every mutation and pattern emits. No silent work. | STABLE | Chorus event count vs board mutations |

### Jeff Fitness (system serves the person)

| ID | Metric | Target | Direction | Source |
|----|--------|--------|-----------|--------|
| F11 | Interaction cost | Keystrokes + mouse per shipped work → trending down | DOWN | Andon input / cards per session |
| F12 | Break frequency | ≥ 1 break per 90-minute block | STABLE | Andon idle/break (#1097) |
| F13 | Sentiment trajectory | Session-end ≥ session-start. Sessions shouldn't drain. | UP | Prompt sentiment / andon |
| F14 | Cold start duration | < 10 min from session open to first productive output | DOWN | Session tempo (start → first activity) |
| F15 | Autonomy ratio | % of role activity during Jeff gaps → trending up | UP | Session tempo: role activity in Jeff idle |
