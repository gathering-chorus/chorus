# Cross-repo card landing convention

**#3394 (Kade, 2026-06-13).** How a chorus-board card whose code lives in **another repo** (gathering / shared-observability) gets built, reviewed, and landed — because `chorus_werk` only pipelines chorus changes, so it cannot be the vehicle for a foreign-repo card.

## Why this exists

The werk pipeline (`chorus_werk` → act → werk.yml) operates on the card's **chorus** werk (`chorus-werk/<role>-<card>/`). When a card's deliverable files live outside the chorus repo, that werk is **empty** — `chorus_werk` would merge/build/deploy nothing. Surfaced live on **#3383** (NiFi prep): the code was gathering-repo, the chorus werk was empty, and the land was a direct gathering commit (`ba06e09`) + `cards done`. That worked but was improvised. This names it as the path.

This is a **transition-era** tool: as the chorus-out-of-gathering extraction completes, fewer cards are cross-repo (chorus owns its own code). The end state is most cards being chorus-native and riding the normal pipeline.

## Detection

A card is **cross-repo** when its deliverable files live outside `/CascadeProjects/chorus`. Common foreign repos: `jeff-bridwell-personal-site` (gathering), `shared-observability`.

Tag it **`repo:<name>`** (e.g. `repo:gathering`) at filing time. Detection must be **tag-automatic** (Wren): `chorus_werk` / the demoer reads the tag at pull/demo and routes a `repo:<foreign>` card down this path — the demoer never has to *remember* it's cross-repo. And the tag is **machine-checked, fail-loud** (Wren, same fail-closed discipline as the write-door): `repo:X` must resolve to a **known** repo or the pull/demo refuses — an unknown repo tag must never silently misroute. The tag fits the existing card-tag metadata surface — **no schema change**; the only build is a routing branch + the known-repo check, nothing standing (it's ephemeral, born to die).

If a card touches **both** repos, split it (the chorus half rides `chorus_werk`; the foreign half follows this convention) — the same forcing reason that split #3383 out of #3097.

## Land path (instead of `chorus_werk`)

1. **Build + test** in the foreign repo (its own test runner, its own tsc/lint).
2. **Cold-eyes** on the foreign-repo diff — record the five gates manually (the chorus demo pipeline has no variant to gate). Peer gathers run as normal (the 4-question review).
3. **Commit** to the foreign repo through *its own* git + pre-commit + CI. There is **no chorus merge/deploy** — the foreign repo's flow is authoritative for its code.
4. **`cards done <id>`** on the chorus board — the board card closes directly, not through `werk-accept`. **Foreign-repo CI-green is a HARD precondition for `cards done`** (Silas): because the gates here are recorded *manually* (no chorus pipeline ran them), the foreign repo's own CI passing is what stops the manual-gate path from becoming a skip-the-quality-layer hole. No green CI on the foreign side → no `cards done`.

There is **no chorus variant to demo**: the demo surface is the foreign repo or its running app. State that on the card so the product gate doesn't expect a chorus-served page.

## Commit hygiene (the real hazard)

Foreign repos are **not werk-isolated** — the working tree may carry unrelated in-flight changes. So:

- `git add` the card's files **explicitly**, never `git add -A` / `git add .` (on #3383, the gathering tree had 7 unrelated stray files that a blind add would have entangled).
- Use **`git -C <repo-path>`** rather than `cd <repo> && git ...`: the infra-guard's `git add`/`git commit` deny-text currently redirects to **`git-queue.sh`, which is retired** (#3182; phantom-reference tracked in #3290/#3393). `git -C` is the interim sanctioned path until the deny-text names the real one.

## Open follow-on

- Fix the infra-guard deny-text (#3290) so it names a path that exists — until then this doc's `git -C` note is the workaround.
- Consider whether cross-repo work should be chorus-board cards at all, or tracked in the owning repo — a question for the priorities layer as extraction shrinks the cross-repo surface.
