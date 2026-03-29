# Spike: The Borg — Codebase Decomposition for Chorus

**Author**: Silas (Architect)
**Date**: 2026-02-21
**Card**: #125
**Time-box**: One session

## The Idea

A product that uses Chorus's multi-agent team to systematically decompose an existing application for refactoring or rewrite. Jeff's original framing: "like the Borg" — it absorbs and assimilates the target system into a structured model that the team can reason about and act on.

## Prior Art

### Jeff's Patent (US9552400B2)
RDF/OWL + SPARQL + workflow gates at Staples. Enterprise-scale pattern: ontology models the system, agents query the model, workflow gates enforce quality. The Borg is this pattern applied to codebase modernization.

### OMG ADM / KDM (ISO/IEC 19506)
The [Knowledge Discovery Metamodel](https://en.wikipedia.org/wiki/Knowledge_Discovery_Metamodel) defines a standard for representing existing software assets:
1. **Reverse engineer** existing system into KDM (language-agnostic ontology)
2. KDM absorbs everything: code, data, UI, platform dependencies, relationships
3. Model becomes a queryable representation more useful than the source
4. **Forward engineer** the replacement from the model

[ISO/IEC 19506 spec](https://www.iso.org/standard/32625.html) | [KDM v1.4 (OMG)](https://www.omg.org/spec/KDM/1.4/About-KDM) | [ScienceDirect overview](https://www.sciencedirect.com/science/article/abs/pii/S0920548911000183)

### Current Landscape (2025-2026)
- **Tree-sitter** — fast, incremental parsing for 100+ languages. [MCP servers](https://www.pulsemcp.com/servers/wrale-tree-sitter) already exist for AI-assisted code analysis
- **ast-grep** — Rust-based AST search/transform, handles large codebases efficiently
- **Claude Code** — already does [architecture extraction on 50k+ line codebases](https://www.oreilly.com/radar/reverse-engineering-your-software-architecture-with-claude-code-to-help-claude-code/) via agentic search and subagents
- **CodeRAG** — [dependency graph extraction via Tree-sitter](https://medium.com/@shsax/how-i-built-coderag-with-dependency-graph-using-tree-sitter-0a71867059ae) for retrieval-augmented generation

## What Already Exists in Chorus

| Capability | Status | Relevance to Borg |
|---|---|---|
| SQLite FTS5 index | Live (17k+ messages) | Query engine for extracted knowledge |
| References table | Live (2,289 refs) | Cross-referencing between entities |
| /werk workflows | Live | Sequenced decomposition/refactoring tasks |
| /clearing sessions | Live | Team alignment on architectural decisions |
| Multi-agent roles | Live | Wren (scope), Silas (architecture), Kade (implementation) |
| Chorus HTTP API | In progress (WF-006) | Programmatic access to the index |
| Capturing intake | Approved (WF-005) | Decision/action extraction pipeline |

## What The Borg Needs (New)

### 1. Codebase Indexer
Parse target codebase into structural units. Tree-sitter is the right tool — language-agnostic, incremental, well-supported.

**Output per file:**
```json
{
  "path": "src/services/search-index.service.ts",
  "language": "typescript",
  "symbols": [
    { "name": "SearchIndexService", "type": "class", "line": 15, "exports": true },
    { "name": "indexPhotoBatch", "type": "method", "line": 42, "parent": "SearchIndexService" },
    { "name": "rebuildCollection", "type": "method", "line": 78, "parent": "SearchIndexService" }
  ],
  "imports": [
    { "from": "better-sqlite3", "names": ["Database"] },
    { "from": "../config", "names": ["DB_PATH"] }
  ],
  "exports": ["SearchIndexService"]
}
```

**Storage**: New `codebase_symbols` table in Chorus index, cross-referenced via `refs` table.

### 2. Dependency Graph
Map relationships between symbols, files, and modules.

**Relationship types:**
- `imports` — file A imports from file B
- `calls` — function A calls function B
- `inherits` — class A extends class B
- `reads_from` / `writes_to` — data flow (DB tables, files, APIs)
- `exposes` — module A exports symbol B
- `tests` — test file A covers module B

**Storage**: Extend `refs` table with `entity_type='symbol'` and new relationship types. Or a dedicated `dependencies` table if the volume warrants it.

### 3. Decomposition Engine
AI-driven boundary detection. Given the dependency graph, identify:
- **Natural seams** — clusters of files with high internal cohesion, low external coupling
- **Entanglement points** — symbols referenced across many boundaries (refactoring targets)
- **Data flow boundaries** — where data transforms or persists (API boundaries, DB access)
- **Test coverage gaps** — modules with no test relationships

This is a Claude task — feed the dependency graph + file contents to Claude, ask it to identify boundaries and recommend decomposition.

### 4. Refactoring Plan Generator
Takes decomposition output + target architecture, produces:
- Sequenced /werk workflows (extract module A, then decouple B from A, then rewrite C)
- Risk assessment per step (blast radius, test coverage, dependency count)
- Verification criteria (what proves each step succeeded)

## Architecture

```
Target Codebase (git repo)
    ↓ tree-sitter parse
Codebase Index (symbols, imports, exports per file)
    ↓ dependency extraction
Dependency Graph (calls, imports, inherits, data flow)
    ↓ stored in Chorus index
Chorus SQLite (codebase_symbols + refs + messages)
    ↓ queried by
Decomposition Engine (Claude analysis of graph + code)
    ↓ produces
Refactoring Plan (/werk workflows with sequenced steps)
    ↓ executed by
Chorus Team (Wren scopes, Silas architects, Kade implements)
```

## Build vs Buy Assessment

| Component | Build | Buy/Use |
|---|---|---|
| Tree-sitter parsing | Use existing — tree-sitter CLI or MCP server | **Use** |
| Symbol extraction | Light wrapper around tree-sitter queries | **Build** (small) |
| Dependency graph | Build from import/call analysis | **Build** |
| Chorus index storage | Extend existing schema | **Build** (small) |
| Decomposition analysis | Claude API with structured prompts | **Build** (prompts) |
| Refactoring plans | /werk workflow generation | **Build** (small) |
| Visualization | Extend /werk dashboard with dependency view | **Build** |

**Total new code estimate**: ~500-800 lines. The heaviest lift is dependency extraction from Tree-sitter output. Everything else builds on existing Chorus infrastructure.

## Phasing

### Phase 1: Prove it on ourselves
Point The Borg at `jeff-bridwell-personal-site`. We know this codebase intimately — we can validate the decomposition against our own architectural knowledge. If the Borg produces a decomposition that matches what Silas already knows, it works.

### Phase 2: Produce a refactoring plan
Use the decomposition to identify the top 3 refactoring targets in our own codebase. Generate /werk workflows. Execute one.

### Phase 3: Package as a product
Generalize from "our codebase" to "any codebase." The Borg becomes a Chorus capability that any team using Chorus can invoke on a target repo.

## Revenue Angle

"Point your AI team at a legacy codebase. The Borg maps it, the team refactors it systematically."

- Consulting play: Borg analysis as a deliverable (codebase audit + refactoring roadmap)
- Product play: Self-service tool for teams using Claude Code
- IP play: Jeff's patent (US9552400B2) is prior art proving the pattern at enterprise scale

## Open Questions

1. **Tree-sitter vs Claude native?** Claude Code already does architecture extraction natively. Do we need Tree-sitter, or can we use Claude's built-in codebase understanding and just structure its output?
2. **Graph storage**: Extend `refs` table or dedicated `dependencies` table? Depends on volume — a 50k-line codebase might produce 10k+ dependency edges.
3. **Scope for Phase 1**: Full codebase or start with one domain (e.g., just the harvest pipeline)?
4. **Naming**: "The Borg" is memorable but has Star Trek trademark associations. Keep for internal use, rename for external?

## Recommendation

**Build Phase 1 now.** Point it at our own codebase. The index infrastructure exists, Tree-sitter is available, Claude handles the analysis. The novel part is structuring the output into the Chorus index and generating /werk workflows from it. That's a 2-3 session build.

This is the most commercially interesting thing in the portfolio. It bridges Jeff's enterprise architecture experience (the patent, KDM, ADM) with Chorus's multi-agent coordination. It's not just a tool — it's a demonstration that the Chorus team model works on real engineering problems.
