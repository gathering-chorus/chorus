# Chorus MCP Registry — Spike brief

**Card:** #2452
**Date:** 2026-04-22
**Owner:** Wren
**Status:** Spike — decision recommendation at bottom.

## What this is

A minimal Model Context Protocol (MCP) server exposing three Chorus capabilities:

| Kind | Name | What it does |
|------|------|--------------|
| Resource | `chorus://principles` | Proxies `GET /api/loom/principles` — returns the 13 declared team principles |
| Tool | `enumerate_skills` | SPARQL-queries `urn:chorus:ontology` for chorus:Skill instances + implementedIn paths |
| Tool | `invoke_skill_pull` | Dry-run preview of `/pull <card-id>` invocation (SPIKE boundary — no state change) |

Transport is stdio. A demo client (`demo-client.mjs`) spawns the server, enumerates resources + tools, reads the principles resource, calls `enumerate_skills`, calls `invoke_skill_pull`. Run with `npm run demo`.

## Why

Jeff's 2026-04-22 framing: **"MCP takes away the arbitrary hacking we do to find things."** Today I spent 4 guessed URLs + a source grep to find `/api/chorus/domain/:name/decisions`. The agent-economic-gradient defaults every role to rebuild-over-discover because discovery is expensive in the current harness. MCP makes discovery free — one `listResources` / `listTools` call and the capability catalog is in hand, with typed schemas.

