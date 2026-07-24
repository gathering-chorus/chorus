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
- Operational scripts (`shim-wrapper.sh`, etc.) resolve via `command -v chorus-hook-shim`; `target/release/` remains a fallback for pre-deploy systems. The bash `nudge` script was retired in #2804 (kept as a fail-loud stub for operational callers); agents send nudges via the `chorus_nudge_message` MCP tool.
- `test-hardcoded-bin-paths.sh` is a regression guard — fails if a non-test file adds a new hardcoded `target/release/chorus-*` path.

To answer "which commit is the live binary?" — query the spine for the latest `binary.deployed` event for the binary in question. The cdhash and commit fields tell you exactly what's running.

## Per-role Worktrees (#2735, #2913)

Each card gets its own ephemeral git worktree at `/CascadeProjects/chorus-werk/<role>-<card>/`, created from `origin/main` on `/pull` and removed on `/acp`. Canonical `/CascadeProjects/chorus/` always sits on `main` and is read-only during sessions; edits during a session land in the card's werk, not in canonical. Werks are **not** persistent-per-role — #2913 replaced the persistent `chorus-werk/<role>/` model (one directory mutated across cards via `repoint`) with ephemeral per-card worktrees. Branch-swap-in-place is gone, so the detached-HEAD failure class it produced (which burned the team 2026-05-13/14) cannot recur.

The protected primitive `/chorus/roles/<role>/` IS session-start is preserved — that's still where every role's session anchors to read role state. Only the *write* surface moves to the werk. See `designing/docs/version-control-service-design.html` for the full design.

Substrate scripts:
- Worktree lifecycle is **native in the Rust verbs** (#3431): `werk-pull` does `git worktree add` itself; `werk-accept`/`werk-unpull` tear down via the shared `werk-teardown` crate (refuse-if-dirty, two-tier merge proof #3014, orphan-propagate #3498, `.werk-mcp` teardown #3016). `platform/scripts/chorus-werk` (bash `add/remove/status/prune-merged`) remains for **manual ops only** — no verb shells to it. The `init / repoint / pull / close` verbs were **removed** by #2913 — there is no branch-swap-in-place anywhere.
- `platform/scripts/chorus-werk-sync` — lock-guarded `git pull --ff-only origin main` on canonical. Subcommands: default sync; `repair` (#2779) recovers a detached canonical by re-attaching HEAD to main, fast-forwarding to origin/main, and aligning the working tree. Use when sync aborts with "canonical HEAD is detached" — happens when an unpushed peer commit lands directly on canonical between syncs.
- `platform/scripts/chorus-env-setup.sh` — sets `CHORUS_HOME`, `CHORUS_WERK_BASE`, `<ROLE>_WERK`, `CHORUS_BIN`

Lifecycle: `/pull` (werk-pull) creates the card's worktree fresh from origin/main natively. `/acp` (werk-accept) tears it down natively via `werk-teardown` (#3431) — refuses on dirty werk, two-tier merge proof before branch delete, deletes local + remote branch, prunes the worktree admin entry, emits `card.branch.closed`. A role with two cards in flight has two separate worktrees; removing one never disturbs the other.

Edit/Write rules (enforced by chorus-hooks `canonical_write_guard`):
- Edits under `$CHORUS_HOME/...` from a role session → blocked, redirected to the card's werk
- Edits under another role's werk → blocked (cross-role); the guard parses the owning role as the segment before the first `-` in the werk-slot name (`<role>-<card>`)
- `/tmp/` and `/var/folders/` → allowed (sketch surfaces)
- Reads of canonical → allowed (role state lives there)

The `CHORUS_WERK_ENABLE` feature flag was **retired** by #2908 — the guard fires whenever the role is determinable; bootstrap / migration / generic-shell contexts (no role env) are still silent.

## Conventions

- **Canonical source**: Dashboards live here and get synced/copied to shared-observability for deployment. Alert rules live in `shared-observability/config/grafana/provisioning/alerting/` — single source at the deploy-source boundary; chorus-api references that path directly (#2620).
- **No secrets**: Live permissions are at `~/.claude/settings.json` (allow + deny + hooks). Never write env-var *values* into config files — reference names only. The sensitive-paths hook enforces this.
- **Test before deploy**: Scripts should be testable locally before being symlinked into place.

## Quality layers (ADR-026)

Three quality layers, each owns a different question with a different threat model:

1. **Pre-commit hooks** (`platform/hooks/pre-commit`) — "will this commit obviously break something?" Local fast feedback. Skippable via `--no-verify`.
2. **Role gates** (`/gate-product`, `/gate-code`, `/gate-quality`, `/gate-arch`, `/gate-ops`) — "is this card team-acceptable?" Card-level done. Recorded on the card.
3. **The merge gate** (ADR-053) — "is this land proven?" `werk-merge` content-verify + the local `act` run of `werk.yml` (build → blocking werk-test → deploy-werk → env-up → demo) + role gates, with Jeff's GO as the land authority (DEC-048). There is **no hosted per-PR CI and no branch-protection backstop on `main`** — that lane was cost-killed 2026-04-29 (#2600); ADR-026's layer-3 claim is superseded.

**The red-`main` detector is the 03:00 nightly** (`nightly-suites.sh`), not a per-PR check. `--no-verify` bypasses pre-commit only; the werk pipeline's blocking gates still run on every land. Known gap: the nightly does not yet file cards on red (#2527) — roles read its red list each morning (zero-red bar).

Lock files (`package-lock.json` per active TS package + root, plus Cargo locks) are committed for reproducibility. CI uses `npm ci` against the locks; local installs that drift from the lock raise red flags. See ADR-026 for the full architecture and lock-file policy.
