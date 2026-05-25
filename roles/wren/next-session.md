# Wren — Next Session

**Last session ended:** 2026-05-25 ~12:08 Boston via /reboot.

## Headline (read first)
**Foundational product positioning landed this session — operate from it.** Jeff = **Owner & Head of Product, NOT the customer.** Customer = someone who'll *pay Jeff to help them build something better than they could with solo Claude Code.* Product = the team-model (Jeff's judgment amplified by the team). The bar for everything = **"does it widen the gap vs solo Claude Code"** (not "traces to Jeff's use"). It's his **livelihood**, not a hobby/experiment; complexity must pay rent or it's an exhibit. My PM hat answers to him, can't self-own. Full detail: memory `project_product_is_team_model_vs_solo_claude_code`.

## WIP — #3080 (pulled, the real fix)
**Split chorus-api into tiers: ingestion workers feed the stores, a thin serving API reads them — decouple by data, not process.** Jeff's architecture call.
- **AC1–3 drafted** in `designing/docs/chorus-api-tier-split.html` (werk): two-tier model + diagram; inventory/classify (search/cards/context STAY; reindex/embed/crawl/cache-warm MOVE OUT, each proven not to need the API — the crawler literally curls its own API); migration plan (reindex first → embed → crawl folding in #3069 → cache-warm).
- **AC4 remaining**: the concurrency spike — SQLite WAL multi-process read/write (+ LanceDB) under load — VALIDATE FIRST, the whole design rests on it (note #3073 busy_timeout).
- Then: build cards per migration, each with its own runtime-outcome AC.

## Shipped this session
- **#3077 (Done)** — index byte-offset watermark + embed drop-COUNT+index. VERIFIED LIVE: POST /index 4-10s→30-150ms, /embed 10.5s→227ms (PR #348). AC4(search)/AC6(loop-freeze→0) SUPERSEDED by #3080.
- **#3071** crawler dependency-map (product rollup); **#3069** crawler instance-model; **#3070/#3076** gates passed.

## Key correction (don't repeat)
My AC4 "search not a blocker" was WRONG — built on an **isolated** FTS timing (0.25-0.8s). Silas's **#3079 instrument-the-block** captured the LIVE stack: `GET /api/chorus/search` blocks 5-7s under concurrency. Lesson hammered all session: **instrument the live block, don't infer from isolation / request logs.** Also: "spine" = the event log + emit code (narrow, mine); what freezes is **chorus-api** (the shared runtime) — don't conflate.

## Open threads
- **#3079** (Silas) instrument-the-block — did its job; near close.
- **#3066** collectRdf — async/Fuseki-latency, separate from loop-freeze; NOT in #3080's loop scope.
- **Unpull bug STILL OPEN**: `.git-commit.lock` is committed on origin/main (Silas's #3074 only added gitignore; the `git rm --cached` didn't land — verified). chorus_unpull_card false-refuses werk-dirty team-wide. See memory `project_committed_git_commit_lock_blocks_unpull`.
- **wren-3046 werk** has 1 stray uncommitted file (demo-v2 card) — not lost.
- chorus_commit can't stage brand-new untracked files under multi-werk ambiguity → use `git-queue.sh commit` from the werk (raw `git add` is hook-blocked).

## Pending peer
- Silas building #3079; owns crawler-stale threshold bump + the eventloop monitor.
