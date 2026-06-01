# Chorus Search — E2E Test Cases (spec)

**Purpose:** golden cases that validate `/api/chorus/search` *function* — relevance, ranking, coverage, recency — repeatably and against the **live index**. Assert **structural properties, not exact rows**, so they survive corpus drift (the data changes daily; the *shape* of a correct answer doesn't).

**Owner:** Wren (search / Knowledge domain).

**Gate:** a search/relevance change is not "done" until the affected cases pass **live**. This replaces eyeballing one query by hand.

---

## How to run

- **Live e2e** — against the running `~/.chorus/index.db` (real data). Stable anchors that won't churn: the Heidegger *Versammlung* research doc, `DEC-031` (Heidegger-rooted naming), `DEC-010` (the Gathering name).
- **Hermetic fixtures** — a small seeded index for the deterministic edge cases (dedup / echo / absent / scale), so they don't flake on data drift.
- **Harness** — jest in `platform/api`, hitting `runFtsQueryOnDb` + the search handler. Each case = (query, params) → structural assertion.

---

## Cases

| # | name | query / params | assertion (structural) | why |
|---|------|----------------|------------------------|-----|
| 1 | **authority: doc beats chatter** | `q=heidegger`, `mode=relevance` | top-5 contains ≥1 result with `source ∈ {doc, decision, artifact, memory}` matching `/versammlung\|heidegger/i`; **top-1 `source ∉ {claude, clearing}`** | authoritative knowledge outranks session chatter — gates the fts-query authority change (`6b024438`) |
| 2 | **recall: recency order** | `channel=session:<role>`, `order=recent`, `limit=30` | results strictly descending by timestamp; ≤30; all from that channel | the session-start rebuild stays intact — recency path is untouched by authority weighting |
| 3 | **no self-echo** | `q` = the requester's own recent prompt text | the requester's own current/recent prompt is **not** top-1 | search must not hand you your own prompt back as "context" |
| 4 | **dedup** | `q` = a dup-prone term, `mode=relevance` | no two top-K results share identical `content` | one line indexed as N rows must not eat slots |
| 5 | **absent → zero** | `q` = a token guaranteed absent (nonsense string) | exactly 0 results | genuine absence returns nothing — no LIKE-scan false positives |
| 6 | **structured fact** | `q` = an entity (e.g. a domain name), `mode=unified` | the structured/graph fact surfaces (not only text hits) | SPARQL/graph facts retrievable |
| 7 | **freshness metadata** | any `q` | response carries `_meta` { `stale`, `domain_coverage`, `newest_result_age_s` } | search-quality signal is present (the capture half) |
| 8 | **semantic fuzzy** *(PENDING)* | `q` = a paraphrase ("the German philosophy the product is named after"), `mode=semantic` | top-K contains the Versammlung doc | knowledge findable without naming the term — **blocked** until the knowledge corpus is embedded (today only `messages` is in LanceDB) |

---

## Notes

- **Case 1 is the gate on the current authority-weight deploy** — that change isn't done until #1 passes live on `:3340`.
- **Case 8 is deferred** — semantic only embeds `messages` today; the knowledge corpus (docs in LanceDB) doesn't exist yet (that's the projection contract from `chorus-search-tobe.svg`).
- **Cases 1 vs 2** map the "both activated" split: #2 guards **recency** staying clean (the conversation rebuild), #1 guards **relevance** (authority-ranked knowledge). They're separate modes; the suite proves they don't interfere.

Owner: Wren · drafted 2026-06-01 · implement (jest harness) when Silas's werk-build cascade fix clears.
