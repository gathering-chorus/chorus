# Next session — kade

## Reboot context (2026-05-25 ~12:04 PM EDT)

Jeff invoked /reboot mid-card. #3078 is WIP, doc committed but NOT demo'd/acp'd. Read activity.md + this file first.

## WIP — #3078 (werk-subproduct-design.html)

**Branch `kade/3078`, committed through `cab14954`, NOT acp'd.** This session was a long voice/framing rework of `designing/docs/werk-subproduct-design.html`. Do NOT /acp without Jeff's call — he's been driving each pass.

**The framing we landed on (this is the important part — don't regress it):**
- **Werk is the Chorus *execution protocol* — "how we execute."** NOT "the Building-step subproduct." Earlier framing let the value-stream step-map box Werk into Building; Jeff corrected: "i think u let the step map limit u" / "werk is the chorus protocol - how we execute / its primary step is building yet that is not all."
- **Value is realized in TWO steps: Building AND Proving.** Jeff: "thats where value gets realized - building and proving." Upstream steps (shaping/designing/directing) produce *intent*; Werk turns intent into running, verified software. Building makes it *real* (running on main), Proving makes it *true* (/demo→/acp). Verbs map: /pull+build=Building, /demo+/acp=Proving. Do NOT overclaim "spreads evenly across all 5 steps."
- **Werk is *versioned*.** Jeff: "i honestly want that to show on every turn and that is our protocol version." The `Werk v1.4` in every session header IS the protocol version all 3 roles run under, drift-checked (#2311). Framed in §1 as the concrete anchor ("you are inside Werk before you have pulled a single card") + §3 as the most-seen surface. Werk = the versioned protocol the whole session runs under; the card pipeline is where it realizes value.

**TL;DR voice:** Jeff flagged the TL;DR "still kinda reads like a tech inventory." Rewrote value-first (no verb/contract/trace/check parts-list). §1 problem statement rewritten in human, problem-first voice with concrete felt failures (Done-on-board-while-stale-binary-serves, "breaks in front of Jeff", can't-say-which-commit-runs).

**Research backing:** read 5 real human-authored design docs (Google/Malte Ubl, Rust NLL RFC, React Server Components RFC, Oxide RFD 1, Pragmatic Engineer survey). Pattern: open with a *felt* problem + concrete anchor, plain-English WHAT through outcomes, human voice, never a feature inventory. One move NOT yet taken: anticipate-skepticism FAQ ("isn't this just CI/CD?") — offered, Jeff didn't bite.

## Pending (next session, pending Jeff)

1. **Template sync** — `designing/docs/subproduct-design-template.md` still describes the OLD 5-section shape. Needs: the versioned-protocol framing, value-first TL;DR rule, problem-first §1 with felt-anchor, "value realized in its steps" framing, the human-voice/no-inventory rule. I offered to sync it ~3x; Jeff kept driving the doc instead. Offer again.
2. **#3078 demo/acp** — Jeff's call. Doc is in good shape. ADR-033 file landing via Silas (roles/silas/adr/) is an external dep the doc references.
3. **Other product docs** — Jeff: "truthfully all of our product docs need some thinking / if we are value stream and model aligned." The shape we built for Werk is the template for all 7 product docs.

## Notes
- `localhost:3340` serves canonical `main` — it shows the STALE pre-restructure doc until /acp merges. To view current work, open the werk file directly (file:///.../chorus-werk/kade-3078/designing/docs/werk-subproduct-design.html).
- I failed to print the Chorus prompt header for most of this session until Jeff flagged it ("i honestly want that to show on every turn"). Print it every turn.
