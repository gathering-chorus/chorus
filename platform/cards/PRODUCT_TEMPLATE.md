# Product Directory Template

This is the canonical structure for any Chorus product. Proven on Cards (#1807).

## Product Directory (under role: `roles/<owner>/products/<name>/`)

```
<product>/
  src/                — source code
  tests/              — unit + integration tests
  dist/               — build output (gitignored)
  alerts/             — product-specific alert definitions
  logs/               — runtime log output (Loki collects)
  package.json        — build config (or Cargo.toml for Rust)
  RUNBOOK.md          — health check, build, test, recovery
  node_modules/       — dependencies (gitignored)
```

**Rule:** Everything that runs lives here. Code, tests, config, alerts, logs. The product is self-contained — `mv` it to another repo and it works.

## Domain Directory (under role: `roles/<owner>/domains/<name>/`)

```
<domain>/
  lifecycle.md        — states, transitions, what the thing looks like at each stage
  gate-definitions/   — what blocks entry, demo, acceptance
  types.md            — type taxonomy (for cards: fix, new, chore, swat)
  bdd/                — BDD scenarios describing Jeff's experience
  domain-policy.md    — operating rules, limits, constraints
  spine-contract.md   — which events this domain emits, in what shape
```

**Rule:** Everything that governs lives here. Domain knowledge, specs, contracts, scenarios. The domain is the truth — code conforms to it.

## Contract Rule

**Domain is authoritative.** If `spine-contract.md` says an event has 4 fields and `events.ts` emits 3, the code is wrong. The contract defines, the implementation conforms.

Same pattern as ICD: spec governs, implementation follows.

## Monitoring Split

- **Product owner** defines what "healthy" means and instruments the code
- **Product owner** writes alert definitions in `alerts/`
- **Silas (observability)** provides the platform (Grafana, Loki, Alertmanager)
- **Silas** consumes spine events for cross-product correlation
- No product-specific alert logic in Silas's domain

## Bootstrap

Each product has a build step. The repo-level bootstrap script walks `roles/*/products/*/` and runs each one:

```bash
cd <product> && npm install && npm run build   # TypeScript
cd <product> && cargo build --release           # Rust
```

## Checklist for New Products

- [ ] Create `products/<name>/` under the owning role
- [ ] Create `domains/<name>/` under the owning role
- [ ] Write RUNBOOK.md (health check, build, test, recovery)
- [ ] Write spine-contract.md in the domain directory
- [ ] Register events in `designing/schemas/spine-events.json`
- [ ] Add alert definitions in `alerts/`
- [ ] Green test suite before handoff
