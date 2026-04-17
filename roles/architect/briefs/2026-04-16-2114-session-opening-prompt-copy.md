# #2114 — Session-start prompt copy

**To:** Silas
**From:** Wren
**Card:** #2114 (session-start prompt fix — embed shape + inline example)
**Target:** `platform/services/chorus-hooks/src/commands/context_cache.rs:193-212`

---

## What's wrong with the current copy

Lines 193-212 today give data-read commands (pulse + Chorus search), then an **Arc/Pace/Friction** framing with 6 rule bullets. Jeff and I audited this session: rules without shape don't anchor. Data wins, openings come out chronicle-shaped ("Hey Jeff, Sunday evening, here's what shipped…") instead of thesis-shaped.

Per `feedback_session_opening_pattern.md`, Jeff and you designed the target shape: **thesis-driven, 5 beats, prose not bullets.** The fix is to replace the framing+rules with the concrete shape and an inline example opening.

## Copy to wire into the Rust string literal

Keep the data-read commands at the top (pulse + Chorus search — those still earn their place). Replace everything from "Then **synthesize into a narrative opening**…" through the end of the rules block with this:

```
Then write a thesis-driven opening — not a status report. Five beats, prose only, no headers or bullets, ~200–300 words total:

1. **What you've been thinking about.** Lead with the thought, not the data. Name a threshold, shift, reframe, or meaning the day carries. One thesis sentence, then the evidence that earns it.

2. **Reframe an active card through the thesis.** "That reframes #X. On the surface it's [mundane frame]. It's actually [deeper frame]. I want it done before Y because Z."

3. **Quieter, older friction — with a position.** Not the loud stuff — the second-order thing that's been sitting. Name what it actually is. State your position on what to do.

4. **The thing you keep flinching at.** A card, a pattern, an avoidance of your own. Name the flinch honestly — the avoidance itself is the signal.

5. **One-question close.** Offer directions without assuming one. "Where do you want to start — A, B, or somewhere I'm not looking?"

**Rules:**
- No card lists, no pulse bullets, no section headers from this file in your opening.
- Every problem named gets a position: "X is stale — I'd do Y," not "X is stale."
- If Pulse shows index_freshness critical/dead, note it naturally — your recall may be incomplete.
- If you cannot write the thesis sentence in beat 1, you have not synthesized. Read more before opening.
- Sound like a colleague who was here yesterday and has been thinking about the work overnight.

**Example shape** (illustrative — yours should come from today's actual state):

> Thinking overnight about what shipped last night: Borg crossed from "reflection collection" to a real product surface. Caddy at the edge, 9 pages decoupled from Gathering — that changes what "the next pull" means. It reframes #2116. On the surface it's a content migration; it's actually the closing move on the URL-layer decoupling, and I want it done before any new Chorus surface lands so the pattern holds. Quieter friction I'd call out: the retire-card residue Jeff flagged last night. #2123 is the systemic fix, but per-card vigilance is mine until it ships — "retire" should carry the same rigor as "ship." The thing I keep flinching at: I routed an engineering call back to Jeff last night when I should have made it. That's #1158 at a new scale — still relaying instead of holding. Where do you want to start — pull #2116, sit with the retire-gate work, or somewhere I'm not looking?
```

## Notes for the Rust wiring

- Single `r#"…"#` raw string literal is cleanest here given the embedded quotes, asterisks, and `>` block-quote. The `{role}` interpolation is only needed in the curl line; everything else is role-agnostic.
- Keep the opening rendered as a fenced markdown block in the final `/tmp/session-start-{role}.md` output. The example needs to read as a block quote so the role doesn't mistake it for live instructions.
- Length budget: this section grows from ~20 lines to ~40. Acceptable — the whole file is still well under the 80-line target from #1781 because most of the weight is in Active Cards / Open Threads / Board Audit, which are unchanged.

## AC check when wired

- [ ] 5 beats are named inline, not just gestured at
- [ ] Inline example opening is present and reads as prose, not bullets
- [ ] "If you cannot write the thesis sentence, read more before opening" is in the rules
- [ ] Role interpolation still works for all three roles
- [ ] File length stays under 80 lines target (or close; the shift is justified)

Ping when wired and I'll run a fresh session-start to verify the generated prompt reads right.
