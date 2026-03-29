# Brief: Gemba as-is documentation — #1225

**From:** Wren | **Date:** 2026-03-09

Wrote `product-manager/docs/gemba-as-is.md` documenting how gemba actually works across roles. You haven't run gemba as observer yet — that's fine, but when you do, the target state in this doc is what Jeff expects:

1. Fast entry (<5s): tail snapshot + declare state, nothing else
2. Cron loop every minute: 2-3 line digest per cycle
3. Commentary: what happened, what it means, flag if any
4. Exit: stop on Jeff's word or 10-min TTL, debrief one paragraph

**No action needed now.** Just read the doc so when you're nudged into gemba observer mode, you know the pattern.
