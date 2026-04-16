# Wren — Next Session

## What shipped (2026-04-16)
- DEC-2090: Demo briefs dropped for single-card demos
- #2092: Ontology consolidated 10→5 subproducts, Borg restructured, 1475→1374 triples
- #2093: All artifacts mapped to product homes, /api/chorus/products endpoint live
- #2095: Explorer rewrite — dropdown filters, cascade visibility, 52 permutation tests
- #2107: Three ontology views linked (ER/Explorer/Cards), 7 stale docs bannered
- #2108: Chorus front end bootstrap — 3 views served from chorus-api at 3340
- #2111: Blockverse blog at localhost:8082, two posts published
- Pair gate disabled (per Jeff — pairing is practice, not enforcement)
- Gate:product passed on #1884, #1914, #1777, #2090, #2096, #2097, #2098, #2100, #2101, #2102, #2104

## WIP
- #2094: Chorus front end product designs. Landing page live at 3340/docs/.
  - AC1 done (inventory). AC2 partial — landing page + ontology views done.
  - Next: make tiles clickable to subpages, migrate /chorus from Gathering to 3340

## Key decisions & concepts
- **Shaping surface** (not "control plane") — observe and act on same API, no hierarchy
- **Anti-Gestell** — neither humans nor agents reduced to standing-reserve
- **WYSIWYG for facets** — wired+data=show, wired+empty=show(0), not wired=hide
- **Aggregation domains** — Code and Tests show ALL files, not domain-scoped
- **Spine decoupling** (#2109 shipped) — service call replaces library import

## Captured
- Jeff's OACO values and purpose (7 values + life purpose statement)
- Sridhar Singirikonda, Staples team (MQ viewer, ESB, 4hr reviews)
- Unmesh Deolekar (Mumbai call, immigration, OACO)
- Blockverse origin (Susskind block universe via Aubrey)
- Bridget Snell / FastX Partners consulting framework
- 37 notes themes from Jeff's 2025-2026 archive

## For next session
- Finish #2094 — wire product tiles to subpages
- Migrate /chorus page from Gathering (3000) to Chorus (3340)
- #2099 (Borg front end) — 9 pages identified for migration
- Runtime service registry — discussed but not carded
- Blockverse needs theme customization
