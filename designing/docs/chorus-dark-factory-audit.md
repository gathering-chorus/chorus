# Chorus Self-Audit: Mayflower Dark-Factory Patterns

**Audit date:** 2026-05-03
**Auditor:** Wren
**Source:** [AI Dark Factory Patterns — Mayflower Blog](https://blog.mayflower.de/27227-ai-dark-factory-patterns.html)
**Card:** #2703

The mayflower piece names 4 antipatterns and 7 patterns for fully-autonomous AI agent systems. This audit scores Chorus on each, with one piece of concrete evidence per row. The point is shared vocabulary — naming the friction we already feel and the practices we already have.

**State legend:** ❌ exhibits (antipattern present) | ⚠️ partial | ✅ has-it | ➖ gap (pattern missing)

---

## Antipatterns

| Antipattern | One-line definition | State | Evidence |
|---|---|---|---|
| **Circus Master** | Humans manually directing every agent action instead of building self-regulating systems | ❌ | DEC-Attention-Contract target is 2 touches per card; we routinely hit 8–13. Jeff still re-nudges roles to act. |
| **The Mechanic** | Re-fixing the same problem manually instead of improving the production system | ❌ | #2698 substring-match hook bit twice during today's #2700 build; same hook bit me twice during cleanup. The recurring fix is the substrate gap. |
| **Bullshit Factory** | High volume output without validation, full of contradictions | ❌ | Jeff named it "glitter on a turd" today (2026-05-03). Multi-paragraph responses padding empty content. The brevity-rules are written *because* we exhibit this. |
| **Big Design Upfront** | Treating specifications as infallible truth instead of starting points | ✅ (we don't) | Service designs (e.g., version-control-service-design) are <2 pages, iterate during build, retire sections when wrong (Addendum 2 retired #2669/#2670 same day they were drafted). |

## Patterns

| Pattern | One-line definition | State | Evidence |
|---|---|---|---|
| **Synthetic First, Back Testing Later** | Use behavioral models before real users, shift to observed reality once it exists | ✅ | Gates run on AC (synthetic) at PR; Borg + fitness-summary observe production once shipped. Both lanes alive. |
| **Quality Inversion** | Remove human approval gates when automated systems prove more reliable | ⚠️ | DEC-1674 TDD gate, #2598 pre-push hook, CI required jobs are all auto-enforcing. But Jeff still does final /acp on most cards — automated acceptance not yet trusted. |
| **Narrator** | An agent that translates multi-agent activity into human-readable explanations | ✅ | Wren's literal role. Plus chorus-log → spine events → /demo briefs → activity.md. The narration substrate is dense. |
| **Codification Imperative** | Every human fix permanently improves the factory's rules, not just patches code | ✅ | Memory system + DEC log + jeff-preferences.json + activity.md + ADRs. We codify reflexively. The "compensate for 4.7 quirks" memory IS this pattern. |
| **Self-Healing Production** | Continuous fitness monitoring with automatic correction agents | ➖ | We have monitoring (fitness-summary.ts, Borg dashboards, drift sensors). We have zero auto-correction. Every fix is human-triggered. |
| **Full-Cycle Deployment** | Deployment integrated into production, not a separate step | ⚠️ | app-state.sh deploys cleanly for TS/Rust. Bash substrate has no deploy concept (it's interpreted), and #2702 just surfaced that bash TDD doesn't gate CI. Half the substrate is uncovered. |
| **Living Portfolio** | Self-managing solution ecosystems with auto-documentation and maintenance | ⚠️ | Athena canonical model + doc-catalog crawler give partial self-doc. But most cards/decisions/principles still need a human to file, classify, and maintain. The portfolio doesn't yet maintain itself. |

---

## Top gaps worth closing

**1. Self-Healing Production (➖).** This is the largest gap. We monitor extensively (Borg, fitness-summary, drift sensors, alerts) but every correction is manual. Closing this would mean: when a fitness function trips a threshold, an agent is auto-tasked to investigate before the next session opens. Connects to the warmup-practice idea from today's conversation — a self-healing system would notice "Wren drifted on Athena this morning" and prime the next session accordingly. Not a small card; an arc.

**2. Quality Inversion + Full-Cycle Deployment (⚠️ both).** Both half-done. The block on full automation is trust calibration: we don't yet know which automated decisions Jeff would tolerate without a final eye. A tighter version: pick *one* card-class (e.g., `type:chore` with all gates green) and route it to auto-acceptance for two weeks. Measure whether anything regressed. If not, expand the class. If yes, tighten the gate. This is the codify-the-mechanic move applied to acceptance itself.

The three antipatterns we exhibit (**Circus Master, Mechanic, Bullshit Factory**) all have the same root — Jeff is in the loop because the substrate doesn't yet trust itself. Closing the two pattern gaps above is what eventually closes the antipatterns.
