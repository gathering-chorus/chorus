# Brief: Kanban Tool Setup — Vikunja

**From**: Wren (PM)
**To**: Kade (Engineer)
**Date**: 2026-02-14
**Priority**: High — Jeff wants to review in morning
**Supersedes**: Previous version of this brief (GitHub Projects approach)

## What We Need

Install Vikunja as our team kanban board. It replaces GitHub Projects as the primary board. Vikunja is a Go-based, self-hosted task/project manager with kanban views, REST API, and API token auth. Single container, SQLite default, ~128 MB RAM.

**Why Vikunja over GitHub Projects**: Jeff wants a configurable board he can see and interact with directly, not a GitHub-native tool behind GraphQL. Vikunja has a clean web UI, proper REST API, and API tokens for each role. It also becomes a stepping stone — eventually Gathering will have its own board view, and Vikunja's API becomes a data source or gets replaced.

## Tasks

### 1. Install Vikunja via Docker Compose

Add Vikunja to the project's Docker Compose stack (or create a standalone compose file if cleaner).

**Minimal compose:**
```yaml
vikunja:
  image: vikunja/vikunja
  container_name: vikunja
  ports:
    - "3456:3456"
  volumes:
    - ./vikunja/files:/app/vikunja/files
    - ./vikunja/db:/db
  environment:
    VIKUNJA_DATABASE_PATH: /db/vikunja.db
    VIKUNJA_SERVICE_FRONTENDURL: http://localhost:3456/
  restart: unless-stopped
```

- Check the [Vikunja Docker docs](https://vikunja.io/docs/full-docker-example/) for the latest recommended setup
- Make sure the data volume is persistent (SQLite DB + uploaded files)
- Confirm it starts cleanly and the web UI is accessible at `http://localhost:3456`
- Add to the project's `.gitignore`: `vikunja/db/`, `vikunja/files/`

### 2. Initial Configuration

Once running:

**Create admin account:**
- Username: `jeff` (or whatever Jeff prefers)
- This is done on first visit to the web UI — document the URL

**Create API tokens** (Settings → API Tokens in Vikunja UI, or via API):
- One token per role: `wren-token`, `silas-token`, `kade-token`
- Store tokens in a `.env` file that's gitignored (e.g., `../messages/scripts/.env`)
- Document the token format: `Authorization: Bearer <token>`

**Create the board project:**
- Project name: **Gathering**
- Enable Kanban view as default

**Configure Kanban columns** (these map to task statuses/buckets in Vikunja):
- Todo
- Ready (unblocked and prioritized, not started)
- In Progress
- Blocked
- Done

**Create labels** for categorization:
- **Owner labels**: `owner:wren`, `owner:silas`, `owner:kade`, `owner:jeff`
- **Priority labels**: `P1` (red), `P2` (yellow), `P3` (green)
- **Domain labels**: `health`, `save$`, `make$`, `house-garden`, `gathering`, `infrastructure`

The domain labels map to Jeff's whiteboard columns — this board should feel like a digital version of his physical whiteboard.

### 3. Build a Wrapper Script

Create `board.sh` in `../messages/scripts/` that wraps Vikunja's REST API.

**Commands:**
```
board.sh list                                    # Show all tasks grouped by bucket/status
board.sh add "title" --status "Todo"             # Add a task
board.sh add "title" --status "Todo" --owner "Kade" --priority "P1"
board.sh move <id> "In Progress"                 # Change bucket/status
board.sh done <id>                               # Move to Done
board.sh block <id> "reason"                     # Move to Blocked + add comment with reason
board.sh unblock <id>                            # Move back to Ready
board.sh mine                                    # Show tasks with current role's owner label
board.sh view <id>                               # Show full task details
```

The script should:
- Read the API token from `VIKUNJA_TOKEN` env var or `../messages/scripts/.env`
- Use `curl` against `http://localhost:3456/api/v1/...`
- Cache project ID and bucket IDs locally (e.g., in a `.board-cache` file) to avoid repeated lookups
- Format output cleanly for terminal reading (columns aligned, color-coded if possible)
- Work on macOS (no GNU-isms)
- Be well-commented

**Vikunja API reference**: Available at `http://localhost:3456/api/v1/docs` (auto-generated Swagger) once the instance is running. Key endpoints:
- `GET /api/v1/projects` — list projects
- `GET /api/v1/projects/{id}/tasks` — list tasks in a project
- `PUT /api/v1/projects/{id}/tasks` — create a task
- `POST /api/v1/tasks/{id}` — update a task
- `GET /api/v1/projects/{id}/buckets` — list kanban buckets
- `POST /api/v1/projects/{id}/buckets` — create a bucket
- `POST /api/v1/tasks/{id}/buckets` — move task to bucket

Check the Swagger docs for exact request/response shapes once it's running.

### 4. Seed Initial Items

Add these items to the board with appropriate labels:

**P1 — Now:**
- Exploratory UI testing (owner:jeff) — confirm flows after visibility middleware changes
- Access control permutation matrix (owner:kade) — map 90 permutations against E2E coverage, find gaps

**P2 — Ready:**
- Vision refinement session (owner:wren) — 7-item agenda, extended session with Jeff
- Blind-between-roles coordination review (owner:wren) — Jeff reflecting on team coordination model

**P3 — Todo:**
- GitHub Actions board automation design (owner:kade) — design doc first, build later
- Gathering Phase 1: board view in app (owner:kade) — build kanban view into Gathering that reads from Vikunja or replaces it

**Domain labels**: Apply `gathering` to the board/app items, `infrastructure` to the CI/testing items.

### 5. Update CLAUDE.md Files

Add Vikunja board instructions to all three CLAUDE.md files (Wren, Silas, Kade):

```markdown
## Team Kanban Board (Vikunja)

Board URL: http://localhost:3456
Wrapper script: `../messages/scripts/board.sh`

On session start:
- Run `board.sh list` to see current board state
- Check for items assigned to your role: `board.sh mine`

During work:
- When starting an item: `board.sh move <id> "In Progress"`
- When blocked: `board.sh block <id> "reason"`
- When done: `board.sh done <id>`

When creating new work:
- Add items via `board.sh add "title" --owner "YourRole" --priority "P1"`
- Or use the Vikunja API directly if the wrapper doesn't cover your need
```

### 6. Test It

- Start Vikunja, verify web UI loads at `http://localhost:3456`
- Create the project, buckets, and labels via UI or API
- Run `board.sh list` — verify clean output
- Add a test item, move it through all states, delete it
- Verify each role's token works independently
- Post results to the activity log (`../messages/activity.md`)

## Constraints

- Vikunja runs on port 3456 by default — check for conflicts with the existing stack
- SQLite DB lives in a Docker volume — make sure it's persistent across container restarts
- Don't over-configure — start minimal, Jeff will customize in the UI
- API tokens and `.env` file must be gitignored

## Context

Jeff's physical whiteboard has domains as columns (Health, Save$, Make$, House/Garden) with Now/Next rows and engineering metrics on the side. This board should feel like a digital version of that — configurable, visual, personal. Not a generic Jira clone.

This is also a stepping stone to Phase 1: building a board view directly into Gathering. Vikunja's REST API makes it clean to read data from and eventually migrate away from. Dog-food first, then build our own.

The previous brief for GitHub Projects integration is superseded. Silas's board policies brief (`../architect/briefs/2026-02-13-kanban-board-policies.md`) has good policy guidance (WIP limits, workflow states) — those are folded in here.

— Wren
