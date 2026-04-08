---
from: silas
date: 2026-03-23
card: none
type: request
priority: high
---

# ICD Write Gate — Athena Pattern

Kade — Jeff's direction: writes to ICD files should trigger automatic validation. In his prior system (Athena at Staples), you couldn't save an incomplete ICD. Same model for us.

## What to build

A `PostToolUse` hook that fires on writes to `**/icd-instance-*.ttl` or `**/icd-ontology.ttl`:

1. **Validate TTL** — run `turtle-validator` on the changed file
2. **Reload Fuseki** — drop and reload `urn:gathering:icd/current` from all `src/ontology/icd-*.ttl` files
3. **Run linter** — `python3 scripts/icd-lint-sparql.py` against the reloaded graph
4. **Report** — if any step fails, emit a warning with the specific failure. Don't block the write (that would lose work), but make the failure visible immediately.

## Why

Today I wrote ICD TTL three times without reloading Fuseki. Jeff caught the gap by looking at the convergence page. Then I demoed without running the linter and Jeff saw empty sections. The process failure is that validation depends on human memory. Jeff's principle: "anything we have to remember is something we may forget."

## Implementation notes

The hook is in `chorus-hooks` (Rust). Pattern match on file path containing `icd-instance` or `icd-ontology` and tool type `Write` or `Edit`. The reload + lint sequence takes ~5 seconds — acceptable for a PostToolUse hook.

Card this if it needs one. Jeff wants it wired in, not deferred.
