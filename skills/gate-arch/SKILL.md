---
name: gate-arch
description: Architecture review of a card's diff — one path, retire in the same land, boundaries, duplication, generated files. Runs on every card, fix cards included. Never reads AC boxes.
user-invocable: true
---

# /gate-arch — Architecture review

Jeff 2026-11-04 18:11: "review is supposed to be code review not ac checker and same
for quality and architecture". This gate reviews the **diff**, the way an architect
reads a change. It runs on **every** card, fix cards included (#4889: a fix is where a
second path most often slips in). It never reads, counts or judges AC boxes: the AC
are proven by their own checks in prod, not here.

## Arguments

```
/gate-arch <card-id>
```

## Input

The card's branch diff (`origin/main...HEAD` in the card's werk), and the card text
for context only. In the demo the diff is handed to you; by hand, read it with
`git -C <werk> diff origin/main...HEAD`.

## What to look for in the diff

1. **One path.** Does the change add a second way to do something that already has
   one (a second publisher, a second copy of a hash or prompt, a fallback path, a new
   script beside an existing verb)? Name both paths, file:line.
2. **Retire in the same land.** If it replaces something, is the old thing gone in
   this diff, or does it now run beside the new one?
3. **Boundaries.** Does it reach into another domain's internals (its files, store or
   tables) instead of its door? Does a domain now depend on one it should not?
4. **Duplication.** Copied logic, copied constants, a second source of truth for one
   fact.
5. **Generated vs hand-edited.** Is a generated file (pipelines, models) edited by
   hand, or changed without its generator or rows?
6. **Names and IRIs.** Non-canonical namespaces; a new name for a thing that has one.
7. **ICD consistency.** A change to a domain's data shape or harvester (`*.ttl`,
   `*-harvester.service.ts`, `harvest-*.sh`, an API door's fields) has a matching ICD
   provider section and SHACL shape in the same diff; a new domain is mapped in the
   graph. Name the file that changed shape without its contract.

## Where the old checks went (#4889, Kade 18:34: fix, never throw away)

| Old check | Now |
|-----------|-----|
| Namespace conventions (grep of changed files) | item 6, on the diff |
| ICD consistency (domain query) | item 7, on the diff; `icd-gate-hook` still refuses harvester writes with no ICD |
| Domain boundary scan (directory count) | item 3, read as real boundaries, not a count |
| "Does it fit the architecture?" | the whole review |

Report only what you can point to in the diff: each finding is
`file:line — what — why it matters`. No finding about the card's AC boxes, ticks or
wording.

## Verdict

- **fail**: at least one finding in 1–5 or 7 that ships a second path, a boundary break, a missing contract or
  a hand-edited generated file.
- **pass**: otherwise; list minor findings as notes.

A diff you could not read is an **error**, never a pass.
