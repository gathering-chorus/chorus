# LSP + AST MCP Bridges — Install + Config

**Filed under:** #3108 — Install LSP + AST MCP bridges (cclsp + ast-grep-mcp) for chorus codebase navigation.

**Date:** 2026-05-28
**Author:** Kade, per Jeff's 2026-05-28 directive ("harness matters as much as model" — bet that ~1 day install pays back via accelerated v2-cutover work).

## What this enables

- **LSP via MCP** (cclsp): symbol-level code intelligence — `find_references`, `goto_definition`, `find_implementations`, hover/type info, diagnostics. Replaces grep-based navigation for 100k+ line chorus codebase.
- **AST via MCP** (ast-grep-mcp): structural code search using tree-sitter ASTs across 20+ languages. Pattern-based search like `let _ = run(curl, ...)` finds the silent-fail class structurally, not regex-heuristically. Direct enabler for practice-registry pre-hoc enforcement.

## Prerequisites

Verify all installed before configuring:

```bash
which node npm                          # node v20.20.2 minimum
which typescript-language-server        # npm install -g typescript-language-server
which rust-analyzer                     # ~/.cargo/bin (existing) OR brew install rust-analyzer
which ast-grep                          # brew install ast-grep
which uv uvx                            # brew install uv
which cclsp                             # npm install -g cclsp
```

If any missing:

```bash
# language servers
npm install -g typescript-language-server

# AST + Python tool runner
brew install ast-grep uv

# MCP bridge for LSP
npm install -g cclsp
```

(ast-grep-mcp is run on-demand via `uvx` — no global install needed.)

## Config files

### 1. `chorus/cclsp.json` (chorus root)

Configures which LSP servers cclsp spawns for which file extensions. Lives at repo root so all roles share the language-server config.

```json
{
  "servers": [
    {
      "extensions": ["ts", "tsx", "js", "jsx", "mjs", "cjs"],
      "command": ["typescript-language-server", "--stdio"],
      "rootDir": "."
    },
    {
      "extensions": ["rs"],
      "command": ["rust-analyzer"],
      "rootDir": "."
    }
  ]
}
```

### 2. `roles/<role>/.mcp.json` — add cclsp + ast-grep entries

Each role's MCP config gets two new entries alongside the existing `chorus-api` server. Example for `roles/kade/.mcp.json`:

```json
{
  "$schema": "https://schemas.modelcontextprotocol.io/mcp.schema.json",
  "mcpServers": {
    "chorus-api": {
      "type": "http",
      "url": "http://localhost:3341/mcp",
      "headers": { "X-Chorus-Role": "kade" }
    },
    "cclsp": {
      "command": "cclsp",
      "args": ["--config", "/Users/jeffbridwell/CascadeProjects/chorus/cclsp.json"]
    },
    "ast-grep": {
      "command": "uvx",
      "args": ["--from", "git+https://github.com/ast-grep/ast-grep-mcp", "ast-grep-server"]
    }
  }
}
```

Other roles (wren, silas) get the same two entries added to their own `.mcp.json` files when ready — separate cards per role.

## Activation

After config files land, restart Claude Code (or the active session). MCP servers are discovered from the role's `.mcp.json` at session start; new entries require reload.

## Test commands

After reload, verify in a Claude Code session:

```
# LSP test (via cclsp MCP)
# Ask Claude Code to find_references for a known symbol like chorus_acp
# Expected: list of call sites across platform/api/, .claude/skills/, etc.

# AST test (via ast-grep MCP)
# Ask Claude Code to find_code with pattern: 'let _ = run("curl", $$$)'
# Expected: hits across werk-demo, chorus-hooks invokers, etc.

# Sanity: existing chorus-api MCP tools still work (chorus_pull_card, chorus_cards_view, etc.)
```

## Why this exists / context

- Cole Medin's Anthropic best-practices ratification (seed 2026-05-27): "LSP via MCP" for codebases >100k lines.
- Luca Mezzalira's "harness matters as much as model" framing (seed 2026-05-27): engineers shift from writing code to designing environments.
- Jeff's 2026-05-28 bet: lead with LSP+AST install before substrate-JX v2 cutover.
- Direct enabler for `/werk/code-review` verb (substrate-JX #3064 follow-on): practice-registry pre-hoc enforcement uses AST patterns + LSP semantic info, not regex heuristics.

## Replication for other roles

When Wren or Silas wants the same setup in their session:

1. Verify prereqs (same `which` checks above).
2. Add identical `cclsp` + `ast-grep` entries to their own `roles/<role>/.mcp.json` (their `chorus-api` entry has `X-Chorus-Role: <role>` — leave that alone).
3. Reload their Claude Code session.

The `chorus/cclsp.json` is shared — no role-specific config needed.

## Known limitations

- ast-grep-mcp is "experimental" per its README. Active project (~413 stars, 57+ commits) but no maintenance commitment stated.
- `uvx` invocation downloads ast-grep-mcp on first use and caches; first run is slow.
- cclsp spawns one LSP process per file-extension cluster — Rust analyzer for `.rs`, TS server for `.ts/.tsx/.js/.jsx`. Both processes are heavy; expect ~200-500MB RSS while active.

## Follow-ons

- Add `cclsp` + `ast-grep` to Wren and Silas `.mcp.json` when they want them (separate cards per role).
- Curated ast-grep YAML rules library tied to practice-registry instances (`chorus:practice-no-silent-fail-curl` → `find_code_by_rule` against a curated rule file). Future card under /werk/code-review lineage.
- Consider exposing cclsp + ast-grep through a chorus-api wrapper if cross-role queries (e.g., "find references to this symbol across all role-werks") become valuable.
