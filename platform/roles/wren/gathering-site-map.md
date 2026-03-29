# Gathering Site Map

**Author**: Wren (PM)
**Date**: 2026-02-14
**Purpose**: Big-picture view of every page, API surface, and multi-step workflow
**Note**: Screenshots pending — will capture thumbnails when the app is running

---

## Navigation by Role

What each user type sees in the navbar:

```
UNAUTHENTICATED                AUTHENTICATED (non-admin)         ADMIN (full access)
─────────────────              ──────────────────────────        ───────────────────
Home                           Home                              Home
Login                          Projects ▾                        Projects ▾
                                 Light Life Urban Gardens           Light Life Urban Gardens
                               Logout                            Collections ▾
                                                                    Blog
                                                                    Books
                                                                    Property
                                                                    Gallery
                                                                    Ideas & Projects
                                                                    Music (disabled)
                                                                    Images (disabled)
                                                                    Movies (disabled)
                                                                  Blog ▾
                                                                    lifes-practice (WordPress)
                                                                    Admin (WordPress)
                                                                  Admin ▾
                                                                    Dashboard
                                                                    Access & Users
                                                                  About (Docs)
                                                                  Logout
```

---

## All Pages

### Public (no login required)

| Page | URL | What It Does |
|------|-----|-------------|
| **Home** | `/` | SOLID intro, provider selector, login options |
| **Login** | `/login` | SOLID authentication — choose provider, authenticate |
| **Blog** | `/collection/blog` | Blog posts list (visibility-gated — currently public) |

### Authenticated (login required)

| Page | URL | What It Does |
|------|-----|-------------|
| **Profile** | `/profile` | User profile, collections overview, logout |

### Admin Only

| Page | URL | What It Does |
|------|-----|-------------|
| **Books** | `/collection/books` | Books library — card grid, filters, location info |
| **Property** | `/collection/property` | Houses, gardens, lands — photo galleries, room/bed management |
| **Gallery** | `/collection/gallery` | Photo gallery — local filesystem proxy |
| **Ideas & Projects** | `/collection/ideas` | Combined ideas + projects view — status, tags, promotion |
| **Book Upload** | `/books/upload` | Multi-step book ingestion wizard (see workflow below) |
| **Incubation** | `/incubation` | Chat-style idea capture — commands, promotion, merging |
| **Dashboard** | `/dashboard` | Pod admin — file browser, SPARQL/YASGUI, activity, WebVOWL |
| **Access & Users** | `/admin/access` | User management + collection visibility toggles |
| **Docs** | `/docs` | Documentation hub — categorized docs |

**Total: 12 HTML pages** (3 public, 1 authenticated, 8 admin)

---

## Page Flows

### Flow 1: First Visit → Login → Profile

```
[Home /]
  │
  ├── Choose SOLID provider
  ├── Click "Login with SOLID"
  │     │
  │     └── [SOLID Provider] ──redirect──→ [/callback] ──→ [/profile]
  │
  └── Or: "Server-side Login" button
        │
        └── POST /login ──→ [SOLID Provider] ──redirect──→ [/callback] ──→ [/profile]
```

### Flow 2: Admin Navigation

```
[Profile /profile]
  │
  ├── Collections ▾
  │     ├── [Blog /collection/blog]
  │     ├── [Books /collection/books] ──→ "Add Books" ──→ [Book Upload Wizard]
  │     ├── [Property /collection/property] ──→ manage houses/gardens/photos
  │     ├── [Gallery /collection/gallery]
  │     └── [Ideas & Projects /collection/ideas] ──→ or ──→ [Incubation /incubation]
  │
  ├── Admin ▾
  │     ├── [Dashboard /dashboard]
  │     │     ├── File Browser tab ──→ browse pod files
  │     │     ├── SPARQL tab ──→ YASGUI query editor
  │     │     ├── Activity tab ──→ recent changes
  │     │     └── Ontology tab ──→ WebVOWL (localhost:8089)
  │     │
  │     └── [Access & Users /admin/access]
  │           ├── Users tab ──→ add/remove/edit users
  │           └── Visibility tab ──→ toggle collection public/private
  │
  └── Blog ▾
        ├── lifes-practice ──→ external WordPress site
        └── Admin ──→ WordPress admin panel
```

### Flow 3: Unauthenticated Visitor

```
[Home /]
  │
  ├── [Blog /collection/blog] ──→ 200 (public)
  │
  ├── [Books /collection/books] ──→ 302 redirect to /login
  ├── [Property /collection/property] ──→ 302 redirect to /login
  ├── [Ideas /collection/ideas] ──→ 302 redirect to /login
  │
  └── Any admin page ──→ 302 redirect to /login
```

### Flow 4: Authenticated Non-Admin

```
[Profile /profile]
  │
  ├── [Blog /collection/blog] ──→ 200 (public)
  │
  ├── [Books /collection/books] ──→ 403 Forbidden
  ├── [Property /collection/property] ──→ 403 Forbidden
  ├── [Ideas /collection/ideas] ──→ 403 Forbidden
  │
  └── Any admin page ──→ 403 Forbidden
```

---

## Multi-Step Workflows

### Workflow 1: Book Upload (6 steps)

The most complex workflow in the app. Admin enters from Books collection page.

