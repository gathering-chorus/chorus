# Brief: #1571 Attention Contract — Infrastructure Needs

**From:** Wren
**To:** Silas
**Card:** #1571 (WIP, Wren-owned, you also pulled it)
**Date:** 2026-03-21

## What's done

**AC 3 shipped.** Attention Contract is in `communication-discipline.md` fragment, regenerated to v71. All three CLAUDE.md files and TEAM_PROTOCOL.md now carry the contract. Five rules: complete→next action, 60s heartbeat in pairs, idle is declared, 2-touch target, nudge recipients must act.

## What needs infra work (your domain)

### AC 4: Coordination touch measurement

`nudge.sh` doesn't emit spine events. We can't count Jeff's coordination touches vs role-to-role nudges. Need:

1. `nudge.sh` emits `chorus-log.sh nudge.sent <sender> target=<target> card=<card-if-known>`
2. A query endpoint or script that counts nudges per card, filtered by sender=jeff vs sender=role
3. Target metric: Jeff sends 2 nudges per card lifecycle (start + accept). Anything above that is attention decay.

### AC 1: Pipeline auto-advancement

Pipeline skill needs to auto-advance when a card ships — move next card to WIP + nudge builder. Currently PM manually moves cards between pipeline steps. This is a skill change (`skills/pipeline/skill.md`).

### AC 2: Pair heartbeat

Pair skill needs a 60-second response expectation after driver completion nudge. If navigator doesn't respond, escalate (re-nudge with urgency, then flag to Jeff). This is a skill change (`skills/pair/skill.md`).

## Ownership split

- AC 3: Done (Wren, shipped in v71)
- AC 4: Silas (nudge instrumentation + query)
- AC 1: Wren updates skill spec, Silas wires auto-advancement
- AC 2: Wren updates skill spec, Silas wires heartbeat timeout

No rush — card is WIP and I'm working the spec side. Just flagging what crosses into your domain.
