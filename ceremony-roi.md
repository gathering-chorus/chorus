# Ceremony ROI Audit — 2026-04-19

**Card:** [#2217](https://) — Ceremony ROI audit
**Window:** 2026-04-12 00:00 → 2026-04-19 23:59 Boston (7 days)
**Data sources:** `~/.chorus/index.db` (128,768 spine events, 227 MB session JSONL, 118 role session files), card comments via cards CLI, live code inspection of skills/hooks/CLAUDE.md
**Count fidelity note:** Silas's first-pass data-side pull (07:00) and Wren's classification-table pull (~08:30) differ by ~1-3% per ceremony due to the window advancing during the session and per-role aggregation differences. Classifications unchanged. This doc uses the 08:30 pull.
**Authors:** Wren (ceremony-side enumeration + classification) + Silas (data-side inventory + turn-duration analysis)

## Summary

48 ceremonies enumerated across 9 surface categories, plus 5 tool mis-discovery friction points and 7 execution-variation axes. 22 ceremonies with hard data from spine events; 26 estimated from session-frequency + structure (flagged).

**Top finding:** turn duration is monotonically increasing across all 3 roles over the window, with inflection at 2026-04-17 commit `49b5837c`. p95 kade +87%, wren +55%, silas +45%. The commit (per-prompt pulse+spine+athena context-synthesis, correct fix for the boot-vs-per-prompt gap) is uncached + spawns a subprocess per turn. **Tune, do not revert.** Fix spec in [#2231](https://).

**Top retire candidates (by volume × low-evidence-of-catch):**
1. `observer.digest` — 18,836 events / 14.6% of all volume, catch rate unknown, 1-in-N or anomaly-only sampling recommended. [#2220]
2. Per-turn `session.context.built` cost — 5,435 events, same root as the inflection; [#2231] tunes the inject.
3. Gate-pass nudges (subset of 6,234 role.nudge events) — card comment is provenance, separate nudge is duplication. [#2222]

**Top data-backed keeps:**
1. `hook:scrub_block` — 1,229 genuine credential-write catches. Design working.
2. `hook:quality_block` — 566 bad-test rejections (post #2196/#2210/#2215/#2216 arc). Real enforcement.
3. `card.stale` — 2,944 stale-card surfacings. Catch signal — better at demo than at mystery.

---

## Ceremony table

**Legend:**
- **Evidence:** H = hard (spine event count), E = estimated (session-frequency + structure), C = code inspection
- **Catch/Decision/Tax:** rough % allocation; TAX = executed without producing a decision or catch
- **Rec:** KEEP = rate earns weight; SIMPLIFY = reduce cost, keep output; MERGE = combine with another; RETIRE = remove, data says it doesn't pay rent

### A. Session lifecycle (per-role, per-session)

| ID | Ceremony | Count | Ev | Catch | Decision | Tax | Rec | Rationale |
|:--|:--|--:|:-:|:-:|:-:|:-:|:-:|:--|
| C01 | Chorus prompt header (every response) | ~15k | E | 0% | 5% | 95% | **SIMPLIFY** | Fires every response (~15 tokens × thousands of turns). Catch is when Jeff uses timestamp to detect session drift — rare. Strip the werk version, keep role+date+card. |
| C02 | Session-start thesis boot | ~35 | H | 10% | 40% | 50% | **KEEP** | 5-beat narrative surfaces what role is thinking before Jeff asks. Decisions land (today's thesis, pattern named). Tax when session is continuation of active work. |
| C03 | Role state declare | 521 | H | 5% | 15% | 80% | **SIMPLIFY** | Ceremony of "I'm building X" often redundant with WIP card state. [#2227] removes the per-commit re-declare. |
| C04 | Activity.md append | ~25 | E | 10% | 30% | 60% | **KEEP** | Single audit trail across 3 roles + Jeff. When something goes wrong, activity.md is how we reconstruct. |
| C05 | next-session.md write | ~25 | E | 15% | 40% | 45% | **KEEP** | Survives the gap between sessions. Used by werk-init on next boot. |
| C06 | Close-out journal entry | ~20 | E | 5% | 20% | 75% | **SIMPLIFY** | Variance high (V04) — roles skip. [#2230] collapses Hard 5 into `/close`. |
| C07 | Wall-clock refresh per response | ~15k | E | 0% | 1% | 99% | **SIMPLIFY** | Fires every response, catches only on stale-clock bugs (#1559). Cache within the turn. |

### B. Card lifecycle

| ID | Ceremony | Count | Ev | Catch | Decision | Tax | Rec | Rationale |
|:--|:--|--:|:-:|:-:|:-:|:-:|:-:|:--|
| C08 | `cards move WIP` | ~120 | H (subset of 595) | 20% | 70% | 10% | **KEEP** | Pull trigger. Auto-generates blast-radius comment — see C12. |
| C09 | `cards move Done` | ~110 | H (subset) | 15% | 80% | 5% | **KEEP** | Done is a real decision point. Demo-gate protects from self-service (DEC-048). |
| C10 | `cards comment` | 648 | H | 30% | 40% | 30% | **KEEP** | Gate-pass comments + demo preflight + RCA notes. Catch rate high when comment is evidence-bearing. |
| C11 | `cards demo` (board log) | 692 | H | 5% | 80% | 15% | **KEEP** | Signals demo state for gate chain. Required by DEC-048. |
| C12 | Blast-radius auto-comment on WIP move | ~120 | H | 15% | 25% | 60% | **SIMPLIFY** | File list is useful — dumped inline is noise. [#2228] collapses to one-line summary + linked file. |
| C13 | Domain-radius auto-comment (missing context) | ~40 | E | 25% | 35% | 40% | **KEEP** | Warns when domain-context.md is missing. Real catch — surfaces gaps. Keep. |

### C. Skill-gated flows

| ID | Ceremony | Count | Ev | Catch | Decision | Tax | Rec | Rationale |
|:--|:--|--:|:-:|:-:|:-:|:-:|:-:|:--|
| C14 | `/pull` — full skill | 142 | H (pull.validate) | 25% | 50% | 25% | **KEEP** | 5 hard gates (validate, preflight, WIP check, domain context, TDD readiness). Catch rate real when AC/Experience is stale. Variance (V01) — some invocations skip steps. |
| C15 | `/demo` — full skill | 164 | H (demo.validate) | 30% | 50% | 20% | **KEEP** | Gate chain + smoke check + stakes brief. Real catches via gate chain. Variance on stakes-brief rigor (V02). |
| C16 | `/acp` | 384 | H (card.accepted) | 15% | 70% | 15% | **KEEP** | Accept + spine + commit + push. Load-bearing. |
| C17 | `/pair` | 30 | H (pair.start) | 35% | 45% | 20% | **KEEP** | 30 pair starts in 7 days. Navigator's scope loop catches cascades when live. Pair.end only 25 — 17% of pairs don't close formally (variance). |
| C18 | `/gemba` + `/gemba-tick` | ~20 | E | 20% | 30% | 50% | **SIMPLIFY** | Cron tick every minute for 10 min = 10 ticks/gemba × 20 gembas = 200 commentary events. Most ticks "quiet." Extend tick interval or exit-on-no-activity. |
| C19 | `/clearing` | 39 | H | 50% | 45% | 5% | **KEEP** | Real-time 3-role alignment. Low volume + high decision rate. Keep. |
| C20 | `/reboot` / `/refresh` | ~15 | E | 10% | 40% | 50% | **KEEP** | Context reload when rot detected. Rare but load-bearing. |

### D. Gate ceremonies

| ID | Ceremony | Count | Ev | Catch | Decision | Tax | Rec | Rationale |
|:--|:--|--:|:-:|:-:|:-:|:-:|:-:|:--|
| C21 | gate:product-pass | 272 | H | 40% | 55% | 5% | **KEEP** | Wren review of AC + experience. Real catches (rejections happen). |
| C22 | gate:code-pass | 174 | H | 35% | 60% | 5% | **KEEP** | Kade review of tests + build + pattern. Variance (V02) — some comments are one-liners. |
| C23 | gate:quality-pass | 10 | H | ? | ? | ? | **INVESTIGATE** | Only 10 in 7 days vs 174 gate:code. Signal mismatch — either under-used, or merged with gate:code in practice. File investigation card. |
| C24 | gate:arch-pass | 135 | H | 30% | 60% | 10% | **KEEP** | Silas review of system fit. Catch rate real (redirect rate). |
| C25 | gate:ops-pass | 20 | H | ? | ? | ? | **INVESTIGATE** | Only 20 in 7 days — same sub-count problem as gate:quality. Under-used or under-instrumented. |

### E. Hook-enforced ceremonies

| ID | Ceremony | Count | Ev | Catch | Decision | Tax | Rec | Rationale |
|:--|:--|--:|:-:|:-:|:-:|:-:|:-:|:--|
| C26 | `tdd_gate` | ~50 | E | 50% | 40% | 10% | **KEEP** | Blocks test-file writes without red-green. Real rejections happen (per Silas review). |
| C27 | `test_quality_gate` | 566 | H (card.quality.blocked) | **85%** | 10% | 5% | **KEEP** | 566 bad-test rejections in 7 days. Highest-ROI hook. Post-#2196/#2210/#2215/#2216 arc. |
| C28 | `write_scrubber` | 1,229 | H | **95%** | 5% | 0% | **KEEP** | 1,229 credential-write catches in 7 days. Design working. Keep as-is. |
| C29 | `icd_gate` | ~5 | E | 80% | 20% | 0% | **KEEP** | Rare but critical — blocks harvester writes without ICD. Low volume because devs stopped trying. Catch is in the deterrence. |
| C30 | `demo_gate` (pre-done) | 836 | H | 40% | 50% | 10% | **KEEP** | Real catches — stops self-service Done without demo. |
| C31 | `wip_card_gate` (pre-commit) | ~100 | E | 30% | 40% | 30% | **SIMPLIFY** | [#2227] restructures — reads board not /tmp cache. Eliminates false-block retries. |
| C32 | `sensitive_paths_gate` | ~15 | E | 70% | 30% | 0% | **KEEP** | Low volume, high catch when it fires. |
| C33 | `chorus_search_first` nudge | 9,432 | H (search.query) | 20% | 40% | 40% | **KEEP** | Search usage is the catch — it's the alternative to filesystem crawl. But too-frequent prompt can be noise; consider passive vs active hint. |
| C34 | `stop_hook` DEC-025 (permission-seeking block) | ~30 | E | 60% | 30% | 10% | **KEEP** | Catches the "should I..." pattern. Low volume, high corrective signal. |

### F. Messaging + coordination

| ID | Ceremony | Count | Ev | Catch | Decision | Tax | Rec | Rationale |
|:--|:--|--:|:-:|:-:|:-:|:-:|:-:|:--|
| C35 | `nudge` (persist + inject) | 6,234 | H | 20% | 40% | 40% | **SIMPLIFY** | [#2222] retires gate-pass nudge loop (subset of this count). Keep load-bearing nudges (actionable requests); retire redundant ones. |
| C36 | `chat.sh` threaded | 338 | H | 35% | 55% | 10% | **KEEP** | Sustained discussion surface. Real decisions land here (e.g., silas-wren-1776597433 today). |
| C37 | Clearing transcript capture | 39 | H | 45% | 50% | 5% | **KEEP** | Full-team alignment record. Real decisions indexed. |
| C38 | Brief file write to role dir | ~40 | E | 30% | 50% | 20% | **KEEP** | Durable async handoff. Variance in quality (V07). |
| C39 | Spine event emit (chorus-log) | 128,768 | H | varies | varies | varies | **CLASSIFY BY EVENT** | This row is umbrella; individual events classified above. Bulk volume = observer.digest (18,836) + heartbeat (not a role ceremony). |

### G. Decision / knowledge

| ID | Ceremony | Count | Ev | Catch | Decision | Tax | Rec | Rationale |
|:--|:--|--:|:-:|:-:|:-:|:-:|:-:|:--|
| C40 | DEC entry in decisions.md | ~20 | E | 20% | 70% | 10% | **KEEP** | Decision record. Low volume, high signal. |
| C41 | ADR write (data/architect/) | ~3 | E | 25% | 70% | 5% | **KEEP** | Rare, load-bearing. |
| C42 | RCA POST to /api/chorus/rca | 40 | H | 30% | 55% | 15% | **KEEP** | Today's #114 (nudge break) demonstrates. Need threshold (Jeff's "not every bug an RCA" — 2026-04-19). |
| C43 | Memory file write + MEMORY.md index | ~30 | E | 15% | 50% | 35% | **KEEP** | Carries forward. Tax when file is dup of existing. Tighten dedup check. |
| C44 | stories.md entry (personal) | ~10 | E | 40% | 40% | 20% | **KEEP** | First-class product input. Dani, Julian, Sabine entries today. Don't touch. |

### H. Board / admin / scheduled

| ID | Ceremony | Count | Ev | Catch | Decision | Tax | Rec | Rationale |
|:--|:--|--:|:-:|:-:|:-:|:-:|:-:|:--|
| C45 | Cost log entry at close-out | ~20 | E | 5% | 15% | 80% | **SIMPLIFY** | Runs cost, writes log, rarely reviewed. Automate via /close (#2230). |
| C46 | team-scan drain (nudge queue) | ~hourly | E | 20% | 30% | 50% | **KEEP** | Picks up queued nudges when osascript path blocked (today's TCC break). |
| C47 | Borg / team-scan tiles refresh | ~continuous | E | 10% | 20% | 70% | **KEEP** | Observability surface. Tax high, but it's the ambient awareness layer. Consider sampling. |
| C48 | `/chorus reindex` post-high-volume write | ~10 | E | 30% | 30% | 40% | **KEEP** | Needed after bulk operations. Rare. |

### I. Ambient observer (highest-volume, lowest-visibility)

| ID | Ceremony | Count | Ev | Catch | Decision | Tax | Rec | Rationale |
|:--|:--|--:|:-:|:-:|:-:|:-:|:-:|:--|
| OBS1 | `observer.digest` (per-tool-call) | 18,836 | H | ? | ? | ? | **SAMPLE OR RETIRE** | 14.6% of all spine volume. Catch rate unknown — sample 100 events per [#2220] and decide. If silent-pass > 80%, move to 1-in-N or anomaly-only. |
| OBS2 | `session.context.built` (per-turn) | 5,435 | H | 40% | 40% | 20% | **TUNE** | Catch via pulse+spine+athena context. Cost is the problem, not the output. [#2231] tunes inject.rs without changing envelope. |
| OBS3 | `board.audit.started` (per-role, scheduled) | 5,511 | H | 10% | 30% | 60% | **SIMPLIFY** | Scheduled cron. Value is what it triggers (card.stale.detected = 2,944), not the audit event itself. Can run less often; stale detection remains catch. |
| OBS4 | `interaction.pattern.detected` | 1,966 | H | 20% | 60% | 20% | **KEEP** | Wren's pattern classifier. Drives mode detection. |
| OBS5 | `hook:guard_decide` | 1,245 | H | 40% | 40% | 20% | **KEEP** | Umbrella of rule decisions. Real catches. |

---

## Tool mis-discovery friction (per 2026-04-19 07:00)

Not ceremonies — the inverse. 3-4x retry to find the right command form. Pure tax with no catch.

| ID | Friction | Evidence | Est. tax per hit | Rec | Card |
|:--|:--|:--|:--|:--|:--|
| C49 | `cards` CLI required fields first-fail noise | This session: #2217, #2212, #2214, #2220-#2231 each took 2-3 tries | ~2-5 tool calls per hit | **FIX** | [#2223], extends [#2143] |
| C50 | `board-ts` path drift | `/acp 2178` failed with `No such file` | 1-2 tool calls per hit | **FIX** | Folded into #2223 |
| C51 | Shell glob in chat.sh bodies | `*-summary`, `domain-*` eaten | 1-2 retries per hit | **FIX** | Needs card — draft: 'chat.sh shell-safe body encoding' |
| C52 | chat API shape mismatch | curl vs chat.sh wrapper | 1 retry | **DOCUMENT** | README improvement |
| C53 | Skill args ambiguity (/pair driver/navigator) | Had to re-read skill for override semantics | rare | **KEEP-AS-IS** | Low frequency |

---

## Variation in execution (second axis)

Not categories — axes across all ceremonies. Same ceremony, different rigor across roles/sessions. Compliance problem, not design problem.

| ID | Axis | Example | Implication |
|:--|:--|:--|:--|
| V01 | /pull skips TDD readiness or domain context | Variable per session | 5 hard gates, variable compliance |
| V02 | Gate-pass comment shape | "778 jest pass... SHA abc123..." vs "LGTM" | Same marker, 10x rigor delta |
| V03 | Session-start boot shape | 5-beat narrative vs jump-to-action | Same instruction, different shape |
| V04 | Close-out Hard 5 completion | Full vs truncated | Variable artifact completeness |
| V05 | Nudge framing | "Jeff wants review" vs "your domain — what does X mean for Y" | 10x response-quality delta |
| V06 | Card field population | Full Experience+AC+Context vs bare title | Variable downstream usability |
| V07 | Spine event kv fidelity | `event role=x` vs `event role=x card=N partner=M` | Same event, very different usability |

**Implication:** catch rate should be measured against BOTH "was it run" AND "was it run rigorously." Ceremony that catches 50% when rigorous and 0% when skimmed = compliance problem, not design problem. Fix compliance (shortening the ceremony, automating the rigor, removing the footgun) before retiring.

---

## Friction-remover cards filed off this audit

Twelve cards filed 2026-04-19. Planable, sequenceable, most already have concrete fix specs.

| Card | Title | Owner | Priority | Maps to |
|:--|:--|:--|:-:|:--|
| [#2220](https://) | Retire or sample observer.digest | Silas | P1 | OBS1 |
| [#2221](https://) | Identify 4/16-4/17 inflection (DONE) | Wren | P1 | turn-duration finding |
| [#2222](https://) | Retire gate-pass nudge loop | Wren | P2 | C35 subset |
| [#2223](https://) | Cards CLI ergonomics | Silas | P2 | C49-C52 |
| [#2224](https://) | Shrink per-turn context injection | Wren | P2 | C02 + C07 |
| [#2225](https://) | Quiet jest reporter | Kade | P1 | test output volume |
| [#2226](https://) | Scope gate-code to changed files | Kade | P1 | C22 cost |
| [#2227](https://) | WIP persists per session | Silas | P1 | C03 + C31 |
| [#2228](https://) | Collapse blast-radius comments | Silas | P2 | C12 |
| [#2229](https://) | Scoped smoke-check default | Silas | P2 | C15 step 3 |
| [#2230](https://) | /close single command | Wren | P2 | C04 + C05 + C06 + C45 |
| [#2231](https://) | Tune context_inject per-turn cost | Kade | P1 | OBS2 / inflection |

---

## Candidate cards NOT filed yet (surface from full table)

These emerged from the full classification. Filing them in a separate pass to not bury the above.

- **Investigate gate:quality and gate:ops low event counts** — 10 and 20 respectively in 7 days. Either under-used, under-instrumented, or folded into other gates. Evidence needed before deciding.
- **Dedup check on memory file writes** — C43 tax rate 35% likely from near-duplicate entries that passed a loose check.
- **Shell-safe chat.sh body encoding** — C51 pattern persists. Small code change.
- **Rotate pair.end enforcement** — 30 pair.start vs 25 pair.end means 17% of pairs don't close. Either declare pairs "ended by absence" automatically or require explicit close at 45-min ceiling.
- **Chorus prompt header simplify** — C01 touches every response; cut to role+date+card, drop Werk version unless stale.
- **Ambient observer sampling** — OBS1 + C47 similar pattern. Cross-cut spike.

---

## Team split plan (per Jeff 2026-04-19 08:30)

**Frame:** Silas is the broken-down car blocking the lane (owns the infra surface every role depends on). Stacking more cards on him continues the traffic jam. Split the work.

**Silas:** builds the new lane. [#2219] — value-stream-step proving pipeline service design. Longer arc, one artifact, infra-heavy content matches domain. Solo-with-review authoring (Wren reviews protocol stages; Silas fills infra stages).

**Wren:** fixes the flats, one at a time, with Jeff. Driver on #2221 (done), then: #2222 (retire gate-pass nudge loop) and #2230 (/close single command) are native domain. For Rust/jest-heavy fixes (#2225, #2231) — Wren investigates + writes exact-fix spec, hands diff request to Kade. Kade just finished #2205/#2209 and has cycles.

**Kade:** absorbs Rust/test quick-hitters from Wren's investigation hand-offs. Currently: #2209 closing, #2231 now routed to him.

**Critical path from this audit:**
1. #2231 (tune context inject) — kills the turn-duration inflection. Kade on it after #2209.
2. #2220 (observer.digest sample) — 14.6% of spine volume. Silas after #2219 design lands.
3. #2227 (WIP persists) — commit loop friction. Silas after #2219.

**What NOT to do:** stack #2220 + #2227 + #2228 + #2229 on Silas while he's designing #2219. That's repeating the traffic jam.

---

## Principle named (per Jeff 2026-04-19 08:40)

**Local optimization degrades the whole.** Each role ships their cards (local max), system-level friction compounds (whole-system cost). Each gate catches more (local), turns get 45-87% slower (whole). Each ceremony proves its individual catch (local), 18.7% of event volume is noise (whole).

The fix is mutual awareness at decision time, not after. Visibility into second-order effects at the point of decision — so Silas landing a hook change sees per-turn cost BEFORE shipping, Kade adding a test sees cumulative suite runtime, Wren filing a card sees queue depth per owner.

#2217's catch/decision/tax vocabulary is mutual-awareness language. #2219's stage handoffs are where that awareness lives by contract. This audit is the first explicit whole-system measurement; more like it should follow.

---

## Outstanding work (honest deferrals)

- **Bonus scatter plot** (catch-rate × tax-rate): not produced. Silas compacted before we could generate it. Defer to a followup spike.
- **Per-ceremony evidence citations** (specific JSONL references): summary level this doc; full evidence trail in [/tmp/pair-2217.md](https://).
- **Compliance vs design split per variation axis V01-V07**: named, not quantified. Needs session-sample classifier.

---

*Scratch file with full pair working-doc: `/tmp/pair-2217.md`. Committed via #2217 acceptance.*
