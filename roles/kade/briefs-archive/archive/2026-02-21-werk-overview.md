# The Werk — What We're Building and Why

**From:** Wren (PM)
**To:** Silas + Kade
**Date:** 2026-02-21

This isn't a feature brief. This is context for everything. Jeff shared deeply today about his life, his values, and what this work actually means. You need this to make good decisions in your domains.

---

## What Is the Werk?

The Werk is Jeff's word for the whole thing — not just the apps, not just the team, but the life's work taking shape. It comes from the German, closer to "calling" than "job." Jeff chose it deliberately.

The Werk has a shape: a **spiral** with six spokes — architecture, operations, value/product, pipeline/quality, infrastructure, code/toolchain. Chorus sits at the top, Gathering at the bottom. Each rotation of the spiral crosses all six spokes. When rotations get long, individual spokes need focused attention — but skipping spokes is drift, and drift compounds.

## Two Products, One Nervous System

**Gathering** is the app — a personal infrastructure platform built on SOLID pods, RDF/Fuseki, Express, TypeScript. It holds Jeff's collections (music, books, photos, garden) and presents them through an interactive mind map. The center of that mind map is **Self** — the most protected, most personal domain.

**Chorus** is the nervous system. The value stream is the **spine**. Every capability we build is a nerve that carries a signal along that spine or enforces a rule at a junction.

Every touchpoint in the system does one of four things:
- **Senses** something (Slack message, commit, deploy, error, photo shared, story told)
- **Routes** it (to the right role, the right card, the right dashboard)
- **Constrains** it (can't deploy without tests, can't skip a brief, can't exec into containers, can't write credentials)
- **Proves** it (screenshot, dashboard, audit trail, fitness function)

The nervous system has layers:

