# ADR-057: The declared home and its projections

**Date**: 2026-08-10
**Status**: Proposed (rides #3776)
**Deciders**: Jeff Bridwell, Silas; shape reviewed by Wren (CoS re-sequencing, 2026-08-10)

## Context

"Who may enter" had four answers on 2026-08-10: the guard's hand-kept text file,
`urn:chorus:domains:security`, `urn:chorus:domains:identity`, and a hardcoded
array in the gathering app — and they had already diverged (flow-probe existed
in one graph and not the other). Wren's audit traced the class further: the same
disease produces fourteen HTML files for ~six real UI surfaces, because nothing
declares what exists. Jeff has asked for a runtime registry since February
(#1647, renamed and killed); his ruling on 2026-08-10 is that the registry is a
**constraint on how outcomes are built, not a phase** — it must fall out of
shipping the experience, and it is only real if the doors read it.

## Decision

1. **Every kind of thing that grants or receives access has ONE declared home
   graph, named here, and the home is what doors consult.**
   - Principals (who exists, who may sign in): `urn:chorus:domains:security`.
     This is the graph the Clearing and the API envelope already read
     (`CHORUS_ALLOW_SET_GRAPH` default). `domains:identity` copies are retired
     by #3785, not read by anything new.
2. **A door that cannot read the graph reads a GENERATED PROJECTION of it —
   never a hand-kept copy.** A projection is rendered from the same source the
   deploy validates, carries a generated-header naming its source, and lands in
   the repo like any generated artifact (#3466 convention). Hand-editing a
   projection is a defect the fitness function catches.
3. **live == generated is machine-measured.** Each projection gets a fitness
   row that re-renders and diffs; any hand edit or drift is a red line on
   /security.html by the next run, not an incident.
4. **The shape generalizes to all five surface kinds** — UIs, APIs, CLIs,
   notifications, events — one convention, declared per kind in its domain
   graph, projections only where a consumer cannot query. Wren's observation is
   the why: UI is Jeff-only, API+events are agent-only, and CLI+notifications
   serve BOTH — which is exactly why those two rotted; a surface nobody's tools
   enumerate is a surface nobody maintains. The UI registry (Wren's next card)
   uses this convention rather than minting a second one.

## Consequences

- The guard's `config/share-principals.txt` becomes a projection of
  `canSignIn="true"` Principals (#3776): adding a person is a model edit; the
  file regenerates at deploy.
- Two reach changes surface immediately and deliberately, because the model is
  now the policy: marknakib is admitted (his Principal says canSignIn true; the
  hand file's "no account yet" comment was stale) and Jeff's legacy
  localhost:3001 webId variant is dropped (sign-ins present the public issuer).
  Either being wrong is a MODEL fix, not a file edit.
- The gathering app's `authorized-users.ts` DEFAULT_USERS is the next
  projection to convert (#3785's reopened scope names it).
- What a person may DO remains outcome 3 (tiers, `chorus:Permission` decision,
  #3729) — this ADR fixes WHO EXISTS having one answer, not authorization.

## Refused alternatives

- **A registry service/phase built first** — refused by Jeff 2026-08-10: plumbing
  ahead of experience, and a registry nothing reads is an artifact.
- **Longest-lived list wins ad hoc** — collapsing into whichever copy seemed
  authoritative per door reproduces the drift with extra steps.
