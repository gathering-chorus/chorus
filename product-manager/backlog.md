# Product Backlog

Last updated: 2026-03-24

**Active direction**: Chorus ops hardening — hooks, gates, TDD discipline. Photos pipeline paused at iPhone extraction (#1652) and merge logic (#1643). Bridge live at localhost:3470. NiFi installed on Bedroom (#1662).

## Prioritization: Now / Next / Later

**WIP limit: 3** (DEC-051). One card per role is the healthy state. If a fourth needs WIP, one must move to Done or Blocked first.

**Harvesting lane: WIP 2** (DEC-056). Pipeline/ingestion work has its own lane — separate from feature WIP.

**SWAT lane** (DEC-055). Crisis cards bypass WIP limit. Must close within one session.

**Source of truth: Vikunja board** (`cards list`). This file is the narrative companion — context, specs, rationale. The board is where you look for status.

---

## WIP (building)

### TDD discipline — #1674
- **Owner**: Wren
- **Priority**: P1
- **Description**: Tests before code. CLAUDE.md fragment deployed (v74), decision record written. Kade building Bridge + nudge test suites (AC #2, #3). Silas will extend demo-preflight hook (AC #4).

### Filter role-to-role nudges from Bridge — #1675
- **Owner**: Silas
- **Priority**: P2
- **Description**: Role-to-role nudges polluting Jeff's Bridge stream. Need sender attribution and filtering.

### iPhone photo extraction — #1652
- **Owner**: Silas
- **Priority**: P1
- **Description**: Repeatable governed pipeline from Jeff's phone to source graph. pymobiledevice3 installed, idevicebackup2 running.

---

## Harvesting

### Review Music and Photos ToDo sources — #1332
- **Owner**: Kade
- **Priority**: P3
- **Description**: ~590GB across /Volumes/Gathering/{Music,Photos}/ToDo and ~/Photos/ToDo-local.

---

## Next (ready, sequenced)

### Demo prep: agent-driven schema inference — #1641
- **Owner**: Wren
- **Priority**: P1
- **Description**: Live ICD mapping from unknown source for Deb and Allu demo.

### Name face clusters — #1631
- **Owner**: Kade
- **Priority**: P3
- **Description**: Jeff reviews 48 clusters, system persists names to Person nodes.

### Rebuild semantic embeddings — #1630
- **Owner**: Kade
- **Priority**: P2
- **Description**: Richer metadata for search quality after photo enrichment.

### Generate thumbnails — #1628
- **Owner**: Kade
- **Priority**: P2
- **Description**: Batch job on Bedroom Mac for all canonical photos.

---

## Later (parked)

See board (`cards list --status later`). Key items:

- **#1674 remaining AC** — Bridge + nudge test suites (Kade), demo-preflight extension (Silas)
- **#1667 Borg self-assessment** — 7 dimensions, first Borg use case (Wren P1)
- **#1666 Borg system ontology** — TOSCA, codebase graph research (Wren P2)
- **#1661 NiFi ETL research** — governed data pipelines (Wren P2)
- **#1644 Rebuild canonical photo graph** — from-scratch with era-scoped sources (Kade P1)
- **#1643 Era-scoped merge logic** — per-era primary/supplementary rules (Silas P1)
- **#1296 Scan book collection** — box-by-box catalog (Jeff P3)

---

## Done (recent, since 2026-03-24)

- #1671 Accept-gate hook — block /acp without demo brief, self-accept (Silas)
- #1676 Demo skill auto-brief — provenance brief auto-generated (Silas)
- #1658 Nudge blast radius — warn before interrupting WIP (Silas)
- #1659 Input classifier — statement vs command detection (Silas)
- #1665 Bridge consolidation — thinking stream, graceful restart (Silas)
- #1653 Takeout EXIF enrichment — 101K images (Kade)

---

## Tech Debt

See `engineer/tech-debt.md` for full details. Board cards in Later/Won't Do.
