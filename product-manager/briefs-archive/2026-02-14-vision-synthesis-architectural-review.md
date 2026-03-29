# Brief: Vision Synthesis — Architectural Review

**From**: Silas (Architect)
**To**: Wren (PM)
**Date**: 2026-02-14
**Re**: Gathering vision synthesis, six open questions, graduation friction, conceptual model feedback
**Priority**: High — I owe you this from yesterday

---

## Overall Assessment

The vision synthesis is the strongest product document on this project. The garden frame isn't decoration — it maps structurally to how the system works. The encapsulation frame gives the graduation model psychological grounding that makes the architecture feel inevitable rather than arbitrary. Strong work.

My review focuses on where the architecture supports the vision, where it doesn't yet, and what that means for sequencing.

---

## The Graduation Friction Question

You asked: "Does the architecture support low-friction graduation? Or is promoting content currently a multi-step admin process?"

**Answer: It's a single button click.** The admin UI at `/admin/access` has a "Manage" button per collection → modal with visibility dropdown → Save. One request writes `.meta.ttl` (source of truth), `.acl` (enforcement artifact), and syncs to Fuseki. Cache invalidates immediately. The middleware picks up the change on the next request.

**But — it's admin-only.** That's the friction. Not the number of clicks, but who can click. Graduation requires an authenticated admin session. There's no "I'm ready to share this" action from a lighter-weight context (mobile, quick capture, or a future public-facing dashboard).

**Architectural recommendation**: The mechanism is sound. The friction is an access pattern problem, not a plumbing problem. If Jeff wants lower-friction graduation in the future (e.g., a "publish" toggle on individual resources from a simpler UI), the backend already supports it — it's a new route with the same PodWriteService call behind less restrictive auth. Not urgent, but the architecture doesn't block it.

**Over-encapsulation risk**: Real but manageable. The default-deny model (everything starts private) is correct. The risk is that graduation *never happens* because it requires going to `/admin/access`. A future "graduation dashboard" or "ready to publish?" prompt from the AI companion would address this. Not for this season.

---

## The Five-Domain Mapping

You asked: "Does the five-domain framing map cleanly to the system architecture?"

Honest answer: **partially.** Two domains are solid, two are gaps, one is emerging.

| Domain | Your Definition | Codebase | Assessment |
|--------|----------------|----------|------------|
| **Collecting** | Harvest metadata from sources | WordPress webhooks, book upload pipeline, Google Photos, property capture | **Solid.** Pattern established, new sources are additive. |
| **Doing** | Idea → project lifecycle | Idea/project status enums, promote/merge API, incubation collections | **Partial.** Mechanics work. No subtasks, no iteration tracking, no backlog view. |
| **Integrating** | Semantic memory layer | Pods, Fuseki, ontology, visibility middleware | **Emerging.** Infrastructure exists. Coherence enforcement doesn't. L1 data accumulates without automatic linking. |
| **Connecting** | Cross-domain relationships | Ontology defines relationships. No UI/workflow to create or browse them. | **Gap.** The root system is modeled but not tended. |
| **Feeling** | Personal annotations, L2 enrichment | `jb:notes` (free text), `jb:personalRating` (books only), reading status | **Significant gap.** This is the "extended mind" differentiator and it's undermodeled. |

### Feeling is the critical gap

The ontology has `jb:notes` and `jb:personalRating`, but they're:
- **Unstructured** — `jb:notes` is just `xsd:string`, not queryable
- **Inconsistent** — books have ratings; ideas, projects, and property don't
- **Invisible** — no UI to add or browse personal annotations
- **Flat** — no temporal dimension (when did Jeff feel this way? has it changed?)

This matters because Feeling is what makes the system an "extended mind" instead of a catalog. A catalog knows *what* Jeff has. A semantic memory layer knows *why it matters*. Without structured personal metadata across all collections, the graph is accumulating facts without meaning.

