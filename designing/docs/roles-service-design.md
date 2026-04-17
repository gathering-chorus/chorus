# Roles — Service Design

**Wren, 2026-04-17. Draft. Source: CLAUDE.md fragments, permissions profiles, state declaration files, observer inference (#2120), board ownership, skill scoping, memory system.**

## Promise

Every role is a coherent, accountable agent: known identity, current charter, bounded permissions, legible state, durable memory. When Roles is healthy, Jeff gets predictable partners whose capabilities, tone, and responsibilities are reliably specified and visible. When Roles drifts, agents lose coherence in isolation and Jeff becomes the charter-keeper — correcting tone, assigning ownership, auditing memory, re-explaining context every session.

## Overview

Roles is the identity layer of Chorus. It manages who each agent is (identity), what they know (charter), what they can do (permissions), how they hold state (observation + declaration), and what persists across sessions (memory, next-session handoff). It composes ten sub-components into three role-specific configurations (Wren, Silas, Kade) at build time and at runtime.

**Roles also depends on four external substrates that give each agent a coherent stance:** Principles (durable commitments), Practices (how principles enact as behavior), Policies (binary rules derived from principles), and Decisions (point-in-time choices that cite principles/practices/policies as rationale). These live in their own sub-domains of the chorus ontology (loom-principles, loom-practices, loom-policies, loom-decisions) — they are imported, not owned, by Roles.

Currently three roles — Wren (coordination), Silas (observation/ops), Kade (presentation/code). Each role's runtime configuration comes from a combination of build-time fragments + runtime state + external dependency references. The system is load-bearing across every Chorus sub-product: Pulse reads role state, Clearing renders role tiles, Observer infers role behavior, Board routes card ownership, Gates filter by role, Skills scope invocation.

| Component | Status | Source | Gap |
|-----------|--------|--------|-----|
| Identity | REAL | title + principles + tone fragments → composed CLAUDE.md | None |
| Charter | PARTIAL | `designing/claudemd/` shared + role fragments → manifest.json → `roles/{role}/CLAUDE.md` | Wren missing working-with-jeff.md; portfolio/tone duplication; no drift detection |
| Permissions | REAL | `config/profiles/{role}.json` + base.json | None known |
| State declaration | REAL | `/tmp/claude-team-scan/{role}-declared.json` via role-state script | Manual; decays without explicit transitions |
| State inference | REAL | observer.rs reads tool calls → writes declared.json when divergence detected (#2120, shipped today) | Precedence rules still tuning (#2140 60s/120s override) |
| Memory | REAL | `~/.claude/projects/.../memory/` per role | No cross-role visibility; no governance on accretion |
| Working directory | REAL | `roles/{role}/` — briefs/, next-session.md, state files, activity | None |
| Skills | REAL | `platform/skills/` + skill-level owner checks | Some drift in skill/role coupling |
| Session continuity | REAL | next-session.md written at close, consumed at open | Consumption is manual — no verification it's read |
| Accountability | PARTIAL | spine events tag role; Board assigns owner; gates filter by role | Owner defaults to filer when unspecified → wrong-owner bugs (observed today) |

## Sub-Domain Interaction Model

Roles provides the identity surface; other sub-products read or write against it.

| Trigger | Produces | Consumed By | Surface |
|---------|----------|-------------|---------|
| Role declares state | declared.json update | Pulse, Clearing, watchdog | `/tmp/claude-team-scan/{role}-declared.json` |
| Role runs a tool | observer writes inferred card | Pulse (divergence), Clearing (tile) | declared.json (merged) |
| Card moves to WIP with owner | Board ownership signal | Observer (precedence), Pulse (wip_cards) | Vikunja + board-wip-snapshot.json |
| Session closes | next-session.md written | Next session's boot synthesis | `roles/{role}/next-session.md` |
| Session starts | session-start-{role}.md generated | Role's first response | `/tmp/session-start-{role}.md` |
| Role saves memory | Memory file updated | Role's future sessions only | `~/.claude/projects/.../memory/` |
| Spine event emitted | Event tagged with role | Loki, Chorus search, nightly quality | `chorus.log` |
| Card gated | Gate filtered by role | Owner role runs gate | Skill invocation (gate-product=Wren, gate-code=Kade, gate-arch=Silas) |
| Skill invoked | Skill checks owner role | Skill runs or refuses | Skill tool invocation |

The core pattern: Roles doesn't dictate behavior; it supplies identity and state that other sub-products act on. Consistent with Pulse's "assembler not generator" shape — Roles is the **source** of identity, not the enforcer of it.

## Dependencies (the four substrates)

Per Jeff's 4-layer decomposition (2026-04-17): roles depend on four external substrates for the stance they operate from. Each lives in its own sub-domain.

| Dependency | Sub-domain | Status | Instances | Notes |
|-----------|------------|--------|-----------|-------|
| Principles (durable commitments) | loom-principles | POPULATED | 10 | 3 added 2026-04-17: focus-is-infrastructure, quality-at-source, speed-and-quality-correlate |
| Practices (how principles enact) | loom-practices | POPULATED | 7 | actor-bdd, domain-first, service-next, product-owner-aligned, tdd, production-ready, api-first |
| Policies (binary rules) | loom-policies | MISSING | — | Sub-domain does not exist; blocked on #2151 stand-up |
| Decisions (point-in-time choices) | loom-decisions | SHELL | 0 | Sub-domain exists but empty; blocked on #2152 harvest of ~200 DEC-NNN + ~25 ADR-NNN |

**Why dependencies matter to Roles:** a role's identity is coherent because it stands on stable principles, enacts them via practices, bounds itself with policies, and cites decisions as the audit trail for why it acts the way it does. When any layer drifts (principle redefined but practices unchanged; decision made but not broadcast), roles operate inconsistently and Jeff becomes the layer-reconciler.

**Charter vs team principles.** Each role has role-specific operating principles in its CLAUDE.md charter fragment (wren/principles.md, etc.) — those are role-scoped emphasis. The team-wide 10 principles in loom-principles are the shared doctrine. Both exist; they don't compete. Charter fragments should cite team principles where applicable rather than paraphrase them.

## Components

### Identity
Each role has three constitutive fragments: title (role name + emoji + one-sentence purpose), principles (core operating principles), tone (communication style). Composed into every role's CLAUDE.md at build time.

### Charter (CLAUDE.md composition)
Source: `designing/claudemd/shared/*.md` (team-wide disciplines) + `designing/claudemd/roles/{role}/*.md` (role-specific fragments). Composed via `manifest.json` at version-bump time into `roles/{role}/CLAUDE.md`. Currently at manifest version 164, auto-bumps on fragment change.

**Canonical + role overlay pattern** (per Silas, 2026-04-17): when a discipline has a shared base with role-specific variations (e.g., tone), compose shared-fragment + role-hook fragment in listed order. Declarative, no templating. This pattern should be named in the manifest design when #2150 lands.

### Permissions
Source: `config/profiles/{role}.json` + `config/profiles/base.json`. Referenced by the hook system at PreToolUse time to block or allow tool invocations per role. Env var names only, never values (data safety).

### State declaration
Each role declares its state via the `role-state` script, which writes `/tmp/claude-team-scan/{role}-declared.json`. States: building, blocked, waiting, observing, idle. Updated at transitions (pull card, accept, pair start/end, idle).

### State inference (shipped 2026-04-17, #2120)
`observer.rs` in chorus-hooks parses each tool call for card evidence (commit messages, cards CLI calls, chat topics, brief paths). Precedence: (1) board WIP ownership = authoritative, (2) tool-call regex patterns, (3) fall through to declared. Writes inferred card into declared.json alongside the declared card; when they diverge, the divergence is surfaced visibly on Pulse + Clearing.

### Memory
Each role has a persistent memory directory at `~/.claude/projects/{project}/memory/` with a MEMORY.md index and typed memory files (feedback, project, reference, user). Accessed within role only — no cross-role visibility today. Accretion pattern can become performative without structural check.

### Working directory
`roles/{role}/` contains: CLAUDE.md (composed), briefs/ (inbox), next-session.md (self-handoff), state files (per role's charter), archives. This is the role's filesystem presence.

### Skills
Per-role skills exist at `platform/skills/`. Each skill declares an owner in its SKILL.md header (e.g., `gate-product` is Wren-only). Skill runtime checks role identity before executing.

### Session continuity
`next-session.md` is written at session close, synthesized at session open into `/tmp/session-start-{role}.md` by `context_cache.rs`. Self-handoff mechanism.

### Accountability
Spine events (`chorus.log`) always tag the emitting role. Board cards have a single owner per role. Gates filter invocations by role. Together these form the audit trail: who did what, who owns what, who's accountable.

## Surfaces

Roles exposes no API of its own. It contributes to other APIs and surfaces:

- `/tmp/pulse-latest.json` → `roles.{role}.*` (state, inferred/declared card, age, staleness)
- `/api/chorus/pulse/latest` → same, served via HTTP
- The Clearing tiles (`directing/clearing/public/index.html`) — per-role tile with state, card, divergence indicator
- Chorus search → events filtered by role
- Gates → each gate enforces role ownership

## Consumers

| Consumer | Uses | Status |
|----------|------|--------|
| Pulse | roles.{role} state for snapshot | WIRED |
| The Clearing | role tiles + state + divergence | WIRED |
| Observer | declared.json write; board ownership read | WIRED (shipped today) |
| Board | card owner filter | WIRED |
| Gates | role owner check | WIRED |
| Session boot | CLAUDE.md + session-start + pulse read | WIRED |
| Skills | owner check | WIRED |
| Watchdog | detect role staleness | NOT WIRED — follow-on |
| Cross-role memory search | shared memory index | NOT WIRED — follow-on |

## Gaps

1. **Charter drift detection** — no linter exists; asymmetry (Wren missing working-with-jeff.md) and duplication (portfolio 90%, tone 70%) only found via manual audit.
2. **Charter coherence** — canonical + role overlay pattern not yet expressed in manifest; shared-with-tint cases are duplicated instead of extracted.
3. **Manual state decay** — declared state can go stale if role doesn't transition explicitly. #2120 observer now backstops this; 60s/120s override still tuning (#2140).
4. **Memory governance** — each role accretes memory independently; no cross-role visibility, no schema enforcement, no audit trail of what changed. Memories can become performative (observed today) without structural check.
5. **Session continuity trust** — next-session.md is written and consumed, but no verification the next session actually reads it. Close-out is MANDATORY in charter; consumption is not.
6. **Accountability at card-create** — owner defaults to filer in cards CLI. Wrong-owner bugs happen (observed repeatedly today); no intent check at card creation.
7. **Roles sub-product not in Athena** — chorus domain in Athena is 50% complete; services section empty. Roles should be a first-class entry alongside Pulse, Observer, Scrubber, Context Cache, Stop-the-line.
8. **Dependency chain half-populated** — roles depend on principles + practices + policies + decisions. Only principles + practices are populated. `loom-policies` is missing entirely (#2151). `loom-decisions` is an empty shell — ~225 DEC-NNN and ADR-NNN entries haven't been harvested (#2152). Roles can't formally cite its full substrate until these are filled.
9. **No `dependsOn` edge in the graph** — the four-layer dependency is described in this doc but not yet expressed as a formal ontology relationship. Should be a `chorus:dependsOn` edge from the Roles sub-product to each of the four sub-domains so the dependency is queryable, not just documented.

## Next Steps

| # | Action | Impact | Owner | Status |
|---|--------|--------|-------|--------|
| 1 | Populate Athena chorus-domain services section with Roles + siblings | Taxonomy lives in the graph, not in heads | Wren | In-progress (3 principles landed 2026-04-17) |
| 2 | #2150 fragment streamlining, informed by this design (asymmetry + overlay + linter) | Closes Wren asymmetry, extracts duplication, adds drift detection | Wren + Silas contributes | Later (parked pending this design) |
| 3 | #2151 stand up loom-policies sub-domain | Completes the 4-layer dependency substrate | Wren | Next |
| 4 | #2152 harvest DEC-NNN + ADR-NNN into loom-decisions | Fills the empty decisions shell; makes ~225 decisions queryable | Wren | Next |
| 5 | Express Roles → {Principles, Practices, Policies, Decisions} as formal `chorus:dependsOn` edge in the ontology | Dependency chain becomes graph-queryable, not just doc-described | Wren | Unfiled |
| 6 | Memory governance — cross-role visibility, schema, audit trail | Removes performativity incentive; makes memory a team asset | Unfiled | — |
| 7 | Owner-intent check at card creation — intent-vs-default-to-filer | Removes wrong-owner bugs at source | Unfiled | — |
| 8 | Session continuity verification — mark consumed, spine event on first read | Closes trust gap on self-handoff | Unfiled | — |
| 9 | Roles product page at 3340/chorus (post #2116 migration) | Public surface for the sub-product; parallel to eventual Pulse page | Unfiled | — |

## Not in scope for this design

- Behavioral-decay in chartered disciplines (role not practicing what CLAUDE.md says). That's a separate problem — Roles defines the charter, doesn't enforce practice. Enforcement mechanisms (gates, fitness functions, #2145 stop-the-line hook) are other sub-products.
- Adding new roles. This design assumes three; extending to N roles is a capacity conversation, not a product-shape one.
- Replacing Vikunja with a Chorus-native board. Board is a consumer of Roles owner info; different product, different card.
