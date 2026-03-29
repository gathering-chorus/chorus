# Chorus v1.0 Boundary

**Date:** 2026-03-05
**Status:** Draft — needs Jeff review

## What v1.0 Means

Everything inside this boundary works reliably, every time. No silent failures, no "it works sometimes." If Jeff uses it, it responds correctly. If it breaks, it tells you.

Everything outside waits until v1.0 is solid.

---

## IN — Works Today (maintain, don't touch)

| Surface | Status | Notes |
|---------|--------|-------|
| Board + card flow | Solid | cards CLI, Vikunja, card lifecycle |
| Briefs + handoffs | Solid | Write to recipient dir, activity.md logging |
| Search (FTS + semantic) | Solid | BM25 + LanceDB hybrid, just refreshed (#883) |
| Chorus index | Solid | 50K messages, Slack + Claude + briefs + decisions |
| Loom page | Solid | Role tiles, blog excerpts, team pulse (#1066) |
| Werk page | Solid | Flow metrics, instruments tab (#621) |
| Mind map / hub | Solid | Collapsed spoke design (#689) |
| Deploy pipeline | Solid | app-state.sh, bind mounts for views/static |
| Harvest pipelines | Solid | Music, photos, Facebook, LinkedIn, blog, notes |
| CLAUDE.md generation | Solid | Fragment-based, claudemd-gen.sh |
| Git queue | Solid | Serialized commits via lockf |

## IN — Needs Hardening (fix before v1.0 ships)

| Surface | Issue | Card | Owner |
|---------|-------|------|-------|
| Gemba | Startup was 60s, now simplified. Needs real-world proving across sessions | — | Wren |
| Andon / role state | Just shipped (#1070). Declared state works but enrichment staleness, edge cases untested | #1070 | Kade |
| Spine events | Events emit but no e2e test proving flow from emit → chorus.log → index → query | #1075 | Silas |
| Close-out | Noisy, 4 warns on Silas last close. #1073 in progress | #1073 | Silas |
| Posture/mood capture | Broken — LaunchAgent exit 127, no captures, /lm dead | #899 | Silas |
| Chorus SDK | MVP spine in progress, not yet usable by external consumers | #972 | Silas |
| Doc-drift gate | Detects stale docs but self-accepted (#763). Needs proving gate compliance | #763 | Silas |
| Ops-agent card creation | Fires too aggressively, 50+ Won't Do cards from noise | #936 | Silas |

## OUT — Deferred Past v1.0

| Surface | Why deferred |
|---------|-------------|
| /talk (voice input) | #269 — high value but new surface area, not hardening |
| /listen (receive mode) | #547 — same: new input mode, not stabilization |
| Clearing voice tuning | #265 — depends on stable Chorus context injection |
| Garden observability | #624 — entirely new domain |
| Drone survey pipeline | #655 — speculative |
| Seed vs Glimmer taxonomy | #195 — ontology work, not operational |
| CMDB | #407 — valuable but additive |
| Self memory partition | #939 — depends on stable Chorus read API |
| Node 22 upgrade | #537 — risk with no user value |
| Morning wake-up daemon | #539 — nice-to-have automation |
| Pre-response decision gate | #1010 — needs stable spine first |
| Re-prompt rate metric | #981 — needs stable spine first |

## Hardening Definition

A surface is "hardened" when:
1. **It works on first use** — no setup failures, no stale state
2. **Failure is visible** — if it breaks, Jeff sees an error, not silence
3. **It has been proven** — demo to Jeff, accepted, used across 3+ sessions without incident
4. **Blast radius is mapped** — dependencies documented (DEC-072)

## Exit Criteria for v1.0

- All "Needs Hardening" items either fixed or consciously deferred with reason
- Zero silent failures in core surfaces (gemba, andon, spine, close-out, posture)
- Jeff can run a full day — boot, gemba, direct work, demo, close — without hitting a broken tool
- Chorus SDK has at least one external-facing example (even if internal use only)

## What This Changes

- **New feature cards go to Later** until hardening is complete
- **Priority inverts**: reliability > capability. Fixing posture capture > building /talk
- **Proving gate tightens**: "works across 3+ sessions" before Done
- **Blast radius required**: DEC-072 on every card entering WIP