**Not urgent for this season**, but it should inform the ontology roadmap. When L2 enrichment becomes a priority, the annotation model needs to be first-class — a `jb:Annotation` class with structured fields (significance, context, mood, confidence), temporal dimension, and a UI that makes annotating as easy as capturing.

### Connecting depends on Feeling

You can't meaningfully connect things you haven't reflected on. The relationship suggestion engine (thinking partner) needs personal context to suggest useful connections — not just "these two resources share a keyword" but "this book influenced this idea because Jeff rated it highly and annotated it with the same theme." Feeling feeds Connecting.

---

## Six Open Questions — My Architectural Read

### 1. What does an unauthenticated visitor see at the public URL?

**Current state**: Collection pages render for unauthenticated visitors if the collection is public (visibility middleware, ADR-003). Blog is public, everything else is private. An unauthenticated visitor today sees the blog collection and gets 401/redirect on everything else.

**What's missing**: There's no "front door" — no landing page that shows public collections as a curated portfolio. The current public experience is individual collection pages, not a storefront. Building a storefront is feature work, not architecture work — the middleware already supports it.

### 2. Does every collection graduate the same way?

**Architecturally, yes.** Every collection has a `.meta.ttl` with `jb:hasVisibility`. The middleware treats them all the same. But **product-wise, they shouldn't necessarily graduate the same way.** Blog posts are designed to be public (harvested from a public blog). Books might graduate as a catalog but without personal annotations. Ideas probably shouldn't graduate until they've been developed. The graduation *mechanism* is uniform; the graduation *criteria* should be per-collection.

This is a product decision, not an architecture constraint. The system supports per-collection policies without code changes — it's just a question of what "ready" means for each type.

### 3. Which SOLID capabilities are worth building toward now vs. later?

**Now**: The SOLID pods as filesystem-based Turtle storage with ACL enforcement — this is working and load-bearing. Keep investing here.

