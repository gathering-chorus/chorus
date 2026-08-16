# ADR-059: Vocabulary semver — declared at the pen, implemented at the make

**Status:** Proposed — drafted on #3902 (Jeff-directed, 2026-08-15/16), lands with it
**Author:** Silas (OWL-DBA)
**Builds on:** ADR-058 (athena verb family), #3288 (version-ledger pattern), ADR-032 (zero-dep verbs)

## Context

Jeff, 2026-08-15: "are our owl and owl-api domains semantically versioned?" The
live probe said no: two stale hand-stamps in `owl:versionInfo` (0.2.0-June,
0.3.0), envelopes serving `modelVersion: "unclassified"` and a content *hash*.
A hash says something changed; it can never say whether the change breaks you.
Jeff: "seems like an issue" → "athena-model declares versions and athena-make
implements them" → instance tagging "makes version migrations less arbitrary"
→ "all of this versioning discussion need adrs."

## Decision

1. **One home, ledger-shaped.** `designing/schemas/model-version-ledger.jsonl`
   is committed and append-only; each entry pins {version, bump, verb, target,
   by, card, date}. The head IS the vocabulary's semver. This deliberately
   copies the #3288 claudemd pattern: one home, derived, never hand-edited.

2. **Declared at the pen.** Every athena-model TBox write classifies itself
   deterministically and bumps the ledger at the write:
   - `retire` (a term leaves the vocabulary) → **MAJOR**
   - `class` / `property` / `shape` (additive or tightening) → **MINOR**
   - ABox writes (`seed`, entity verbs) never bump the vocabulary.
   Classification is by verb, never judgment — a new verb must be added to
   `classify()` or the pen refuses.

3. **Projected into the store.** The pen re-renders
   `designing/data/model-version.ttl` (GENERATED; in the MODEL_SET) carrying
   `chorus:model chorus:vocabVersion "<head>"`, so the store itself is
   versioned and a reload reproduces it.

4. **Implemented at the make.** owl-api (→ `athena-make`, Jeff's 08-13 rename
   ruling — executed under this program when the cdhash/TCC grant plan is
   ready) serves `vocabVersion` on every envelope. Absent from the store →
   `"unversioned"`, loud, never defaulted to a number.
   `chorus:modelVersion` is untouched: it remains the per-class REVIEW
   classification (target/legacy), a different question.

5. **Hand-edits are detectable, not forbidden by prose.** The security probe
   `vocab-version-ledger.sh` (#3900 suite, nightly) compares the SERVED version
   against the ledger head; any mismatch reds. Proven red before this card's
   land (projection not yet deployed), green after — register-before-run.

## Named future legs (not this card)

- **Per-domain grain**: today the ledger versions the whole vocabulary; when
  `definesVocabulary` claims give every term one owning domain, the ledger
  entries gain a `domain` field and heads become per-domain.
- **ABox stamps**: instances stamped with the vocab version they were written
  under (rides the pen's provenance stamp) → `gc-unmigrated-instances`
  (`stamped != current` as rows-are-violations) makes migration a query.
  Jeff, 08-15: this is what "makes version migrations less arbitrary."
- **Grammar-in-graph**: the act/MCP execution surfaces declare their grammars
  in the graph and version them the same way (Jeff's 08-16 act↔MCP parallel).

## Consequences

- A consumer can pin: `vocabVersion` major-bump = review your reads.
- The two stale `owl:versionInfo` hand-stamps stop being the version story;
  they remain as historical annotations only.
- Every future "is this versioned?" is answerable by one probe, not archaeology.
