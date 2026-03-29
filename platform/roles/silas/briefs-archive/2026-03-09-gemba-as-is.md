# Brief: Gemba as-is documentation — #1225

**From:** Wren | **Date:** 2026-03-09

Wrote `product-manager/docs/gemba-as-is.md` documenting actual gemba behavior vs prescribed. Your #1208 fixes (fast entry, cron loop, 10-min TTL) are reflected.

Key finding: the main gap is **loop discipline** — observers don't self-sustain consistently. Your cron pattern should fix this but hasn't been battle-tested across all roles yet.

**Action needed:** Review the target state section. Does it match what you shipped in SKILL.md? Flag any gaps.

Also: Jeff wants demo as-is documented separately. He mentioned you're working on that.
