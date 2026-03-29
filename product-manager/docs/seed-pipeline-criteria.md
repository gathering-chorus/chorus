# Seed Pipeline — Promotion Criteria

## Overview

Seeds are a **triage inbox** — raw captures from SMS, web, email that fan out to destinations across the app. This document defines when and why items move between stages.

## Stage 1: Seed (Sowing spoke)

**Entry:** Automatic — SMS via Twilio, web capture, manual entry
**Statuses:** pending → routed | discarded

### Triage Decision: Where does this seed go?

| Destination | Spoke | When to route here | Example |
|---|---|---|---|
| **Glimmers** | Growing | A fleeting impression worth holding — not yet an idea, but resonant. Emotional or aesthetic signal. | "That feeling when the garden light hits at 6pm" |
| **Ideas** | Growing | A concrete concept with enough shape to name — skip Glimmer when it's already actionable. | "Build a seasonal light coverage map" |
| **Projects** | Growing | Already scoped and committed — rare from seed, usually graduates from Idea. | "Drone survey pipeline for spring garden" |
| **Reading** | Practicing | A book, article, or resource to consume. | "Thinking in Systems by Donella Meadows" |
| **Watching** | Practicing | A show, film, video, or series to watch. | "Severance season 2" |
| **Cooking** | Practicing | A recipe or meal idea to try. | "Thai basil chicken from the Woks of Life" |
| **Todo** | Practicing | An actionable task — not an idea, just something to do. | "Call dentist" |
| **Garden Beds** | Property | Something about the physical garden — planting, observation, plan. | "Move the hostas to the north bed" |
| **Rooms** | Property | Something about the house — maintenance, improvement, observation. | "Fix the bathroom faucet leak" |
| **Team Brief** | Chorus | Direction for a role — routes to role's brief inbox. | "Silas: check disk usage on Bedroom Mac" |

### Discard criteria
- Duplicate of existing item
- No longer relevant (time-sensitive and expired)
- Too vague to route anywhere — and Jeff doesn't care to refine it

## Stage 2: Glimmer (Growing spoke)

**Entry:** Routed from Seed, or created directly on /glimmers
**Statuses:** glowing → ignited | faded

### Ignite → Idea (promotion)
- The glimmer has **crystallized into something nameable** — you can describe what it is, not just how it feels
- There's a possible action or outcome, even if vague
- Jeff keeps returning to it — recurrence is signal

### Fade (demotion)
- The resonance faded — it was a moment, not a thread
- Superseded by a different glimmer or idea
- Can be reignited later if it comes back

**Lineage:** `ignitedTo` (on glimmer) → points to created Idea. `sparkedFrom` (on idea) → points back to glimmer.

## Stage 3: Idea (Growing spoke)

**Entry:** Ignited from Glimmer, routed from Seed, or created on /ideas or /incubation
**Statuses:** captured → developing → parked | merged

### Promote → Project
- The idea has **scope** — you can describe what done looks like
- Jeff is ready to **commit time** to it (not just think about it)
- There's a clear first step

### Park (defer)
- Interesting but not now — no energy or priority
- Waiting on an external dependency

### Merge (consolidate)
- This idea is really part of another idea
- Combining strengthens both

**Lineage:** `promotedTo` (on idea) → points to created Project. `promotedFrom` (on project) → points back to idea.

## Stage 4: Project (Growing spoke)

**Entry:** Promoted from Idea
**Statuses:** active → paused | completed | abandoned

Terminal stage — no further promotion. Projects complete or get abandoned.

**Lineage:** `promotedFrom` → points back to originating Idea.

## Gap: Practicing-spoke lineage

Currently, when a seed routes to Reading, Watching, Cooking, Garden Beds, or Rooms, **no lineage is recorded**. The destination item doesn't know it came from a seed. This means:
- Can't trace "where did this reading list item come from?"
- Can't measure seed → action conversion rate
- The pipeline view can't show flow volume to Practicing destinations

**Recommendation:** Add `routedFrom` property on destination items when routed from a seed.
