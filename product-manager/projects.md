# Project Status

Last updated: 2026-03-22 (session 6 — 31+ cards shipped)

## jeff-bridwell-personal-site — "Gathering"
- **Product name**: Gathering (DEC-010) — from Heidegger's Versammlung. Meaning coheres through relations, not central control.
- **Vision**: Personal Knowledge Graph with Agency — a living model of Jeff's world he can think with. Privacy-first, publish-when-ready.
- **Core model**: Everything starts private in SOLID pods, graduates to shared or public via ACLs. "The workshop is not the storefront."
- **Domains**: Self, Music (31K files), Photos (63K canonical from 83K source), Sexuality (1.86M items in 5 typed graphs), Books (70), Property, Blog (43), Gallery, Profile, Stories (141 TTL), Notes (823), People (2,942 + 48 face clusters + 23 Google Takeout), Garden, WordPress, Documents (43K), Social (2,075), Episodes (28), Capture (253)
- **Stack**: TypeScript, Express, Node.js, SOLID pods (Turtle/RDF), Apache Jena Fuseki (SPARQL), native LaunchAgents (Docker fully retired Mar 14, only WordPress/MySQL/SOLID remain in Docker)
- **Ontology**: v1.1.0 — Domains: Self, Music, Photos, Books, Property, Blog, Gallery, Glimmer, Stories, Home Cloud, Sexuality, People. `jb:Story`, `jb:StoryCollection` added.
- **Auth**: Local CSS as OIDC provider (#685 shipped) — login 31s→91ms. No more Pivot redirect.
- **Health**: GREEN. 3938+ tests passing. Lint `--max-warnings` 10. E2E smoke suite (Playwright).
- **Surfaces**: `/chorus` (SVG system map), `/werk` (value stream + spine), `/loom` (ownership playbook + team metrics), `/flow` (chunk board), `/search`, mind map home, `/about` (refined bio + AI team)
- **Search**: Three-layer — FTS (154K items), Shape matching (theme/value overlap), Semantic (LanceDB + nomic-embed-text, 15K docs embedded via Ollama on Bedroom). Search pipeline WF-162 shipped 2026-03-21.
- **Deploy**: `app-state.sh` for all lifecycle. Bind mounts: `dist/` + `views/`. Native LaunchAgent — deploy 90s→19s. Startup sync manifest-gated.
- **Nav**: Two-hub nav-tree (#1294) — System eliminated, About L2 added. Live-reload from nav-tree.json (#1287). Mind map + navbar unified from single source (#1274).
- **In progress**: Photo serving from Bedroom media server, seed capture pipeline (#1617), Bridge/Clearing v2 polish
- **Recent shipped (3/22 — 31 cards)**: Bridge/Clearing v2 (#1601+#1605-#1613), Photos→People pipeline (#1502 canonical layer, #1504 serving model, #1499 face clustering, #1604 Google Takeout people, #1609 fuzzy match, #1610 ICD mapping), People enrichment (#1270 relationship depth, #1493 cross-domain linker, #1356+#1494 stories→people), Person detail page (#1525), Story extraction (#1500+#1614+#1616 capture API), /jeff page (#1572+#1573), Search (#1548+#1585), Sexuality graphs (#1584), Nudge passive queue (#1600), Cron close-out (#1597), JDI instrumentation (#1598), Ambient observation (#1595).
- **Self AI (shipped)**: Reflect at `/self` — local Mistral Small 22B on Bedroom Mac. Chat UI with conversation memory, story context from knowledge graph.
- **Design system (shipped)**: Shared CSS classes, collapsible sections, mind map aligned with navbar via nav-tree.json.
- **Dependencies**: shared-observability (metrics/logs)

## Chorus — Team Coordination Product
- **Product name**: Chorus (DEC-019, DEC-034) — an authored Werk, not a tool. The protocol IS the product.
- **Vision**: Versioned, legible, auditable team operating protocol. Forkable coordination framework for human+AI teams.
- **Positioning**: "Enterprise architect takes proven patterns and makes them accessible to individuals." Patent lineage: US9552400B2. Validated by Fowler's "Harness Engineering" (Feb 2026) — Chorus applies harness engineering to team coordination, not just code.
- **Core principle**: Observability not surveillance — data serves the person it's about, not someone above them.
- **Stack**: Git repo (gathering-team), CLAUDE.md files (generated from ~25 fragments post-audit #1313), team-architecture.md, Vikunja kanban, TypeScript board client, workflow engine, SessionStart hooks, context service (SQLite FTS5 + LanceDB semantic, 97K messages), The Clearing (multi-party chat), CLAUDE.md generator (fragments → manifest → 3 roles)
- **Health**: GREEN. 100 decisions recorded (DEC-001 through DEC-100). 15+ ADRs. 10+ Grafana dashboards. Card-first gate enforced. WIP limit 3 + Harvesting lane 2 + SWAT lane.
- **Board stats**: ~1595 total cards. Done column growing. Board sweep skill runs every 4h.
- **Key decisions this session**: DEC-101 (Jeff Constitution), DEC-102 (Attention Contract — roles monitor each other), #1594 (chorus-hooks test backfill), #1595 (persistent role observation). Rust hooks service (#1587) shipped — 12 shell scripts → compiled binary. PostToolUse nudge drain working with exit-code-2 stderr.
- **Semantic search**: 97K messages indexed. Chorus API supports mode=fts|semantic|hybrid. Inverted search hierarchy (#1302) — Chorus enrichment injected transparently on context searches.
- **Three surfaces (DEC-043)**: `/werk` = protocol, `/loom` = team, `/chorus/*` = memory. Plus `/flow` = product board.
- **Board tooling**: `cards` CLI with quality gates, blast radius checks (#1266, #1318), chunk+sequence enforcement (#1272). Workflow manifests auto-created.
- **Process model**: Werk value stream — Directing → Designing → Building → Proving. Proving gate (DEC-048): deploy → demo → accept. WSJF tiebreaker (DEC-049). WIP limit 3 (DEC-051).
- **Hardening sequence (shipped)**: 20+ cards since 3/6 — gate audit, hook consolidation (scripts 50→32, fragments 63→~25), Docker CLI guard, lint ratchet, smoke-check gate, fitness scorecard, search inversion, blast radius WIP overlap, doc drift audit (97% health).
- **Revenue path (DEC-067)**: Akasha → Consulting → Light Life → Mobile. Next: sell the method.
- **Convergence Architecture (DEC-096)**: Strategic product thesis connecting patent → Staples → Gathering/Chorus. Four companion docs: convergence-architecture.html (thesis), borg-assimilation-pattern.html (legacy extraction), attention-architecture.html (agent focus), wardley-map-icd-product.html (market positioning). Proof of concept working across 6 domains. The ICD-as-contract-layer is the product's core differentiator.
- **Design constraint**: Chrome + Terminal only. No third-party apps.
- **Interaction model**: One navigator (Jeff), multiple AI drivers (Wren/Silas/Kade). Nine interaction patterns instrumented. Coordination: nudge→chat→brief→clearing escalation ladder. /pair skill for cross-domain co-working. /board-sweep for automated hygiene.
- **Cadences (#1397)**: Condition-driven, not calendar. Pulse (every session), Sweep (threshold-triggered), Reflection (Jeff-initiated). Build:clean ratios tracked per role.
- **Borg**: Three expressions — convergence (card absorption, cross-domain linking), instrumentation (spine events, ratios, patterns), self-awareness (condition triggers, Wren session-start observations). Currently blended with Chorus, may differentiate as Gathering graph matures.

## wordpress-blog
- **Stack**: WordPress, MySQL, Docker
- **Health**: Stable. Jeff's "Songs I Love" blog — public reflection through music.
- **Content**: 41 posts (2019-2024). Arc: uncertainty → personal stories grounded in music → loss and recovery → principles. Water imagery throughout.

## Infrastructure
- **Disk**: 87% on Library. Up from 80% — marathon session data growth.
- **Library** (primary): Mac mini M1, 16GB, 2TB SSD (192.168.86.36) — native LaunchAgents (Docker retired, 3 containers remain: WordPress, MySQL, SOLID). RAM 62% free.
- **Bedroom** (secondary): Mac mini M2 Pro, 32GB, ~178TB external (192.168.86.242) — media/storage, Ollama, thumbnail generation, harvest execution (DEC-089: bulk ops run on Bedroom via SSH, not over NFS)
- **NFS**: /Volumes/Gathering/ on Library, served from Bedroom. Read-only single-file access fine. Bulk operations → SSH to Bedroom.
- **Performance milestones**: Login 31s→91ms, Deploy 90s→19s, Search 42s→207ms, Startup sync 223s→8s, Fuseki batch load 10min→8sec (TriG)
