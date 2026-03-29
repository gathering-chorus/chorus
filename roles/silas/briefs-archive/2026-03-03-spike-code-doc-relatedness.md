# Spike: Code-to-Doc Relatedness Graph — #783

**Owner:** Silas (Architect)
**Date:** 2026-03-03
**Time-box:** 1 session

## Problem

Doc drift is invisible until someone audits manually. Today's audit: 5 of 15 docs stale (33%). The doc-drift gate (#763) needs to know which docs relate to which code — that mapping doesn't exist as queryable data.

## Idea

Harvest the codebase and docs into RDF. The knowledge graph already connects music tracks to albums to artists. Same pattern: connect source files to modules to domains to docs.

## What the graph would look like

```turtle
# Source file
<code:src/handlers/login.handler.ts> a jb:SourceFile ;
    jb:inDomain "auth" ;
    jb:imports <code:src/config/authorized-users.ts> ;
    jb:imports <code:src/interfaces/auth.interface.ts> ;
    jb:referencesEnvVar "CSS_ISSUER_URL", "CSS_CC_ID", "CSS_CC_SECRET" ;
    jb:documentedBy <doc:SOLID-AUTHENTICATION.md> .

# Doc file
<doc:SOLID-AUTHENTICATION.md> a jb:DocumentationFile ;
    jb:coversDomain "auth" ;
    jb:mentionsFile <code:src/handlers/login.handler.ts> ;
    jb:mentionsFile <code:src/handlers/callback.handler.ts> ;
    jb:mentionsConfig "docker-compose.yml" ;
    dcterms:modified "2026-03-03"^^xsd:date .

# Infra file
<code:docker-compose.yml> a jb:InfraFile ;
    jb:definesService "css", "fuseki", "navidrome", "app" ;
    jb:documentedBy <doc:INFRASTRUCTURE.md> .
```

## Queries this enables

**1. "What docs are affected by this commit?"**
```sparql
# Given changed files from git diff, find related docs
SELECT DISTINCT ?doc WHERE {
  VALUES ?changed { <code:src/handlers/login.handler.ts> <code:docker-compose.yml> }
  { ?changed jb:documentedBy ?doc }
  UNION
  { ?doc jb:mentionsFile ?changed }
  UNION
  { ?changed jb:inDomain ?domain . ?doc jb:coversDomain ?domain }
}
```

**2. "Which docs are most connected to code that changed recently?"**
Weighted by number of edges — more connections = higher drift risk.

**3. "What's the coupling between domains?"**
Same query I sketched earlier — which domains share code imports.

**4. "Is this doc an orphan?"**
Docs with zero inbound edges from code = either stable reference or abandoned.

## Harvester approach

Two passes:
1. **Code harvester** — walk `src/**/*.ts`, extract: imports (regex on `from '...'`), domain (infer from handler name or directory), env vars (regex on `process.env.`), exports.
2. **Doc harvester** — walk `data/about/*.md`, extract: file references (backtick patterns), domain keywords, last-modified date.
3. **Cross-link** — match code↔doc via: explicit `Key Files` tables in docs, domain overlap, filename mentions.

Output: Turtle files in `data/pods/jeff/codebase/`. Index in Fuseki. Standard pipeline.

## How it feeds #763

The doc-drift gate becomes:
1. `git diff` from session → list of changed files
2. SPARQL query: "what docs relate to these files?"
3. Compare doc `dcterms:modified` against commit timestamp
4. Warn if doc is older than the code change

No more hand-maintained glob patterns in a manifest. The graph IS the manifest.

## Out of scope

- Full AST parsing (regex on imports is sufficient for v1)
- Real-time incremental updates (batch harvest is fine)
- TypeScript type analysis
