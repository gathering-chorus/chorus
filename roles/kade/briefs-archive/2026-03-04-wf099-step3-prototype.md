# Brief: WF-099 Step 3 — Prototype a Visible Surface

**From:** Silas | **To:** Kade | **Workflow:** WF-099 | **Card:** #947

## Context

Step 1 (value model) and Step 2 (architecture) are done.
- Step 1: `jeff-bridwell-personal-site/data/about/SYSTEM_MODEL.md`
- Step 2: `architect/docs/CONCEPTUAL_ARCHITECTURE.md`

## Your Task

Pick one piece of the model and make it visible in the app. Three options ranked by impact:

### Option 1: /borg page (recommended)
A convergence dashboard showing:
- Board Borg output (cards absorbing cards — data already exists in werk-init.sh)
- Cross-domain connection count (SPARQL query across named graphs)
- Recent spine event patterns (Chorus API query)

Low code — mostly wiring existing data into a new view.

### Option 2: Cycle trace
Pick a real idea that became a card → feature → absorbed other ideas. Show the full Ideate → Think → Build → Borg path with timestamps and artifacts. Narrative UI.

### Option 3: Connection ratio
Instrument the cross-domain connection count as a single number with trend. Simplest but least visual.

## Key References
- `architect/docs/CONCEPTUAL_ARCHITECTURE.md` — sections 4 (Borg) and 7 (Step 3 preview)
- `SYSTEM_MODEL.md` — the value model you're making visible
- Board Borg output: `werk-init.sh` Board Borg section
- Spine events: `chorus.log` via Chorus API
