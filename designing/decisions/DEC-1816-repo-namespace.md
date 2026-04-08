# DEC-1816: Repo Top-Level Structure — Peers, Not Nesting

**Date:** 2026-04-08
**Source:** Jeff, in session with Wren
**Status:** Active

## Decision

The chorus repo root level is organized as **peer directories**, not nested under roles or a single `platform/` tree.

### Root-level peers:

| Directory | Type | Contains |
|-----------|------|----------|
| `designing/` | Value stream stage | Products and domains in design phase |
| `directing/` | Value stream stage | Products and domains in directing phase |
| `building/` | Value stream stage | Products and domains in build phase |
| `proving/` | Value stream stage | Products and domains in proving phase |
| `roles/` | Peer | State, briefs, memory, perspective per role (not code) |
| `skills/` | Peer | Shared tooling — demo, chat, pair, nudge, etc. |
| `interactions/` | Peer | 9 interaction patterns, clearing protocol |
| `platform/` | Infrastructure | Scripts, services, API, hooks |
| `archive/` | Historical | Retired code |

### Rules:

1. **Value stream stages contain products and domains.** Cards lives in `directing/`, not `roles/wren/products/cards/`.
2. **Roles contain perspective, not code.** CLAUDE.md, briefs, memory, state files, next-session.md. The role is a viewpoint, not a code repository.
3. **Skills are shared.** Not per-role copies. One `skills/demo/`, not three.
4. **Interactions are shared.** Protocol definitions that govern how roles work together.
5. **Ownership is declared in PRODUCT_TEMPLATE.md** inside each product directory. Not derived from path nesting.
6. **Migrate one product at a time.** Don't open the whole road. Fix every reference before moving to the next.

## Why

The previous org design put products under `platform/roles/<owner>/products/<product>/`. This nested code under identity, which conflicts with the repo's existing value-stream-at-root structure. Jeff's correction: value streams, roles, skills, and interactions are all peers. Code goes where the work flows. Identity stays with the role.

## Supersedes

Updates the org design in `wren/artifacts/org-design.html` which proposed `platform/roles/<owner>/products/<product>/` as the namespace.
