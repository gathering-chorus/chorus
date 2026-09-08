# Survey finding 02 — the matcher works, on the hard cases

**Card:** #4119 (survey) · **Date:** 2026-09-07

Finding 01 said the names can be lined up but not by string matching. This builds the
thing and runs it, so that claim is demonstrated rather than asserted.

`prohibited-check.py` resolves a name through GBIF, then walks the resulting key's
parent chain against `ma-prohibited-keys.json` (140 keys, resolved once, offline).

## Run against the cases that break naive matchers

```
PROHIBITED      Reynoutria japonica     listed directly   [listed as: Polygonum cuspidatum]
PROHIBITED      Euonymus alatus         listed directly
PROHIBITED      Cuscuta gronovii        matched an ancestor (genus Cuscuta)
PROHIBITED      Lycium ferocissimum     listed directly   [listed as: Lycium ferrocissimum]
not prohibited  Acer rubrum
not prohibited  Oenothera biennis
not prohibited  Cornus florida
```

Each of the four hits is a case a text comparison fails:

- **Japanese knotweed** identified under its current name against a list keyed on a
  synonym GBIF will not place at species rank.
- **Dodder** identified at species against a `Cuscuta spp.` genus row — only works by
  walking up the chain.
- **Lycium** matched despite the state's own misspelling of it.

And the three clears matter as much: this is the negative half. A checker that says
PROHIBITED to everything would pass the four above and fail here. *Oenothera biennis*
is the plant I tentatively identified in a client's garden this morning, and
*Cornus florida* the dogwood — both correctly clear.

## Verified, and it corrects nothing I said

Every plant I flagged in the Kittredge draft is genuinely on the list:

```
PROHIBITED   Acer platanoides       Norway maple
PROHIBITED   Rhamnus cathartica     common buckthorn
PROHIBITED   Frangula alnus         glossy buckthorn
PROHIBITED   Euonymus alatus        burning bush
```

I had called those out from a photograph and a hand-drawn list, so they needed
checking against the source rather than trusting my own memory of the list.

## What is still unanswered

The identification half. This takes a *name* and answers correctly; it has not yet been
handed a name produced by Pl@ntNet from a photograph. That needs an API key —
free tier, 500/day, one account. That is the open item on this card.
