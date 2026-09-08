# Survey finding 01 — can an identification be matched to the Massachusetts list?

**Card:** #4119 (survey) · **Date:** 2026-09-07 · **Answer: yes, but not by string
matching, and the state's list has errors in it.**

## What was done

The MDAR Prohibited Plant List was fetched as published (142 rows, common name +
scientific name), and every scientific name was resolved through GBIF's
`/species/match`, which is the backbone Pl@ntNet and Plant.id both key into
(Pl@ntNet returns a `gbif` and a `powo` id per result; Plant.id returns `gbif_id`).

No API key was needed for this half. The identification half of the probe still needs
a Pl@ntNet key.

## Result

```
142 rows resolved
  EXACT        128
  HIGHERRANK    11    genus-level rows (Cuscuta spp., Striga spp. …) — correct behaviour
  FUZZY          2
  NONE           1    a page header my parser picked up, not a plant
```

## The three findings

**1. The published list contains two misspellings.** GBIF's fuzzy match caught both:

```
Lycium ferrocissimum      -> Lycium ferocissimum
Pennisetum polystachyon   -> Pennisetum polystachion
```

A naive exact string match would silently fail to flag either plant. This is the
argument against ever matching binomials as text.

**2. Two rows resolve only to FAMILY, and one of them matters a lot.**

```
Polygonum cuspidatum  -> Polygonaceae (FAMILY)   common name: Japanese knotweed
Salsola vermiculata   -> Amaranthaceae (FAMILY)
```

Japanese knotweed is the most consequential plant on this list and its listed name is
a synonym GBIF will not place at species rank. The current accepted name resolves
cleanly:

```
Reynoutria japonica -> EXACT, SPECIES, usageKey 2889173
```

So the list is keyed on an outdated name. Any matcher must carry a synonym table or
it will fail on exactly the plant a Massachusetts gardener most needs flagged.

**3. Genus rows need the match to walk UP the tree.** Eleven `spp.` rows are genus
records. An identification returning *Cuscuta gronovii* must match the `Cuscuta`
row — which only works if the resolved key's parent chain is stored, not just the key.
The list header also bans "all cultivars, varieties and hybrids", so the walk goes
down as well as up.

## What this means for the product

Resolve the whole list once, offline, into GBIF usage keys plus each key's parent
chain, and store that. At field time, match the identification's returned key against
the stored set by walking the chain. Never compare names as strings.

## Files

- `ma-prohibited-rows.tsv` — the list as published, parsed
- `ma-prohibited-gbif-match.json` — every row with its matchType, rank, usageKey and
  accepted name, so this finding can be re-checked rather than believed