Relevant today:
- Authorization graph just merged (#2348) — 32 skills, 7 utility skills, 14 policies, 17 gates, all with implementedIn paths. The graph is a natural source for an MCP registry generator.
- Roles service design (2026-04-17, Gap 9) asked for `chorus:dependsOn` edges. MCP makes those dependencies navigable, not just documented.
- Integrations section of every domain is thin. MCP registry would populate it declaratively.

## Demo output

Running `npm run demo` produces:

```
1. listResources         → 1 resource: chorus://principles
2. readResource          → 13 principles returned from live graph
3. listTools             → 2 tools: enumerate_skills, invoke_skill_pull
4. callTool enumerate_skills → 32 skills returned with implementedIn paths
5. callTool invoke_skill_pull → dry-run contract validated
```

**The anti-hacking JX is real.** The demo client never hardcodes a URL, never greps a source file, never guesses a skill name. Every capability is discovered from the registry at call time.

## Graph-to-MCP generation — IMPLEMENTED in spike (2026-04-22 second pass)

**Update:** `generator.mjs` now wired into `server.mjs` at startup. 45 tools generated from the graph (32 skills + 13 gates, post-filter), plus the 2 static tools (enumerate_skills + invoke_skill_pull). Tool names namespaced per Silas's amendment: `chorus.skill.<id>` and `chorus.gate.<id>`.

Fail-loud discipline applied: if any `chorus:Skill` or `chorus:Gate` lacks `chorus:implementedIn`, generator throws with the offending IDs enumerated. **The first run surfaced real graph drift:** `gate-BuildGate`, `gate-DesignGate`, `gate-DirectionGate`, `gate-ProvingGate` are declared `a chorus:Gate` but are really category superclasses (not concrete gates with implementations). Current generator filter excludes them via `FILTER NOT EXISTS { ?instance rdfs:subClassOf ?entity }`, but the right graph-side fix is to either (a) change their type to `rdfs:subClassOf chorus:Gate` instead of instance-of, or (b) mark them `chorus:abstract true`. Follow-on for #2348 or the generator card.

### Original sketch (retained for reference)

```sparql
PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?skill ?label ?comment ?impl
WHERE {
  GRAPH <urn:chorus:ontology> {
    ?skill a chorus:Skill .
    ?skill rdfs:label ?label .
    ?skill rdfs:comment ?comment .
    OPTIONAL { ?skill chorus:implementedIn ?impl }
  }
}
```

Each row → one MCP tool declaration. `label` becomes the tool name (slugified), `comment` becomes the description, `implementedIn` becomes the invocation target. Rebuild the registry whenever the graph reloads (same `/api/athena/reload` hook Silas shipped today).

Same for resources: every `chorus:SubDomain` with a populated API endpoint becomes an MCP resource with `chorus://<subdomain>/<section>` URI.

**Implication:** adding a skill = declaring in graph = appearing in MCP on next reload. No separate registry to maintain. No drift possible.

## Authorization sketch

The spike has no auth. Real adoption needs a mapping from role-scoping to MCP trust.

Current Chorus role-scoping:
- `/gate-product` — Wren only
- `/gate-arch`, `/gate-ops` — Silas only
- `/gate-code`, `/gate-quality` — Kade only

MCP doesn't have first-class per-role auth. Two shapes to explore:
1. **Per-role MCP server** — each role connects to `chorus-mcp-server-wren` (or `-silas` / `-kade`), the server filters tools by owner. Cleanest, but 3× server processes.
2. **Single MCP server + caller identity** — the client sends a role identity header; server filters on each `listTools` / `callTool`. Lighter, but requires trusting client-supplied identity.

Neither is blocking for the spike. Both are viable; need to be decided before production adoption.

## Integrations-section implication

Every domain in Chorus has an `integrations` section (see `athena-subdomain-completeness.ts`). Today it's thin / ad-hoc.

If MCP becomes the registry, domain integrations become **queryable and honest**:

```
GET /api/chorus/domain/<name>
  → integrations: [
      { name: "enumerate_skills", exposesVia: "mcp", source: "chorus:skill-pull →" },
      { name: "loom-principles", exposesVia: "mcp", consumedBy: ["chorus-prompt", "session-start"] },
      ...
    ]
```

The honest-fold discipline Jeff named today lands naturally: absent integrations render "0 MCP tools exposed" rather than hiding.

## What the spike doesn't prove

- **Production-readiness.** stdio transport is single-process; for multi-client production we'd want HTTP+SSE transport per the MCP spec.
- **Performance.** SPARQL query per enumerate_skills call — fine for 32 skills, untested at higher scales.
- **Auth.** Entirely punted; see sketch above.
- **Invocation safety.** `invoke_skill_pull` is a dry-run. Real invocation needs transactional guarantees equal to `/pull`'s existing gates.
- **Discovery UX from inside a role session.** Demo is a standalone client; integration with Claude Code's built-in MCP support is a follow-on.

## Decision recommendation

**ADOPT, incrementally.** Three signals:

1. **The anti-hacking JX lands cleanly.** The demo proves discovery works without URL-knowledge. That's the core premise, validated.
2. **Graph-to-MCP generation is natural.** The authorization graph (#2348) is already the right shape; a generator is a small additional piece, not a new architecture.
3. **Integrations section becomes live.** Today's thin integrations model becomes queryable infrastructure if MCP is the registry.

**Proposed follow-on cards** (do not file until Jeff approves direction):

- **MCP generator:** SPARQL → MCP tool/resource declarations, auto-rebuild on graph reload.
- **MCP auth mapping:** pick a trust model, implement role-scoping.
- **HTTP+SSE transport:** replace stdio for multi-client production use.
- **Invocation safety:** `invoke_skill_pull` becomes real, delegates to chorus-hook-shim with full gate chain.
- **Domain integrations via MCP:** domain API returns integrations populated from registry.

**Scope discipline:** MCP extends the existing surface; it does not replace `/api/loom/*`, `/api/chorus/*`, `/api/athena/*`. HTTP endpoints keep serving browser/human consumers. MCP is the agent-native surface over the same data.

**If we stop here:** the spike directory stays as the "we tried this, here's the code and why it works" record. Zero production footprint.

## Files

- `server.mjs` — the spike MCP server
- `demo-client.mjs` — end-to-end demo (`npm run demo`)
- `package.json` — dependencies (@modelcontextprotocol/sdk@^1.29.0)
- `README.md` — this brief

## How to run

```bash
cd platform/spikes/mcp-registry
npm install        # one-time
npm run demo       # runs server + client, prints discovery trace
```

Requires `chorus-api` on `localhost:3340` (for principles resource) and Fuseki on `localhost:3030` (for skills enumeration).
