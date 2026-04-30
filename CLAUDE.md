# Chorus — Protocol Implementation

Chorus is the coordination protocol for the Gathering team. It's the nervous system — hooks, scripts, gates, dashboards, profiles, and the shared memory index that let three AI roles and one human work together.

## Ownership

Chorus has **shared ownership** across all three roles:
- **Wren** owns the interaction layer (coordination tooling, context service, skill)
- **Silas** owns the observation layer (dashboards, alerting, gates, infrastructure)
- **Kade** owns the presentation layer (if/when Chorus gets a UI)

All three roles can commit to this repo. Use your role prefix: `wren:`, `silas:`, `kade:`.

## Relationship to gathering-team

The Werk spans two repos:
- **gathering-team/** — coordination substrate. Where roles live, brief each other, maintain state. Social.
- **chorus/** — protocol implementation. Scripts, profiles, gates, dashboards. Technical.

Role state (CLAUDE.md, briefs/, memory files, decisions) stays in gathering-team. Protocol tooling lives here.

## Directory Structure

```
chorus/
├── scripts/          — Operational scripts (chorus-*.sh)
├── config/           — Configuration
│   └── profiles/     — Permission profiles (base.json, silas.json, kade.json, wren.json)
├── docs/             — Design docs, communication flows
│   └── diagrams/     — Mermaid sources + rendered PNGs
├── dashboards/       — Grafana dashboard JSON (canonical source)
├── skill/            — /chorus skill definition (symlinked to ~/.claude/skills/chorus/)
├── index/            — Database schema, init scripts
└── CLAUDE.md         — This file
```

## Runtime Artifacts

The SQLite index database lives at `~/.chorus/index.db` — it's runtime state, not source code. Scripts in `~/.chorus/scripts/` are symlinks to `chorus/scripts/`.

## Conventions

- **Canonical source**: Dashboards live here and get synced/copied to shared-observability for deployment. Alert rules live in `shared-observability/config/grafana/provisioning/alerting/` — single source at the deploy-source boundary; chorus-api references that path directly (#2620).
- **No secrets**: Permission profiles reference env var names, never values. The sensitive-paths hook applies here too.
- **Test before deploy**: Scripts should be testable locally before being symlinked into place.

## Quality layers (ADR-026)

Three quality layers, each owns a different question with a different threat model:

1. **Pre-commit hooks** (`platform/hooks/pre-commit`) — "will this commit obviously break something?" Local fast feedback. Skippable via `--no-verify`.
2. **Role gates** (`/gate-product`, `/gate-code`, `/gate-quality`, `/gate-arch`, `/gate-ops`) — "is this card team-acceptable?" Card-level done. Recorded on the card.
3. **CI** (`.github/workflows/quality.yml`) — "does main build cleanly from scratch?" Branch-protected on `main`.

**`--no-verify` is overridden by CI as authoritative on `main`.** A commit that bypasses pre-commit hooks locally will still be checked when its PR runs against `main`. Branch protection blocks merge of red PRs. Pre-commit failure messages reference this; the CI workflow itself is the source of truth.

Lock files (`package-lock.json` per active TS package + root, plus Cargo locks) are committed for reproducibility. CI uses `npm ci` against the locks; local installs that drift from the lock raise red flags. See ADR-026 for the full architecture and lock-file policy.
