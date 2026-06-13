# ADR-033: Werk Pipeline Model — RETIRED (number never independently landed)

**Status:** Superseded / Retired — 2026-06-13 (Silas, SA). This number was claimed for a "werk pipeline model" decision (Kade's #3078 reconcile, ~2026-05-25, the `act`-as-orchestrator + git⊃env⊃DSL framing) that was **ratified in discussion but never written as an ADR file**, and whose substance was then superseded before it could land.
**Superseded by:** ADR-037 (Atomic Verb Execution — the verbs-in-sequence / no-orchestrator model), ADR-036 (One Build/Deploy Execution Path), ADR-032 (Verb Contract v1). The surviving pipeline model — one env = production; `gh` = source-of-truth + per-card process-holder via one commit-status check per verb — lives in those ADRs + the ci-pipeline / werk subproduct design docs.
**Card:** #3393 (ADR hygiene pass).

## Why this stub exists

The 2026-06-13 ADR coherence audit found ADR-033 referenced by three design docs (`ci-pipeline-service-design.html`, `werk-subproduct-design.html`, `crawler-dependency-map.html`) while no ADR-033 file existed — a dangling decision-reference, and the docs disagreed (one called it "retired," another "ratified, pending landing"). This tombstone resolves the number so it isn't a black hole: **ADR-033 is retired; cite its successors (ADR-037/036/032) for the live pipeline model.** The `act`-as-orchestrator framing specifically is dead — the model is verbs-in-sequence with no orchestrator (ADR-037).

## Consequences

- The number is not reused; it stands as a retired marker (the supersession chain stays legible, like the ADR-001/011/012/015 → ADR-019 Docker→native chain).
- Docs that referenced ADR-033 for the pipeline model should cite ADR-037/036/032; the `werk-subproduct-design` "ratified, file landing via Silas — not yet in roles/silas/adr/" line is corrected by this retirement (it was never going to land independently).
