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

The no-arg view is one line per WIP card (#4494): where it is now, for how
long, and which run of the card this is.

```
#4235 kade   land: waiting in line (1st)        6m · run 6
#4493 wren   land: deploy-canonical             7m · run 2
#4494 kade   failed at deploy-canonical    (22:24) · run 2
```

- `build` / `test` / `demo` … — a run is at that step, for that long
- `waiting for your go` — the demo is up; the go command follows on the line
- `land: waiting in line (2nd)` — the go is in; lands go one at a time
- `land: <step>` — the land is running that step
- `failed at <step> (HH:MM)` — the run stopped there, at that time
- `run N` — the card's own run count, from dagu's run list
- A card with only old-pipeline runs keeps its old line (pid verified against
  the pin's own startedAt, the #3781 identity rule; a dead or reused pid reports
  **abandoned (pin stale)**, never running).

`/cws <card>` opens with the same line, then every run in a table.

## Hard rules

- The script is the contract — no hand-reading pin files, no `python3 -c`
  one-liners against `~/.chorus/werk-runs` (the exact toil this skill retires).
- `--fixture` is the self-test (negative proof): it must print
  `NEGATIVE PROOF OK` — a cws that cannot distinguish a stale pin from a live
  run must not be trusted, per the two-states discipline (#3734).
- chorus-athena inherits this surface: when its pipeline lands (same pin
  contract — phase/stage/startedAt/pid/patchId, per the #3782 scope note),
  /cws renders its runs identically. One status tool, both pipelines.
