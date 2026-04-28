# Wren — Next Session

## Last session (2026-04-28)

**The #2545 curation arc shipped end-to-end.** Three children Done:
- **#2549** doc-catalog write API — five-field tag + lineage predicates + drift + curated overlay + audit feed; persists to `urn:chorus:instances` per #2314 pattern
- **#2550** curation side-panel UI — click any catalog row → edit tags, add lineage, see drift accept/dismiss, walk untagged docs (j/k/Esc), audit feed reads chorus.log
- **#2554** SHACL CatalogDocShape + finished Borg→subproduct migration; 4 runtime CHECKs surface non-conforming CatalogDocs at `/api/athena/validate`

**Two ontology shifts surfaced inline by curation work:**
- **Borg → Chorus subproduct** (commit `8e17289e`) — was a top-level Product, demoted to SubProduct via SPARQL update + 3 code-cache syncs + 1 new test
- **Akasha → Consulting product** (commit `f403cac1`) — Athena had no Akasha Product instance though tagger emitted `product=akasha`; folded into new `chorus:consultingProduct`

**ADR-027 drafted, reviewed, accepted** — "derived domain mappings live in the graph, not in code." Three landed cases (#2314 hasPrinciple, #2318 hasDecision, #2516 hasTestPathPrefix) plus two pending sites (discover-code, discover-pages). Silas + Kade reviewed; all tightenings folded (commit `aeca02a1`).

**Peer demos gated:** Kade's #2514 (test reshape pattern), #2516 (graphify aliases). Silas's #2523 (hermeticity audit), #2524 (test categorization), #2525 (DEC-2525 required-checks).

**Other:** Filed #2552 (done-gate routing fix — Kade picking up; retires the #1815→#2338→#2440 chain). Moved #2547 SWAT→Next (Silas-domain). Helped Jeff frame Tuesday consult with Anthe (HTML at `roles/wren/notes/anthe-tuesday-framer.html`).

## Pattern Jeff named today

"Almost 100% of cards now end up generating follow-up / fix cards — and it compounds." We talked the structural shape: cards close with asterisks → felt-incomplete tax compounds → arcs scope wider → more follow-ons. Three structural moves named (close arcs not cards, cap follow-ons at one, let observations die in conversation). Jeff asked me to receive the pattern, not act on it. Carrying as framing into next session.

## Anthe consult — Tuesday

Framer ready at `roles/wren/notes/anthe-tuesday-framer.html`. Jeff scopes to (1) diagnose, (2) device + site hygiene. The "bot" Anthe wants is something she installs and pays for (Aura/IdentityForce). Boundary line in close section.

## Open follow-ons

- **#2547** (Silas) — CHORUS_INJECT_DRY_RUN nudge leak, P1, awaiting
- **#2552** (Kade) — done-gate routing fix; sequenced after his #2440 close
- **#2545 parent** — children all Done; arc closes when Jeff actually curates through the surface in real use (not on /acp)
- **Bats convention extension** (deferred from #2524) — uncarded, awaiting Jeff's pull
- **ADR-027 pending sites**: discover-code.ts + discover-pages migrations, file when pulled

## What next session should know

- The catalog at `localhost:3340/doc-catalog.html` is now a working surface. Hard-reload picks up wave 2. The "Walk untagged" button + side-panel is the rhythm Jeff named as "tool you use, not one you admire."
- Curated overlay (`/api/chorus/catalog/curated`) merges over runtime tagger output. Untagged count is now honest at 2 — genuinely no tagger signal on `/loom/policies.html` and `/loom/principles.html`; both already curated by Jeff via the API.
- The leverage move ADR-027 names is shape-driven form generation. Whoever pulls discover-code/discover-pages should land them with that arc in mind, not as bespoke per-predicate work.
