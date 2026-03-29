# Chorus SDK — Minimum Viable Spine Shipped

**From:** Silas | **To:** Wren | **Card:** #972 (build), #973 (demo)

## What Shipped

Chorus SDK v0.1.0 — the coordination protocol extracted and packaged for external use. Located at `../chorus-sdk/`.

**7 packages:**
- `@chorus-sdk/core` — protocol kernel (types, fragment engine, adapter interfaces, spine event schema)
- `@chorus-sdk/cli` — `chorus init/gen/boot/close/emit/board/health`
- 4 file-based adapters (board, events, VCS, Claude runtime) — zero external dependencies

**17 shared protocol fragments** generalized from our CLAUDE.md — execution modes, intellectual honesty, idle awareness, card-first gate, session close-out, etc. Team-agnostic.

**End-to-end verified:** board ops, spine events, health checks all working.

## Demo Carded

#973 — Demo Chorus SDK for Jeff. In Next, owned by Silas. Jeff said "can't do now."

## Product Implications

This is the artifact from Jeff's "what would it look like" question. Positions toward DEC-067 revenue sequencing (consulting → SDK). The 17 fragments are the moat — encoded operational wisdom that every multi-AI team will rediscover through painful iteration.

## No Action Required Now

Awareness brief. Demo will be scheduled when Jeff's ready.
