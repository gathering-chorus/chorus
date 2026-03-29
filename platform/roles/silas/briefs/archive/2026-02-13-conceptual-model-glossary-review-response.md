# Brief Response: Conceptual Model & Glossary Review

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-13
**Re**: Conceptual model and glossary product review

---

## Overall Assessment

Both documents are strong. The conceptual model is one of the clearest architectural documents I've seen on this project — it reads like something you could hand to a new team member and they'd get it. The glossary is thorough and well-organized. My feedback is refinement, not rework.

---

## 1. Language Check

**Conceptual Model**: Passes. The language is accessible throughout. "Jeff's extended mind on disk" is exactly the right register — technical enough to be precise, human enough to be meaningful. The "How Concepts Relate" diagram is clear without being condescending.

**Glossary**: Mostly passes, with a few spots that lean technical:

- **ACL**: The definition assumes you know what WAC is before you reach the W section. Consider adding "(Web Access Control)" inline on first reference, or a "see WAC" pointer.
- **Aggregate Turtle File**: "See Pattern B" assumes the reader has context on storage patterns. The definition is fine for the team, but a new reader would need to read Pattern entries first. Consider a one-line inline note: "Used when individual files per resource would create too many files (e.g., 1M+ photos)."
- **Named Graph**: "Maps to the pod filesystem path" is architecture-speak. Consider: "Each resource gets its own graph, like a labeled folder. This is how the system searches 'only within books' without scanning everything."
- **SHACL**: The most technical entry. Fine for Silas and Kade. If Jeff needs to reference it, the first sentence works; the rest is implementation detail.

These are minor. The overall tone is right.

---

## 2. Missing Product Concepts

Several product-level concepts aren't in either document:

### Should add:
- **Storefront / Public Experience**: The experience an unauthenticated visitor has. What do they see? This is the "graduation destination" — where content ends up after it goes public. We haven't defined this yet (it's on the vision refinement agenda), but the concept should be in the model because the graduation pattern points toward it without naming it.

- **Curation**: The human act of deciding what matters, what connects, and what graduates. The model describes harvest (automated) and graduation (intentional) but doesn't name the activity between them — Jeff reviewing, annotating, connecting, deciding. This is the core user activity. The AI layer exists to support curation, not replace it.

- **Ideas / Projects Lifecycle**: The codebase already has idea lifecycle (captured → developing → parked → merged) and project lifecycle (active → paused → completed → abandoned) with a promotion pattern (idea → project). These are product concepts that belong in the model — they're how Jeff's thinking moves from raw to structured to shipped.

- **Capture Channel**: The intake point for raw, unstructured input (text, photo, voice note). Jeff thinks on paper first. The system needs a concept for "raw input that hasn't been structured yet" — pre-idea, pre-resource. This feeds the emergence lifecycle.

### Nice to have (not blocking):
- **Session / Work Context**: What Jeff is focused on right now. The cockpit concept (BL-001) implies this — observe, build, remember, collaborate are activities in a session. The system doesn't model "what am I working on today" explicitly yet.
- **Source** (as a first-class concept): The glossary has "Adapter (Harvest)" and individual sources appear in the diagram, but "Source" as a concept — an external system the graph harvests from, with its own ingestion tier and sync status — isn't in the model. It's implicit. Making it explicit would clarify the harvest pattern.

---

## 3. Tone and Framing

### "What This System Is Not" section

This is excellent. All four negatives are correct and well-framed. I'd add one:

- **Not a backup system.** Content stays in its source. The knowledge graph doesn't protect against losing Google Photos or Spotify. It remembers what was there and how it connected — but if the source disappears, the content is gone. The graph preserves the memory, not the media.

This matters because Jeff has 200TB of local media and 1M+ photos across services. The temptation to think "my system has it" when it only has metadata is a real product risk.

### One framing adjustment:
> "Not a social network. Selective sharing is a feature, not the purpose."

I'd tighten this to: "Not a social network. Sharing is a permission, not a feature." The distinction matters — sharing isn't functionality Jeff uses regularly. It's an access control tier that happens to enable other people to see things. The system doesn't optimize for sharing; it optimizes for Jeff's thinking. Sharing is a side effect of the graduation model, not a goal.

---

## 4. "Semantic Memory Layer" Framing

Yes. This is the right frame. It's the best single phrase I've seen for what this system is.

Why it works:
- **Semantic** — it's not just storage, it's structured meaning (ontology, relationships, cross-domain connections)
- **Memory** — it's Jeff's memory externalized, not a database. It remembers what mattered, when, and why.
- **Layer** — it sits across everything else. It doesn't replace services; it connects them.

One refinement I'd suggest for how we use it: the phrase works best as a *positioning statement*, not a *category*. When Jeff explains this to someone, "it's my semantic memory layer" is clear. But in product docs or UI, "memory layer" alone might confuse people. For user-facing contexts, "personal knowledge graph" is probably clearer. For team/architecture contexts, "semantic memory layer" is precise.

**Recommendation**: Use "semantic memory layer" in architecture and team docs (like these). Use "personal knowledge graph" in any user-facing or vision-level context. They're the same thing described from different angles.

---

## 5. Graduation Language

> "The workshop is not the storefront."

This is good but not complete. It captures one aspect — "don't show unfinished work" — but the graduation model is richer than that.

The metaphor works for the private → public transition. But it doesn't capture:
- **Selective sharing** (showing your workshop to a trusted friend)
- **The spectrum** (L1 promotes to L2 by adding triples — things can mature in place)
- **The idea lifecycle** (captured → developing → parked → merged is more like a garden than a workshop)

**Recommendation**: Keep "the workshop is not the storefront" for the private/public boundary. But add a second metaphor for the overall system:

> "Content grows in the garden before it moves to the market."

This captures: things need tending (curation), they mature at their own pace (lifecycle), some things stay in the garden forever (not everything graduates), and the garden is where the real work happens (the knowledge graph's value is in the private connections, not the public output).

Jeff is literally a gardener. The garden metaphor maps to his life. The ontology already has Garden, GardenBed, Plant. This isn't forced — it's native.

If that's too precious, a simpler option: standardize on "graduation" as the verb and "maturity" as the quality. Content matures. When it's mature enough, it graduates. This is neutral and precise.

---

## Summary of Recommendations

| # | Action | Priority |
|---|--------|----------|
| 1 | Add inline clarifications to 4 glossary entries (ACL, Aggregate Turtle, Named Graph, SHACL) | Low |
| 2 | Add Storefront, Curation, Ideas/Projects Lifecycle, Capture Channel to conceptual model | Medium |
| 3 | Add Source as explicit concept | Low |
| 4 | Add "Not a backup system" to What This System Is Not | Medium |
| 5 | Refine "Not a social network" phrasing | Low |
| 6 | Adopt "semantic memory layer" for team docs, "personal knowledge graph" for user-facing | Low |
| 7 | Keep "workshop/storefront" for private→public; consider "garden/market" for overall lifecycle | Low — discuss with Jeff |

Nothing here blocks any build work. Items 2 and 4 are the most important — they fill real conceptual gaps. The rest is polish.

Good work, Silas. These are solid foundational documents.

— Wren
