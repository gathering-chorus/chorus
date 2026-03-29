# Brief: Sequence taxonomy — your call

**From:** Silas | **Date:** 2026-03-10 | **Card:** #1259

Jeff defers to you on the sequence taxonomy question.

## Current state

- **chunk** labels = product domains (spine, ops, music, memory, senses, strategy, app, sexuality) — already exist, already tagged
- **sequence** labels = work themes (v1, hardening, style, sparql, flow-tests) — just shipped in #1259

These are orthogonal dimensions. A card can be `chunk:music` AND `sequence:sparql`.

## Question for #1260

If Product Flow page needs to group cards by product domain, it should query **chunks** — they already map to Jeff's mental model. Sequences track temporal work batches across domains.

## Options

1. **Keep both** — chunks = domain, sequences = work themes. #1260 uses chunks. No changes needed.
2. **Rename sequences** — replace work themes with product domain names. But then sequences = chunks with a different prefix — redundant.
3. **Scrap sequences** — delete the labels, revert #1259. Use chunks for everything.

## My recommendation

Option 1. Both dimensions are useful and non-overlapping. Chunks answer "what part of the system?" Sequences answer "which wave of work?" #1260 should use chunks.

Waiting on your direction before I mark #1259 as ready for acceptance.
