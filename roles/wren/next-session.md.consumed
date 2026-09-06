# Next session — Wren

**Written 2026-08-03 16:37 Boston.** Long session (~7h). Read this before doing anything.

---

## THE ONE THING TO CHECK FIRST

**#3724 is BUILT but NOT ON MAIN.** The land stalled twice today. Do not report it as done without checking:

```
git log --oneline origin/main -3        # look for a #3724 merge
gh pr list --head wren/3724             # empty means it never merged
```

A relaunch was in flight at close (`3724-wren-1785789032644-24`). **The pipeline said `running` for 17 minutes with no process behind it, and refused a retry with `go-while-running` — the stall protects itself.** Recovery that worked twice: `mv ~/.chorus/werk-runs/3724.json ~/.chorus/werk-runs/3724.json.stalled-$(date +%s)` then re-invoke `chorus_werk`. If it stalls a third time it is a real defect in the land verb and it is Silas's substrate, not something to keep working around.

Jeff's exact words when I mislabelled this: *"how can u 'land' what u did not do?"* — say **built** / **on main**, never a phase word.

---

## WIP

**#3724 — path-routed domain navigation.** Built, demoed, Jeff said go. 6 commits on `wren/3724`. Serves `/domains`, `/domains/<domain>`, `/domains/<domain>/<instance>` from one template for all 40 domains; `domain.html?d=` 301s. Also carries two things Jeff asked for while looking at it: **the raw OWL per domain** (`/api/athena/domain-owl/<d>`, real turtle) and **the model as a table schema** (`/api/athena/domain-schema/<d>`). Scope was cut mid-card — products moved out, see below.

**#3718 — athena-model.** Rename done, ADR refusal core done, **TBox verbs `class`/`property`/`shape` wired and demonstrated live**, 53 tests green, 2 commits. **Remaining: AC4 — arm the commit-time gate that refuses a hand-edited `.ttl`.** That is the half that makes the rest not-theatre. The gate script exists (`platform/scripts/athena-model-gate.sh`) and reports but is not armed.

Correction that matters: the "21 served classes unclaimed" blocker was **inverted** — owl-api builds its serve list FROM `definesVocabulary` (lib.rs:863), so served-but-unclaimed is 0 by construction. The gate's parked reason does not exist; it can arm.

**#3739 — model ERD.** Pulled, not started, **and it duplicates #3585** ("owl-api generate-classdiagram") which has sat at rank 118 in my own `coherent-model` chunk the whole time. I filed it without reading the walk. **Fold #3739 into #3585 rather than keeping both** — the exact add-without-retiring behaviour we spent the day naming.

---

## JEFF'S RULINGS TODAY — these change the model, not just a page

**Products are a PEER of domains, not a domain's contents.** `/valuestreams`, `/products`, `/domains`, `/services` are **all top level**. A Product *composes* Domains (`athena hasDomain value-streams, domains, services, products`). He said "peer not subordinate" on 08-02 and I built `/domains/products` anyway; he had to say it four more ways. `/domains/products` was removed from #3724 rather than left half-working.

**A class diagram is the right mental model** for seeing the model — boxes with attributes, typed lines. Not WebVOWL (which we do not have: `INFRASTRUCTURE.md` lists it on :8089, nothing is listening).

**He wants to see the OWL itself per domain**, then asked for it *as a table schema*. Both shipped in #3724.

---

## THE FINDING THAT MATTERS MOST

The schema table exposed, without anyone auditing: **across 6 domains, 54 property facts are declared in the SHAPE only, 44 in the ONTOLOGY only, 31 in both.** On `chorus:Domain`, 16 required properties and **3 declared in both**. That is the coherence question Jeff keeps returning to, now visible on a page instead of found by hand.

Live numbers (2026-08-03): **117 classes declared · 44 claimed by a domain · 21 served.** 16 of 40 domains declare no vocabulary. 23 classes claimed with no shape.

---

## MODEL WRITES ARE WORKING AGAIN

The DAL was fail-closed for a week — **21,764 `identity-token-required` refusals since 2026-07-27 11:00**, all three roles, zero alerts. Root cause was NOT a CSS outage (I reported that wrongly; my 500 was a headerless call outside its identifier space). It was #3687's fail-closed flip plus a minter with no retry.

**Recovery if it recurs:** `rm ~/.chorus/identity/wren/token.cache` then `export CHORUS_IDENTITY_TOKEN="$(chorus-identity-token wren)"` with `platform/scripts` on PATH. Verified — landed `set: chorus#athena label=Athena`. Note `--kind product` lowercase; `Product` is refused.

---

## OPEN WITH PEERS

**Silas** — filed #3741/#3742 (episode-keying, numeric `duration_ms`, `op=unknown` attribution). Agreed split: he owns counting/attribution, I own the freshness recompute (**#3740**, filed, P1). He landed #3728. Also owns #3735 (competing `chorus:security` definitions — I blessed him deleting my stale block from `domains-wren-silas.ttl:179-185`), #3732, #3736.

**Kade** — #3734. He caught that two of my four nightly-coverage findings targeted **dead code**: `nightly-coverage.sh` is orphaned; the live path is `run_coverage()` in `nightly-suites.sh`. Agreed: **retire the orphan, don't fix it.** His rule, adopted: **a check that gates anything must ship with a negative proof — a fixture where the guarded condition is violated and the check is shown to FAIL.**

---

## WHAT I KEEP GETTING WRONG (it repeated four times today)

I built the *two-states-look-identical* defect into code written to expose it — **four separate times**:

1. `reconcile.rs` fabricated a whole partition when the store was unreachable (scored every class 0).
2. `is_fixture_graph` silently defaulted unrecognized graphs to "live" — the typo'd `urn:chorus:domain:tests` counted as model content.
3. The domains page reported "store unreachable" when the real cause was **a missing endpoint on an older build** — the page is served from disk and updates instantly; the API behind it needs a deploy.
4. I read a stale approval sidecar as proof a card had not filed. It had.

**And I stated hypotheses as measurements twice** — "provenance is being stripped" (the delete log shows only test fixtures) and "3→51/day worsening" (the counts are a sawtooth; the *tail* is the finding: 14.5s → 20.6s → 24.9s → **75.8s**).

Jeff on the pattern: *"u are like a person in the dark who refuses to use the lamp in their hand."* The lamp is `~/.chorus/chorus.log`. Every one of these was already written down.

**Also: censored data.** The event-loop probe times out at 8000ms, so its durations pile at exactly 8000 — five days showed a "median" that was the cap talking. **Split by detector before trusting any duration.** And 44 of 257 events are the same block counted twice.

---

## NOT DONE

- #3724 on main (above)
- #3718 AC4 — arm the gate
- Fold #3739 → #3585
- The Clearing asks for your name *after* you authenticate (`directing/clearing/src/server.ts:426`) — a real defect Jeff raised, no card yet
