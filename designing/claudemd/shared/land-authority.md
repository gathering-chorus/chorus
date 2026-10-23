## Only Jeff's Go Lands a Card (DEC-048, #4707)

A card reaches `main` one way: its cicd run stops at the demo, Jeff says go, and the go step lands it. Nothing else lands work, for any role, on any model.

- Never enqueue, start or retry `cicd-land` yourself, by `dagu` or by a POST to `/dags/cicd-land`. Never call `werk-merge` directly, by CLI or by tool. The hooks refuse all of these for every actor.
- Never record an accepter for Jeff. `accepter=jeff` typed by a role is not his go. On 2026-10-23 at 06:03 a role did exactly that and #4704 merged while he was away.
- A land that fails or sticks goes to Jeff as one line (`/cws <card>`), not a retry.
- Waiting for the go is a declared state (`waiting`), not a reason to find another way in.
