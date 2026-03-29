## API Endpoint Table (Chorus API — localhost:3340, no auth)

| Domain | Endpoint | Filters | Notes |
|--------|----------|---------|-------|
| Cards | `GET /api/chorus/cards` | `?owner=silas&status=Now` | Wraps cards — DEC-093 compliant |
| Cards | `GET /api/chorus/cards/:id` | — | Full card detail with description, comments |
| Cards | `GET /api/chorus/cards/domain/:domain` | — | Filter by domain label |
| Roles | `GET /api/chorus/roles` | — | All roles with current state |
| Roles | `GET /api/chorus/roles/:id` | — | Single role detail |
| Roles | `GET /api/chorus/roles/:id/state` | — | Just the andon state |
| Perf | `GET /api/chorus/perf` | — | Latest perf-baseline run with deltas and pass/fail |
| Services | `GET /api/chorus/services` | — | All LaunchAgents with PID, status, RSS |
| Disk | `GET /api/chorus/disk` | — | Library disk usage, warning/critical flags |
| Harvest | `GET /api/chorus/harvest` | — | Graph counts and triples per domain from Fuseki |
| Cost | `GET /api/chorus/cost` | `?period=summary` | Cost report output (summary, daily, weekly) |

CLI (`cards`) for mutations. API is read-only.
