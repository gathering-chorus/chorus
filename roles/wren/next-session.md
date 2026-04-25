# Next Session — Wren

## This session shipped (2026-04-25 morning)

**Drift discussion on `/loom/principles-reference-impl.html`** — surfaced that the page violates I-1 on its own subject. Card-status sections (#2450, #2451 mermaid classes) are hand-edited and stale within 45 minutes of the last edit. Position banked for Jeff (FYA, not action): static-status sections must read live from API; editorial sections (Heart, Layers, Invariants) stay hand-curated. The "reboot-and-reread" cycle IS an empirical drift test — any document that can't survive it is paraphrase pretending to be contract.

**Gate-product runs:**
- **#2475** (MCP observability + governance) — initial FAIL on AC 5/6 shape (split-off [ ] under AC), retry-3 PASS at 6/6, formal forward to Kade.
- **#2476** (Principles MCP tools) — gate-product PASS retracted mid-conversation. Performative-gates pattern: AC 9/9 + Silas tests green satisfied technical surface, but **Wren cannot reach the typed tools from this session** (ToolSearch returned no chorus_*) — which IS the user-surface check. Reach-test is the AC. Silas mapped path: env injection from .claude/settings.json + reboot + ToolSearch + live call → spine evidence closes new AC#7. **Reboot is the trigger for #2476 close.**

**ADR-026 (CI architecture + lock-file policy)** — driven by Kade hitting local↔CI divergence (gitignored package-lock.json) on #2481 build. Kade escalated to Wren via chat kade-wren-1777128773. Position: ADR shape, four decisions, Silas-owned. Silas drafted v1 fast; Wren PM-review APPROVE w/ 3 changes (drift-warning in §b, fold three follow-ons into #2481 AC, reconcile 12 vs 6 package count); Silas v2 + v3 applied; Kade impl-review PASS; ready for Jeff final + branch-protection toggle.

**Drive harvest analysis (deeper)** — sampled ~200 files. Mapped to Gathering domains. Architectural call: build ONE Drive harvester with per-folder/per-pattern config, not N one-offs. High-fit harvest targets: grandma audio interview, resume time-series, jeff-bridwell-user-guide, life-purpose+core-values+wants bundle, finance spreadsheets, garden refs. Mid-tier: FTF/LendCo 2023-07 career archive (~30 PDFs — same shape as Staples archive, direct Chorus design lineage). Defer/archive: ~50 cover letters (metadata only). Two flags raised before any card filing: (1) Gathering domain readiness gap, (2) classification policy needed before sensitive harvest. **Open question to Jeff:** draft ADR-027 ("Drive as a harvest source") now, or scan Gathering schema readiness first?

**Career pivot memory + canonical model lineage memory + script-paths memory + Loki-before-bug memory** — four memories banked from this session. Notable: re-read of 2011 Staples Canonical Model Governance + Interface_and_Mapping_Capture decks confirmed Chorus is direct lineage from Jeff's 2011 ESB work (same Cambridge Semantics → Anzo → Athena vendor thread). Career arc visible in Drive: ~35 cover letters 2024-25 → re-source from own canon → built Chorus. Frame Chorus as "thing he'd want to be hired for," not side project.

## Open threads to resume

1. **#2476 reach-test (load-bearing).** First action this session: run ToolSearch for `chorus_principles_list` from this fresh seat. If visible, call it (any reasonable principle id) — spine event with `from=wren, origin=mcp` closes new AC#7. Silas + Kade waiting. After reach-confirmed: re-route to Kade for /gate-code re-run, then re-/gate-product for clean PASS, then full close.

2. **ADR-026 — Jeff sign-off pending.** Silas + Kade signed; Wren PM signed. Jeff's two actions: flip Status: Proposed → Accepted at top, toggle GitHub branch protection on `main` (Settings → Branches → Add rule, require CI green to merge).

3. **#2476 Kade/Silas open question on tool descriptions.** WHEN-guidance in `chorus_principles_get` description is grounded (names CLAUDE.md citation / card / list as call-sites) but lacks a positive example showing the rhetoric ("e.g., role file cites principle-ship-small in scope; fetch full body to ground a sizing argument"). Position: ship as-is, refine description as a follow-up bundle when third tool lands. Don't block close on copy.

4. **Drive harvest decision pending Jeff.** Either draft ADR-027 ("Drive as a harvest source" — four decisions: source enumeration, extractor table by file-type, classification policy split, drift/idempotency posture) OR first scan Gathering domain schema readiness so the ADR doesn't promise harvest into empty domains. Wren recommends scan-first.

5. **JX/AX gap in gate-product skill.** Jeff named the meta-miss: gate-product checks machinery (AC count, comment, spine call) but not experiences (does Jeff see/use it; does an agent reach for it). Position: amend skill to add JX-confirm + AX-confirm as required gates 8 and 9. Held during demo loop, not yet executed. Same risk applies retroactively to #2475 (alert visible to Jeff?), #2451 (SubDomain detail page reached?).

6. **Principles reference-impl page** still stale on card statuses (#2450 + #2451 should not class :::later). FYA, not action — Jeff explicit on that.

## WIP at session close
- **#2476** — WIP, gate-product retracted, awaiting reach-test.
- Wren WIP = 1/1.

## What's queued for Wren (unchanged)
- #2470 — harden check-principle-direct-edit hook (mods + deletes)
- #2471 — multi-parent permaculture audit
- #2123 — retirement gate
- #2125 — Borg handler error observability
- #2116 — Chorus page migration parent (still the long-running flinch)

## Friction notes (this session)
- Script paths: `chorus/platform/scripts/` is canonical for `nudge` + `chorus-log` + `cards`. Wrong paths (`chorus/scripts/`, `gathering-team/scripts/nudge.sh`) silently failed when run in background. Memory banked.
- Loki-before-bug: filed #2484 against a non-bug (origin-tagging test traffic) without checking trace_ids. Retracted after Jeff prompted "we have logs / loki wren!" Memory banked.
- Ankle-biter cards: filed #2483 (folded) + reference-doc-line-fix card-shape (dropped) before Jeff named the pattern. Net new this session: #2482 only.
- Performative gates: gate-product PASS on #2476 without reach-test. Retracted mid-conversation. The deeper miss is JX/AX absence in the skill itself (open thread #5 above).

## Reach-test for next Wren session
Run as first turn after boot envelope:
```
ToolSearch query "chorus_principles_list"
# if visible:
mcp__chorus__chorus_principles_list (or whatever the surfaced name is)
# spine event should tag from=wren, origin=mcp
# that closes #2476 AC#7
```
