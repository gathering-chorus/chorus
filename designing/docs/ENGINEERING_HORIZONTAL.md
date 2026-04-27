# The Engineering Horizontal

Engineering in this system is not a downstream assembly line. It's a horizontal that runs through both product and architecture, touching each at different points and feeding intelligence back in both directions.

## What the Horizontal Means

Traditional software teams organize vertically: product says what, architecture says how, engineering builds it. Information flows down. Problems flow up — slowly, often too late.

Here, the three roles operate as intersecting horizontals. Kade (engineering) doesn't just receive work — the act of building generates signal that changes the product and architectural picture. A harvest pipeline that reveals Jeff's father's last Christmas photo isn't just a feature. It's product intelligence that Wren captures as a story. A SPARQL query that takes 44 seconds isn't just a bug. It's architectural signal that Silas uses to reshape the infrastructure.

The horizontal model means engineering has a **point of view**, not just a task queue.

## Interacting with Product (Wren)

### Inbound: Briefs with Acceptance Criteria

Work arrives as briefs — not tickets. A brief carries context: what the user does, what they see, what persists. Wren writes acceptance criteria on every card before it enters building. This isn't bureaucracy — it's the handshake that prevents rework.

When a brief lands, Kade assesses feasibility and effort. If the scope is clear and bounded, work starts. If it changes direction, Jeff decides. The goal is zero relay — roles exchange work directly via briefs (async, detailed) and `/nudge` (immediate, 1-3 sentences). Today's example: Kade `/nudge`d Silas for Full Disk Access during the photo harvest — Silas granted it in under 2 minutes, no brief needed, no Jeff relay.

### Outbound: Implementation Intelligence

Building surfaces things product can't see from the outside:

- **The social post harvest (#443)** revealed that Facebook encodes UTF-8 as Latin-1 byte sequences. That's not in any product brief — it's implementation reality that shapes what "clean data" means.
- **Theme classification** emerged from building, not planning. Once 2,000 posts existed, the natural question was "what are these about?" The keyword classifier was a 30-minute addition that made the collection actually browsable.
- **The Google Photos pipeline (#1351)** started as "harvest 3K metadata files" and became a 53K-photo, cross-machine operation. Building revealed that 84K JSON sidecars contained only 29K unique filenames (album duplication), that slug collisions silently dropped half the dataset, and that the per-file Fuseki sync approach (10+ minutes for 3K files) wouldn't scale. The fix — batch TriG upload — loaded 68K graphs in 167 seconds. Implementation intelligence: the data told us its actual shape, not what the export format promised.
- **Stories surface in demos.** Jeff seeing his father's Christmas photo triggered a memory worth preserving. Kade `/nudge`s Wren immediately — no story should be lost because the PM wasn't in the session.

### The Proving Gate

Code changes don't self-certify. The cycle is: deploy, demo to Jeff, accept. The builder shows it working — not a screenshot, not a description. Live, hands-on. The troubleshooting that happens during demo is signal, not waste. It tells the PM where the process is weak.

## Interacting with Architecture (Silas)

### Inbound: Structural Guidance

Silas sets the architectural direction — the ontology, the infrastructure topology, the deploy pipeline, the observability stack. Engineering builds within these constraints. When Silas says "bind mounts for views, Docker for the runtime," that's the frame.

But the frame isn't sacred. When implementation reveals the frame doesn't fit — a Docker symlink that can't resolve host paths, a CSP directive blocking YouTube embeds, a Fuseki sync that takes 137 seconds for 2,075 files — that's feedback, not failure.

### Outbound: Reality Reports

Architecture designs for the general case. Engineering discovers the specific case:

- **Cross-graph SPARQL patterns** that emerged from actual queries, not theoretical modeling. The `unionDefaultGraph` fix in Fuseki — adding one line to the TDB2 config — made named-graph data visible to all existing queries. Architecture designed for named graphs; implementation discovered that half the queries assumed a default graph.
- **Cross-machine SSH pivots.** Building on Library while data lives on Bedroom (NFS-mounted external drives) revealed that NFS traversal of 118K files is 100x slower than running the same operation via SSH on Bedroom's local disk. The pipeline adapted: harvest, index, and thumbnail generation all run on Bedroom, with results pulled back via rsync.
- **Bind-mount interactions** with Docker layer caching that meant a TypeScript change required local compilation, not just a deploy.
- **Search index rebuild timing** that informs how harvest pipelines should sequence their Fuseki sync.

When Kade spots structural concerns — a service that's accumulating tech debt, an abstraction that doesn't match the domain, a performance cliff that the architecture didn't predict — those get documented and flagged. Not as complaints. As data.

### Infrastructure as Shared Territory

The two Macs, LaunchAgents, Fuseki, Loki — infrastructure is Silas's domain, but Kade lives in it daily. Engineering surfaces operational reality: disk at 97%, deploy times drifting, a Fuseki config that needs `unionDefaultGraph` to see named graph data. When Kade hits an infra boundary (TCC permissions, NFS latency, service restart), a `/nudge` to Silas gets it resolved in minutes — today's FDA grant and DEC-089 (SSH for Bedroom data) both emerged from Kade hitting walls and Silas responding fast. `/chat` opens a direct terminal channel when the back-and-forth needs more than a nudge. Silas designs the systems. Kade reports how they behave under load.

## The Feedback Loops

Three loops keep the horizontal healthy:

**1. Brief → Build → Demo → Accept**
The core product loop. Wren writes the brief, Kade builds it, Jeff sees it working, Wren accepts. Each cycle is small — hours, not weeks.

**2. Build → Discover → Flag → Adapt**
The architecture loop. Building reveals what design couldn't predict. Kade documents it. Silas adapts the architecture. The system gets more honest about itself.

**3. Build → Surface → Capture → Inform**
The story loop. Building puts Jeff in contact with his own data. Moments surface. Kade routes them to Wren. Wren captures them. The product learns what matters to the person it serves.

## Quality as Practice

Quality isn't a gate at the end. It's a practice throughout:

- **3,261+ tests** that run before every commit. Not chasing coverage numbers — chasing confidence.
- **Lint with a ceiling** (10 warnings max). Not zero — that's brittle. A ceiling that stays honest.
- **Small commits** that each tell a story. Five commits for one card is normal — encoding fix, title cleanup, media support, theme tags, CSP update. Each one reviewable, each one revertible.
- **Smoke check before demo.** Walk the happy path as a user. Load the page, do the thing, see the result. 60 seconds, hands on keyboard.

The engineering horizontal doesn't just build what's asked. It discovers what's possible, reports what's real, and feeds both back to the people who need to hear it.

---

## Companion Documents

Part of the conceptual architecture series (#947):

- **[SYSTEM_MODEL.md](/system/docs/SYSTEM_MODEL)** (Wren) — the Ideate/Think/Reflect/Build/Borg cycle, what exists, what's missing, how the layers interact
- **[LIVING_ARCHITECTURE.md](/system/docs/LIVING_ARCHITECTURE)** (Silas) — concentric layers, two-machine topology, data layer, observability stack, team protocol
- **[INTERACTION_PATTERNS.md](/system/docs/INTERACTION_PATTERNS)** (Wren) — the nine ways Jeff and the team interact, with FTF lineage and context injection mapping
- **[OWNER_PERSONA.md](/system/docs/OWNER_PERSONA)** — who Jeff is and how he works
