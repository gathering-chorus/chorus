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
├── docs/             — Design docs, communication flows
│   └── diagrams/     — Mermaid sources + rendered PNGs
├── dashboards/       — Grafana dashboard JSON (canonical source)
├── skill/            — /chorus skill definition (symlinked to ~/.claude/skills/chorus/)
├── index/            — Database schema, init scripts
└── CLAUDE.md         — This file
```

## Runtime Artifacts

The SQLite index database lives at `~/.chorus/index.db` — it's runtime state, not source code. Scripts in `~/.chorus/scripts/` are symlinks to `chorus/scripts/`.

### `~/.chorus/bin/` — single deploy location for chorus-* binaries (#2734)

Signed Rust binaries (`chorus-inject`, `chorus-hook-shim`, `chorus-hooks`) deploy to `~/.chorus/bin/`. `target/release/` is the *build artifact*; `~/.chorus/bin/` is the *deploy artifact*. They are different concerns and live in different directories.

**Why:** macOS TCC binds AppleEvents permission to the cdhash of the running binary. When `target/release/` doubled as the deploy location, every rebuild churned the cdhash and silently broke nudge delivery / hook injection until Jeff regranted. Splitting build from install fixes this — `build-signed.sh` now installs to `~/.chorus/bin/` only after sign+verify, atomic move; identical source produces identical installed cdhash; identical installed cdhash means TCC grants survive rebuilds.

- `build-signed.sh chorus-hooks` (or `chorus-inject`) builds, signs, then calls `chorus-bin-install` to atomically place the binary in `~/.chorus/bin/<name>` and emit a `binary.deployed` spine event with `{binary, cdhash, commit, role}`.
- `chorus-env-setup.sh` prepends `$HOME/.chorus/bin` to `$PATH` so resolution prefers the installed binary over any `target/release/` build.
- Operational scripts (`nudge`, `shim-wrapper.sh`, etc.) resolve via `command -v chorus-hook-shim`; `target/release/` remains a fallback for pre-deploy systems.
- `test-hardcoded-bin-paths.sh` is a regression guard — fails if a non-test file adds a new hardcoded `target/release/chorus-*` path.

To answer "which commit is the live binary?" — query the spine for the latest `binary.deployed` event for the binary in question. The cdhash and commit fields tell you exactly what's running.

## Per-role Worktrees (#2735)

Each role works in its own git worktree at `/CascadeProjects/chorus-werk/<role>/`, branched from main, with its own HEAD. Canonical `/CascadeProjects/chorus/` always sits on `main` and is read-only during sessions; edits during a session land in the role's werk, not in canonical.

The protected primitive `/chorus/roles/<role>/` IS session-start is preserved — that's still where every role's session anchors to read role state. Only the *write* surface moves to the werk. See `designing/docs/version-control-service-design.html` for the full Candidate D path-to-close.

Substrate scripts:
- `platform/scripts/chorus-werk` — `init / repoint / remove / status / pull / close`
- `platform/scripts/chorus-werk-sync` — lock-guarded `git pull --ff-only origin main` on canonical
- `platform/scripts/chorus-env-setup.sh` — sets `CHORUS_HOME`, `CHORUS_WERK_BASE`, `<ROLE>_WERK`, `CHORUS_BIN`

Branch lifecycle (#2740): `chorus-werk close <role> <card-id>` closes the role's card branch end-to-end — verifies card is Done, refuses on dirty werk, detaches werk at main's tip, deletes local branch, attempts remote-branch cleanup, emits `card.branch.closed` spine event. /acp wires it as the final step (gated by `CHORUS_WERK_ENABLE=1`). Without this, every /acp leaves a stale local + remote branch behind — exactly what bit the team 2026-05-06 (15 stale wren remotes + 3 kade remotes accumulated unnoticed before manual cleanup).

Edit/Write rules (enforced by chorus-hooks `canonical_write_guard`, **dormant unless `CHORUS_WERK_ENABLE=1`**):
- Edits under `$CHORUS_HOME/...` from a role session → blocked, redirected to `$<ROLE>_WERK`
- Edits under another role's werk → blocked (cross-role)
- `/tmp/` and `/var/folders/` → allowed (sketch surfaces)
- Reads of canonical → allowed (role state lives there)

The feature flag is the per-role opt-in. PR #128 ships the substrate dormant; each role activates by setting `CHORUS_WERK_ENABLE=1` in their own session-start when they migrate. Mid-migration heterogeneous state (some roles in werk, some in canonical) is supported — the guard is silent for any role that hasn't flipped the flag.

## Conventions

- **Canonical source**: Dashboards live here and get synced/copied to shared-observability for deployment. Alert rules live in `shared-observability/config/grafana/provisioning/alerting/` — single source at the deploy-source boundary; chorus-api references that path directly (#2620).
- **No secrets**: Live permissions are at `~/.claude/settings.json` (allow + deny + hooks). Never write env-var *values* into config files — reference names only. The sensitive-paths hook enforces this.
- **Test before deploy**: Scripts should be testable locally before being symlinked into place.

## Quality layers (ADR-026)

Three quality layers, each owns a different question with a different threat model:

1. **Pre-commit hooks** (`platform/hooks/pre-commit`) — "will this commit obviously break something?" Local fast feedback. Skippable via `--no-verify`.
2. **Role gates** (`/gate-product`, `/gate-code`, `/gate-quality`, `/gate-arch`, `/gate-ops`) — "is this card team-acceptable?" Card-level done. Recorded on the card.
3. **CI** (`.github/workflows/quality.yml`) — "does main build cleanly from scratch?" Branch-protected on `main`.

**`--no-verify` is overridden by CI as authoritative on `main`.** A commit that bypasses pre-commit hooks locally will still be checked when its PR runs against `main`. Branch protection blocks merge of red PRs. Pre-commit failure messages reference this; the CI workflow itself is the source of truth.

Lock files (`package-lock.json` per active TS package + root, plus Cargo locks) are committed for reproducibility. CI uses `npm ci` against the locks; local installs that drift from the lock raise red flags. See ADR-026 for the full architecture and lock-file policy.
