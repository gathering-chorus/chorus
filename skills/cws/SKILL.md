---
name: cws
description: chorus-werk-status — where is each pipeline, one answer for Jeff and roles alike. Reads run pins with the #3781 trust rules (stale pins report abandoned, never running) and flags stale-present rounds before a go can land the wrong witness.
user-invocable: true
---

# /cws — Chorus Werk Status (#3782)

Jeff (or a role) types `/cws` and gets pipeline status: which runs are live,
what stage each is in, how long it's been there, and — at the presented stop —
whether the round matches the werk's HEAD plus the exact go command.

Born from Jeff, 2026-08-06: "a skill like /cws can be used by both me and u to
see where we r in a pipeline" — and, one hour later, mid-wait: "this is where
i need cws".

## Argument

```
CARD_ID=<optional card number>      /cws 3781
--role <role>                       /cws --role wren
(no arg)                            all LIVE runs, all roles — Jeff's view
```

## Step 1: Run the script

```bash
python3 /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-werk-status [card] [--role <role>]
```

## Step 2: Paste the output

**Focus mode rule: the returned text IS the answer — paste it verbatim.**
One line per run. No re-narration, no summary on top.

## What the lines mean

- `running · 9m in · stage: demo` — live (pid verified against the pin's own
  startedAt, the #3781 identity rule; a dead or reused pid reports
  **abandoned (pin stale)**, never running)
- `presented · round matches HEAD ✓ · go: …` — waiting on Jeff, with the go
  command verbatim
- `presented · ⚠ STALE-PRESENT` — a commit landed after the present; a go now
  would land a witness missing content. Supersede + re-present first.
- No-arg view shows LIVE runs only — the ~190 landed/stale pins on record are
  history, not status.

## Hard rules

- The script is the contract — no hand-reading pin files, no `python3 -c`
  one-liners against `~/.chorus/werk-runs` (the exact toil this skill retires).
- `--fixture` is the self-test (negative proof): it must print
  `NEGATIVE PROOF OK` — a cws that cannot distinguish a stale pin from a live
  run must not be trusted, per the two-states discipline (#3734).
- chorus-athena inherits this surface: when its pipeline lands (same pin
  contract — phase/stage/startedAt/pid/patchId, per the #3782 scope note),
  /cws renders its runs identically. One status tool, both pipelines.