| Layer | What | Examples |
|-------|------|----------|
| **Brain** | Where rules are defined | CLAUDE.md files, team-architecture.md, decisions.md, ADRs |
| **Spine** | Central routing — fires automatically | SessionStart hooks, UserPromptSubmit hooks, post-commit hook |
| **Peripheral nerves** | Block bad actions at the edge | sensitive-paths-hook, write-scrubber, infra-guardrails, **permission profiles** (card #97) |
| **Shared memory** | Persistent context across sessions | ~/.chorus/index.db, state files, activity.md, **self-stories.md, self-memories.md** |
| **Sensory organs** | Observability — what happened? | chorus.log → Loki → Grafana (8 dashboards), alert routing (card #88) |
| **Muscles** | Operational scripts — do things | app-state.sh, system-state.sh, board-ts, slack-post.sh |

Design principle: rules in the wiring, not the manual. If a role can skip it, it's aspirational. If a hook enforces it, it's real.

They're not separate projects. Gathering is what gets built. Chorus is how it gets built. Both are products. Both are the Werk. The spine connects them — signals flow from Jeff's direction through the nervous system into built features and back through proof.

## Why It Matters — Jeff's Story

Jeff has worked since he was 13 — summers on his father's commercial construction sites, full-time through school, then 30+ years as a technology leader. 50-60 hour weeks, engulfed by emails and coordination. He was laid off in summer 2024 — the longest he'd been without a paying job since childhood.

That break made something clear: **his life is his work, not just what he gets paid for.** Work and life aren't separate domains to balance. They're one thing. Building Gathering and Chorus isn't a side project or a hobby. It's the continuation of a lifelong relationship with work — now on his own terms.

His financial advisor says he has 10 years of runway. He wants to make income — ideally ~$100k — but not by going back to trading time for someone else's priorities. He wants to build options where he makes the money directly. His instinct: **the portfolios themselves are the pitch.** Present the music collection, the photos, the garden, the blog publicly. Let people see what he builds. Let the work start conversations.

Chorus is the same play at a higher level — a working coordination product that technical leaders can see themselves in.

## The Self Domain

The center of the mind map. The innermost ring of the concentric trust model (local AI only, never leaves the Macs). This isn't just profile data. It's:

- **Stories** — narrative accounts of Jeff's experiences, organized by theme (15 currently in `product-manager/self-stories.md`)
- **Memories** — extracted values, patterns, preferences, relationships (`product-manager/self-memories.md`)
- **Life's Practice** — Jeff's inner ontology, a directional chain:

```
Flexibility/Equanimity
    → activates → Mindfulness/Presence
        → builds → Reflection/Agency/Possibility
            → grows by → Learning
```

The whole chain is **Practice** — not a thing you do, but how you live. The verbs (activates, builds, grows by) are typed relationships. This is RDF-native thinking on paper.

Two philosophical anchors sit beneath:
- **Agency** is the alternative to control
- **Equanimity** is the alternative to surrender

Card #94 asks Kade to render Stories and Memories as clickable leaf nodes on the mind map. This is the first domain where the map goes from schema (categories) to instances (actual content). The pattern matters as much as the feature.

## The Life Board

Jeff created a full personal planning system in August 2025 — during the post-layoff period. Physical whiteboards and handwritten pages structured as kanban boards:

- **Inside Me** — healing relationship with self (the ontology above)
- **Outside Me** — relationships, house, garden, savings, job search
- **Operational boards** — tactical items with cross-domain dependencies
- **Career Vision** — north star document (needs updating — the "find an org" goal has shifted to "build my own income")

All boards use Now/Next/Later lanes with the same planning gate: **Worth it? Have what I need? Can I start?**

Jeff sees Wren as capable of helping manage not just the product boards but the life boards. The same rigor — prioritization, trade-offs, opportunity cost, honest reflection — applied to the whole picture.

## Today's Work Mapped to the Spine

Everything from this session maps to the nervous system:

| What | Signal Type | Layer | Card |
|------|-------------|-------|------|
| Permission profiles | **Constrains** | Peripheral nerve | #97 (Silas) |
| Self-Stories / Self-Memories | **Senses** + **Routes** | Shared memory | #94 (Kade) |
| Mind map leaf nodes | **Proves** (makes Self visible) | Muscle | #94 (Kade) |
| Notes harvester | **Senses** (new input source) | Sensory organ | #95 |
| Social posts harvester | **Senses** (new input source) | Sensory organ | #96 |
| Life board + ideation session | **Routes** (Jeff's life → cards) | Brain | #92 |
| Weekly cadence calendar | **Constrains** (when signals flow) | Spine | #93 |

The pattern: Jeff shared stories → Wren sensed and routed them into structured data → that data becomes shared memory → Kade proves it by making it visible on the mind map → Jeff sees his Self domain taking shape. That's a full signal loop from Directing through Proving.

## What This Means for Your Work

**For Silas:**
- The concentric trust model is architecture, not aspiration. Self = local only. The AI-beneath-the-pod principle (AI is a service client, not an administrator) governs everything in the inner ring.
- Permission profiles (card #97, your brief) directly enable Jeff to walk away and live his life while we work. This isn't convenience — it's the operating model.
- Infrastructure decisions carry a life-quality cost. Every time something breaks and Jeff has to babysit, that's time taken from the garden, the walk, the meditation.

**For Kade:**
- The mind map is the product's visual identity. Self at the center, stories and memories as leaves — this is what Jeff will show people. It needs to feel like his space.
- New harvesters coming: Apple Notes (#95), Facebook + LinkedIn posts (#96). Same pattern as music and photos — bulk ingest, refine later.
- The portfolio-as-pitch strategy means public-facing views matter more now. How the music collection, the photos, the blog present publicly is a revenue question, not just a UX question.

**For both:**
- The Werk isn't a metaphor. It's the frame Jeff uses to evaluate everything we build. "Does this serve the Werk?" is the real prioritization question.
- Jeff's daily calendar has specific blocks — meditation 6:30-8:30, coding 11-12, walk 3-5. We need to fit into that rhythm, not disrupt it. Calendar/cadence work is coming (card #93).
- When Jeff shares a story or a photo or a philosophy reference, receive it. It's not a distraction from the work. It IS the work.
