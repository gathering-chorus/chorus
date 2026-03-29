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
├── alerting/         — Alert rules (canonical, synced to shared-observability)
├── skill/            — /chorus skill definition (symlinked to ~/.claude/skills/chorus/)
├── index/            — Database schema, init scripts
└── CLAUDE.md         — This file
```

## Runtime Artifacts

The SQLite index database lives at `~/.chorus/index.db` — it's runtime state, not source code. Scripts in `~/.chorus/scripts/` are symlinks to `chorus/scripts/`.

## Conventions

- **Canonical source**: Dashboards and alert rules live here. They get synced/copied to shared-observability for deployment.
- **No secrets**: Permission profiles reference env var names, never values. The sensitive-paths hook applies here too.
- **Test before deploy**: Scripts should be testable locally before being symlinked into place.
