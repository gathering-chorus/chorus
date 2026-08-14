# ADR-058 — The athena verb family and governance-as-checkable-data

**Status:** Accepted (Jeff-approved card #3846, 2026-08-13; direction re-confirmed by Jeff 2026-08-14 — "design an athena-validate verb that scans a graph for any violations of adrs and practices and decisions")
**Owner:** Silas (architecture) · Wren (verb/domain code) — pairing per Jeff 2026-08-14
**Supersedes:** the 2026-06-22 athena step naming (model → make → deploy → land)

## Context

The athena value stream lost its proving steps. The 06-22 renaming to
model/make/deploy/land mirrored the verbs that existed at the time and silently
dropped **seed** and **validate** — the two steps that make a generated surface
*proven* rather than merely present. The consequences were measured, not
hypothetical: the Properties domain sat "hollow" for two months (#3845 — a
compiled-in cap pretending to be governed data), and athena-validate's first
sweep (#3846) found products claiming domains that do not exist.

Separately, the rules that actually bit us — one home graph per subject,
an edge lives in its subject's home graph — are **cross-graph conventions SHACL
cannot express**, and until today they lived only in ADR prose and session
memory, where they get re-derived (twice in 16 hours on 08-13/14).

## Decision

1. **Step taxonomy** (value-stream-athena, per athena-value-stream-design.html):
   `shape → forge → seed → validate → demo → live`.
   Owners per the doc: shape=Silas (OWL-DBA), forge=Wren (generator, owl-api core),
   seed=Silas (DAL #3257), validate=Silas (CI), demo=Jeff.
2. **`chorus_athena` extends athena-forge** — the orchestrator is the
   chorus_werk analog dispatching the athena-* verbs; it grows out of owl-api
   (Wren's crate), never a competing implementation. Each verb refuses until
   its precedent step is proven.
3. **Governance is checkable data**: `chorus:GovernanceCheck` individuals
   (governance-checks-3846.ttl) bind a SPARQL rows-are-violations query to the
   ADR/DEC/practice it enforces, with severity and a recorded proving run
   (`provenRedOn` + `provenRedRows`). `athena-validate` runs the SHACL floor
   plus every registered check and cites the governing document per violation.
   **A check that has never been red may not gate** — register-before-run is
   the #3734 negative-proof discipline expressed as schema.
4. **Adding a rule is a model write**, never a deploy. The registry lives in
   `urn:chorus:ontology` and deploys with the MODEL_SET.

## First registered checks (proven red on the live store, 2026-08-14)

| check | boundTo | provenRedRows | found |
|---|---|---|---|
| gc-one-home-per-subject | ADR-051 | 4 | pulse, clearing, borg, werk typed in both ontology and instances |
| gc-edge-follows-node | this ADR | 3 | role-{silas,wren,kade} word-cap edges asserted outside the subjects' home |

`gc-edge-follows-node` flipping green is the acceptance proof of #3876 (the
edge move) — the first fix whose done-ness is defined by a governance check.

## Consequences

- ADR compliance stops being archaeology: violations are queryable, cited, and
  counted, and the "found N then, M now" arc replaces ran-once checkbox proof.
- The retired step subjects (athena-model/make/deploy/land) are staged in
  model-retirements.jsonl and executed at this card's land (#3752 path);
  the six replacement steps seed in the same transaction.
- The verb *names* athena-model / athena-deploy (binaries) are unchanged — they
  are implementations that serve the shape/seed and live steps; the stream
  describes the flow, not the binary inventory.
