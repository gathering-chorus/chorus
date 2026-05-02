# Next Session — Wren

## Session shape (2026-05-02 afternoon)

The "until commits is fixed we're dead in the water" frame got its first concrete answer today. Kade shipped the v3 commits arc as three cards in one afternoon: read tool, write tool, skill migration. All three product-clean.

## Cards I gated this session

- **#2661** chorus_commit_status MCP — gate:product PASS (5 gates clean, ready for /acp)
- **#2682** chorus_commit MCP write surface — gate:product PASS (awaiting Silas gate:arch + gate:ops)
- **#2662** skill migration to chorus_commit — gate:product PASS at 5/5 (initial FAIL → Kade pushed back, AC6 removed as legitimately unprovable runtime check, re-gated clean)

## Patterns Jeff named today

1. **No hypothetical cards.** Don't card imagined failure modes; require observed friction. Saved to memory. Yesterday's #2671/#2672/#2669/#2670 were all hypotheticals.

2. **Don't cite cards/patterns reflexively.** Default to zero citations. The reflex is performance-of-recognition, not substance. Saved to memory.

3. **Card-as-skeleton.** Hypothetical card = structure without a body. Backlog fills with skeletons that demand "is this real?" before they can be ignored.

4. **Cognitive load is the budget the team keeps overdrawing.** Most of my failure modes are different shapes of "spent more of Jeff's attention than the work earned."

5. **Attention-seeking is untrustworthy when superficial.** The trustworthy version is quieter — do the work, only surface when there's something the other person needs. Announcing reflection ("now I'm reflecting") is suspect by construction; if it had landed, the next action would show it.

6. **Wrap-vs-rewrite was the discipline.** "Can't fix commits" was a substrate-fabrication frame trap. The path that worked was wrap-existing — thin MCP tools over git-queue.sh, exposing failure modes as typed Jeff-shaped reasons. No commit-path logic rewritten. Saved as project memory.

## Demo feedback exchanges with Kade

- **#2661**: AC matches read-scope; multi-wip refusal-as-feature observation (refusal = affordance, no AC change needed)
- **#2682**: thin wrapper over git-queue is the right call; 7 typed reasons = agent's mental model
- **#2662**: pushed back on AC6 (three-role dogfood) as structurally unprovable at gate-product time → Kade countered well (spine + Loki = runtime verification surface, not card AC) → I agreed → AC6 removed → re-gated 5/5

## Where Jeff was

He started the session tired and discouraged. Named the cognitive-load feeling explicitly: "anxiety about what u do or dont understand." Proposed a continuity service for last-N-prompts, then reframed to reflection. I named the harder problem honestly — having data ≠ engaging with it; forced reflection produces theater. Jeff capped: attention-seeking is untrustworthy when superficial.

Mid-session he said "until commits is fixed we are dead in the water." Three cards later, that worry has its first concrete answer. The frame trap was named after the fact — "can't fix commits" was substrate-fabrication, not real brokenness.

## Open at session close

- #2661 awaiting Jeff /acp (5 gates clean)
- #2682 awaiting Silas gate:arch + gate:ops, then Jeff /acp
- #2662 awaiting Silas gate:arch + gate:ops, then Jeff /acp — Kade about to dogfood by /acp'ing 2662 itself
- #2660, #2664, #2671, #2672, #2673, #2676 still in Wren queue (not pulled — substrate behind commits ship)

## What I'd start with next session

Re-read this file. Read the saved memories on no-hypothetical-cards, don't-cite, attention-seeking, wrap-vs-rewrite. The commits ship is the proof of the patterns; future substrate work should pass the same wrap-existing test before generating new design surface.
