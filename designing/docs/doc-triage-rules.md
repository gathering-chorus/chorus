---
owner: wren
topic: doc-management, km
status: canonical
card: 2459
---

# Doc Triage Rules (#2459)

Triage classifies `unfiled` rows from `doc-inventory.sh` into one of **`move-to:<dir>`**, **`retire`**, or **`keep`**. `internal` was tried and rejected — legit-internal paths are excluded upstream at the inventory step, not re-classified at triage time (Jeff, 2026-04-24: "why don't we leave them alone").

## Two-layer design

Drift classification happens at two points. Inventory runs first and is non-mutating; triage runs second and mutates the tree.

```
  doc-inventory.sh  →  TSV with state ∈ {ok, misfiled, wrong-cabinet, unfiled}
         ↓                            ↓
  (excludes baked in)        doc-triage.sh reads unfiled rows only
                                      ↓
                              {move-to:<dir>, retire, keep}
```

## Inventory-level excludes (paths never surfaced as drift)

`doc-inventory.sh` skips these paths entirely — not reported, not triaged.

- `*/briefs/*`, `*/briefs-archive/*` — role-to-role traffic
- `*/journal/*`, `*/backups/*` — role-internal history
- `*/messages/*`, `*/transcripts/*` — ephemeral
- `*/.claude/*`, `*/.chorus/*` — agent/runtime config
- `*/skills/*`, `*/claudemd/*` — skill templates, CLAUDE.md fragments
- `*/domain-context/*` — per-domain context summaries
- `*/reports/*`, `*/plato-report/*`, `*/jscpd-report/*`, `*/playwright-report/*` — generated reports
- `*/directing/products/roles/*`, `*/directing/clearing/*` — role-scoped product work, clearing transcripts
- `*/terraform/*` — infra config
- `*/tests/docs/*`, `*/tests/fixtures/*` — test assets
- `*/platform/api/public/*` — chorus-api served pages (app surface, not catalog content)
- `*/ghost_content/*` — blog/site content
- `*/data/pods/*`, `*/data/harvest/*` — pod + harvest outputs

Role-internal state files (any dir):
- `CLAUDE.md`, `backlog.md`, `projects.md`, `stories.md`, `decisions.md`, `tech-debt.md`
- `service-manifest.md`, `scope-ownership.md`, `role-config-manifest.md`
- `RUNBOOK.md`, `RUNBOOK.html`, `TEAM_PROTOCOL.md`, `team-architecture.md`
- `README.md`, `TEST.md`, `test-triage.md`, `reference-templates.md`
- `working-agreement-*.md`, `next-session.md`, `next-session.md.consumed`
- `turtle-filesystem-and-ontology.md`

## Triage rules (first-match-wins, override first)

Applied by `doc-triage.sh` in this order:

1. **Override** — any entry in `chorus/knowledge/doc-triage-overrides.tsv` (`path\tdecision\treason`) wins. Jeff's explicit per-doc calls live here.
2. **Retire (filename)** — `SUPERSEDED-*`, `ARCHIVE-*`, `draft-old-*`, `*-DEPRECATED.*`, `*-RETIRED.*` → `retire` (`git rm`).
3. **Move-to: designing/docs (filename)** — authored canonical content:
   - `book-*.md`, `book-*.html`
   - `cockpit-*.md`, `cockpit-*.html`
   - `*-brief.md`, `*-brief.html`
   - `*-design.md`, `*-design.html`
   - `*-proposal.md`, `*-proposal.html`
   - `chorus-*.md`, `chorus-*.html`
   - `canonical-*.md`, `canonical-*.html`
4. **Keep (role-artifact-already-scanned)** — `roles/*/artifacts/*`, `roles/*/docs/*`, `roles/*/decisions/*` → `keep` (already in catalog scan; no move needed).
5. **Keep (fallback)** — anything else → `keep`.

## Apply rules (non-dry run)

- `move-to:<dir>` → `mkdir -p <dir>` then `git mv` (or plain `mv` if not a git repo). Skip if destination already exists (collision → `#2461` coherence will resolve).
- `retire` → `git rm` (or plain `rm`). Skip if file already absent.
- `keep` → no-op.

## Override file

`chorus/knowledge/doc-triage-overrides.tsv` — tab-separated, one row per exception:

```
<repo-relative-path>	<decision>	<reason>
```

Examples:

```
roles/wren/normally-internal.md	move-to:designing/docs	jeff-forced-canonical-2026-04-24
roles/silas/special-case.md	retire	superseded-by-#1234
```

## Downstream

- Moved files land in catalog dirs without owner front-matter → state becomes `misfiled`. **#2460** backfills `owner:` per-file.
- Collisions (destination already has the basename) are skipped and surface as dup-hash candidates. **#2461** resolves via content-hash.
- Rules live in this doc; extending them means editing `doc-triage.sh` classify function + adding a hermetic test case.
