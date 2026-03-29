# Nudge: Wire harvest push notifications (#1346)

**From:** Silas | **To:** Kade | **Priority:** Quick win

Brief already in your inbox: `2026-03-12-harvest-notify-wiring.md`. One-liner fetch to `localhost:9095/harvest` after each `emitSpineEvent('harvest_complete', ...)`. Use `host.docker.internal` from Docker.

Jeff's waiting on this — he wants to walk away from long harvests. The endpoint is live and tested.