```
Step 1: Create Session          POST /api/books/upload/session
  │
Step 2: Upload Photos           POST /api/books/upload/photo (repeatable)
  │                             DELETE /api/books/upload/photo/:id (cleanup)
  │
Step 3: AI Classification       PUT /api/books/upload/photo/classify
  │                             (Claude Vision: cover vs publisher page)
  │
Step 4: Metadata Extraction     POST /api/books/upload/process
  │                             (Claude Vision + OpenLibrary lookup)
  │
Step 5: Manual Pairing          POST /api/books/upload/pair (optional)
  │                             (correct auto-pairing, assign locations)
  │
Step 6: Confirm & Save          POST /api/books/confirm
  │                             (writes Turtle to pod, redirects to /collection/books)
  │
Cleanup:                        DELETE /api/books/upload/session
```

### Workflow 2: Property Photo Management

Three intake paths, one organization model.

```
Path A: Direct Upload           POST /api/property/photos
Path B: iCloud Upload           POST /api/property/photos/icloud
Path C: Google Photos           GET  /api/property/google-photos/auth
  │                             ──→ OAuth ──→ Picker ──→ Poll ──→ Import
  │
All paths ──→ Photo associated with house/garden/land
              Auto-thumbnail generated
              Served via /api/property/photos/:id
```

### Workflow 3: Idea Incubation

Chat-style interface with command parsing. All via `/incubation`.

```
Capture:    Type plain text ──→ POST /api/ideas (status: captured)
            (first line = title, #tags auto-extracted)

Develop:    /tag <slug> <tags>        (add tags)
            /status <slug> <status>    (captured → developing → parked)

Promote:    /promote <slug>           (idea → project)
            POST /api/ideas/:slug/promote

Merge:      /merge <slug> into <target>
            POST /api/ideas/:slug/merge

Direct:     /project <title>           (skip idea stage)
            POST /api/projects

Delete:     /delete <slug>
            DELETE /api/ideas/:slug or /api/projects/:slug
```

**Status lifecycle**: captured → developing → parked → merged/abandoned
**Project lifecycle**: active → completed/abandoned

### Workflow 4: User Access Management

Admin manages who can see what. Via `/admin/access`.

```
Users Tab:
  List users ──→ GET /api/admin/users
  Add user   ──→ POST /api/admin/users (by SOLID webId)
  Edit user  ──→ PUT /api/admin/users/:encodedWebId
  Remove     ──→ DELETE /api/admin/users/:encodedWebId

Visibility Tab:
  Per collection: blog | books | property | ideas | projects
  States: Public | Private (Selective planned for Phase 1)
  GET  /api/admin/collection-visibility/:type
  PUT  /api/admin/collection-visibility/:type
```

---

## API Surface Summary

### By Collection

| Collection | API Reads | API Writes | Visibility | Auth |
|-----------|-----------|-----------|------------|------|
| **Blog** | None (WordPress-driven) | None | Public | Middleware-gated |
| **Books** | 4 endpoints (list, get, locations, images) | 10 endpoints (CRUD + upload workflow) | Private | Read: vis-gated, Write: admin |
| **Property** | 12 endpoints (houses, gardens, lands, photos) | 20+ endpoints (CRUD + photos + Google Photos) | Private | Read: vis-gated, Write: admin |
| **Ideas** | 2 endpoints (list, get) | 5 endpoints (CRUD + promote + merge) | Private | Read: vis-gated, Write: admin |
| **Projects** | 2 endpoints (list, get) | 3 endpoints (CRUD) | Private | Read: vis-gated, Write: admin |

### Admin-Only Systems

| System | Endpoints | Purpose |
|--------|-----------|---------|
| **Dashboard** | 7 | File browser, SPARQL, sync, activity, stats |
| **User Management** | 5 | CRUD users + access info |
| **Visibility** | 2 per collection | Get/set collection visibility |
| **Pod Management** | 4 | Create/read/write pods |
| **Ontology** | 1 | Serve ontology definition |
| **Webhooks** | 2 | WordPress webhook + harvest trigger |
| **Groups/ACL** | 7 | Group management + ACL read/write |

### Totals

| Category | Count |
|----------|-------|
| HTML pages | 12 |
| API read endpoints | ~25 |
| API write endpoints | ~45 |
| Admin-only systems | 7 |
| Multi-step workflows | 4 |
| External integrations | 4 (WordPress, Google Photos, iCloud, Claude Vision) |

---

## Future Slots (Defined but Empty)

These navbar items exist but are disabled — ready for ontology + harvester work:

| Collection | Navbar Status | Ontology Status |
|-----------|--------------|-----------------|
| **Music** | Disabled | Stub only (Silas Gap 3) |
| **Images** | Disabled | Stub only |
| **Movies** | Disabled | Stub only |

---

## Cross-Reference: Security Coverage

See `access-control-permutation-matrix.md` for the full 90-permutation analysis. Key finding: **39% coverage** with critical gaps on write operation denial tests.

| What's Well-Covered | What's Missing |
|---------------------|----------------|
| Read access by all 3 user types | Write denial for non-admin (CRITICAL) |
| Visibility middleware for private collections | Visibility state transitions |
| Admin CRUD operations | Blog API paths (none exist) |
| Dynamic visibility changes (single collection) | Edge cases (malformed .meta.ttl) |

---

*Screenshots will be added when the app is running. Use `npm start` in the personal site project, then we can capture page thumbnails with Playwright.*

--- Wren