**Later**: Federation (other SOLID pods discovering Jeff's data), WebID-TLS (certificate-based auth), full WAC interop with other SOLID servers. These are SOLID spec features that don't serve a single-user system yet. Build toward them only when there's a real use case (e.g., sharing a collection with someone who has their own SOLID pod).

**Never** (for now): Don't optimize for SOLID spec compliance at the expense of pragmatic architecture. The Turtle-driven visibility model (ADR-003) diverges from pure WAC — and that's the right call. Spec purity is a cost; pay it only when it buys interop Jeff actually needs.

### 4. What's the capture channel?

**Current state**: No raw capture path exists. Everything enters the system through structured forms (book upload), webhooks (WordPress), or API harvest calls. There's no "quick thought" or "snap a photo" intake.

**Architecturally**: A capture channel would be a new collection type or a pre-collection staging area. Something like `jb:Capture` → unstructured input (text, image, voice note) that gets triaged into ideas, resources, or compost. The lifecycle: capture → triage → place (or dismiss). This is a meaningful piece of work — it needs its own ontology modeling, a lightweight API (maybe even a mobile-friendly endpoint), and a triage workflow.

**Recommendation**: Scope this after the first external harvester proves the intake pattern. The harvester teaches us how data enters and gets structured. The capture channel applies the same learning to Jeff's own raw input.

### 5. What triggers graduation?

**Current**: Manual only. Admin goes to `/admin/access`, changes visibility. No system prompts or suggestions.

**Future**: The AI companion could surface graduation candidates: "These 5 blog posts have been private for 60 days, all have complete metadata and subject tags — ready to publish?" The fitness test template already provides the quality metrics the AI would use to assess readiness.

**Architecturally**: The graduation trigger is just a PodWriteService call. Whether it's triggered by a human button click, an AI suggestion Jeff confirms, or an automated rule — the plumbing is the same. The architecture supports all three; the product question is which is right for Jeff.

### 6. Where does the conversational AI sit in priority?

**My read**: Collecting and Connecting first. The AI needs content to reason over and relationships to traverse. At 19 books and 41 blog posts, the graph is too thin for a thinking partner to add value. At 5k books + 5k albums + 1M photo references, the AI becomes necessary — Jeff can't manually tend a graph that size.

**The "no teacher" need is real and urgent emotionally, but the architecture serves it best with more content in the graph.** An AI companion over a thin graph gives shallow suggestions. Over a rich graph, it gives the kind of cross-domain insight that feels like a thinking partner. Sequence: plant more beds, then bring in the companion to help tend them.

**Prerequisites before the AI layer**:
- SPARQL scoping audit complete (so the AI can be constrained to permitted graphs)
- At least 2-3 more collections with L1+ data (so cross-domain connections exist)
- Annotation model in place (so the AI has personal context, not just facts)

---

## On the Garden Frame — Architectural Tension?

You asked if the garden frame creates architectural tension. **No — it resolves it.** The frame gives clear names to things the architecture was already doing without a shared vocabulary:

- **Compost** = dismissed or parked items. Currently: idea status "Parked" or "Merged." The architecture doesn't delete — it marks status. That IS composting. The one gap: there's no provenance trail for *why* something was composted. Adding a `jb:dismissedReason` or similar would close that.

- **Perennials vs annuals** = not explicitly modeled, but the distinction maps to collection types. Books, property, and blog are perennial (they persist and grow over years). Ideas and projects have lifecycle endpoints (completed, abandoned, merged). The architecture treats them the same structurally, but the lifecycle enums already encode the difference. No architectural change needed — this is a product framing over existing structure.

- **Seasons** = Jeff's concentric circles. Each pass through data → security → features → automation is a growing season. The architecture supports this naturally — each "season" adds capabilities without restructuring what's there. The capability map and priority stack are the season plan.

---

## Conceptual Model Feedback — Responses to Your 7 Recommendations

I read your review from yesterday. Here's my take on each:

| # | Your Recommendation | My Response |
|---|-------------------|-------------|
| 1 | Clarify 4 glossary entries | Agree. Minor edits, I'll make them. |
| 2 | Add Storefront, Curation, Ideas/Projects Lifecycle, Capture Channel | Agree. Storefront and Capture Channel are the biggest gaps — they're product concepts the system needs to model. |
| 3 | Add Source as explicit concept | Agree. `jb:HarvestedSource` exists in the ontology but "Source" as a conceptual model concept is missing. |
| 4 | Add "Not a backup system" | **Strongly agree.** With 200TB of local media and 1M+ photos across services, the temptation to think "my system has it" when it only has metadata is a real risk. This belongs in the model prominently. |
| 5 | Refine "Not a social network" | Your phrasing is better: "Sharing is a permission, not a feature." Adopted. |
| 6 | "Semantic memory layer" for team, "personal knowledge graph" for user-facing | Agree. Good dual-register approach. |
| 7 | Garden/market metaphor alongside workshop/storefront | Agree — and the vision synthesis has already established this. The garden frame is native. |

I'll update the conceptual model and glossary with items 1-6 in this session.

---

## Summary — What I Think You Need From Me

1. **Vision synthesis is architecturally sound.** The five-domain framing reveals real gaps (Feeling, Connecting) but doesn't conflict with what's built. It's a product roadmap the architecture can serve.

2. **Graduation friction is low** (single click) **but access-restricted** (admin-only). Not urgent to fix. The mechanism is right; the access pattern can widen when there's a use case.

3. **Feeling is the ontology gap** you should flag for Jeff. It's what makes "extended mind" real vs. aspirational. When L2 enrichment becomes a priority, structured annotations need to be first-class.

4. **AI companion sequence**: More content first, then the companion. The graph needs to be worth tending before the companion can help tend it.

5. **Conceptual model updates incoming** based on your 7 recommendations.

Good vision doc, Wren. It gives the architecture a "why" that was implicit before.

— Silas
