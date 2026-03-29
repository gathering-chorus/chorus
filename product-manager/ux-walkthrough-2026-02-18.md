# Gathering UX Walkthrough — 2026-02-18

**Reviewed by**: Wren (PM) with Jeff
**Method**: Jeff took screenshots, Wren gave honest UX feedback
**Key decision made during walkthrough**: DEC-021 — Kill the nav bar, mind map is primary navigation

---

## Site Map & Page Flow

```
Login Page (fence painting + SOLID auth)
  → SOLID Provider Auth (Pivot)
    → Profile / Mind Map (landing page after login)
        ├── Gathering (capture & seeds)
        │     └── Capture Triage
        ├── Cultivating (ideas & growth)
        │     ├── Glimmers → Glimmer List
        │     ├── Ideas & Projects → Ideas List / Projects List
        │     └── Light Life → (external: lightlifeurbangardens.com)
        ├── Harvesting (collections)
        │     ├── Books → Book Grid
        │     ├── Property → Property Detail → Houses / Gardens / Lands
        │     ├── Blog → Blog Post List → (links to lifes-practice WordPress)
        │     ├── Sexuality → Gallery (images-api, currently broken)
        │     ├── Music → Album Grid → Album Detail (track list)
        │     ├── Photos → Photo Grid (paginated, screenshot filter)
        │     ├── Images → (grayed, not active)
        │     └── Movies → (grayed, not active)
        ├── Reflecting (inner world)
        │     ├── lifes-practice → Blog Post List (same as Harvesting/Blog?)
        │     ├── Journal → (grayed, not active)
        │     └── Notes → (grayed, not active)
        ├── Admin ▾ → (not reviewed)
        └── About → (not reviewed)
```

---

## Page-by-Page UX Review

### 1. Login Page

**Background**: Jeff's fence painting — a self-portrait through his strengths, painted with his mother (an art teacher, age 83) last summer. The flow of images maps to how he experiences them in his body/self. This is the most personal artifact in the product.

**What works**:
- The painting IS the invitation to Gathering — deeply personal, not generic
- Login card is clean and simple

**Issues**:
- [ ] **"Gathering" doesn't appear anywhere** — the product has no name on its front door
- [ ] **Welcome text is implementation, not invitation** — "This site uses SOLID for decentralized data storage" should be replaced with something that matches the painting's intent (e.g., "A place for everything I'm becoming")
- [ ] **Login card is buried** — bottom-right corner, small, fighting the visual dominance of the painting
- [ ] **Two SOLID provider radio buttons** — if Jeff always uses solidcommunity.net, remove the choice
- [ ] **"Home" nav link nearly invisible** — white text on busy background, and you're already home

**Recommendation**: Design the login page *around* the painting. The painting is the invitation. The words need to match what it's already saying.

---

### 2. SOLID Authorization (Pivot)

**What it is**: Standard SOLID Community Server auth page. Out of our control.

**Issues**:
- [ ] **App name says "Jeff Bridwell Personal Site"** — should be "Gathering"
- [ ] **Jarring transition** — from personal painting to generic enterprise auth dialog
- [ ] **"An application is requesting full access"** sounds alarming

**Recommendation**: Change registered app name to "Gathering". Consider a brief interstitial before redirect to smooth the transition.

---

### 3. Profile / Mind Map (Landing Page) ★

**The heart of the product.** This page IS Gathering.

**What works**:
- Jeff at the center of his world — hub node connected to four quadrants
- Graph traversal as UX — exactly what Jeff described wanting
- Four quadrants are clear with good subtitles (ideas & growth, collections, inner world)
- Emojis give each domain visual identity
- Grayed-out items show planned but not active — honest transparency
- Moon + bare branches background continues atmospheric feel

**Issues**:
- [ ] **"Gathering" name collision** — product is called Gathering, quadrant is also Gathering. Quadrant subtitle helps ("capture & seeds") but it's confusing at a glance
- [ ] **Jeff is off-center** — hub node pushed to the left. For a product about "my world," self should be centered
- [ ] **Hub node shows "SolidCommunity User"** sometimes instead of "Jeff Bridwell" — bug
- [ ] **"lifes-practice"** — looks like a URL slug. Should be "Life's Practice" like every other title-cased item
- [ ] **Connection lines barely visible** — thin, low-contrast. Relationships ARE the point of a graph view; they should be the most confident visual element
- [ ] **No click affordance on nodes** — nothing signals these are interactive (no hover state, no cursor change visible)
- [ ] **Nav bar duplicates the mind map** — two navigation systems competing (DEC-021 resolves this: kill the nav bar)

**Decision**: DEC-021 — Remove nav bar. Mind map is primary navigation. Admin/About/Logout become subtle utility links.

**Future**: TheBrain-style active-thought-centered traversal — click a node, it becomes the center, everything re-orients around it. Card #52 tracks this research.

---

### 4. Capture Triage (Gathering quadrant)

**What it is**: Inbox for SMS captures. Text yourself → triage here → route to domains.

**What works**:
- Core text→triage pattern is built and functional
- Tags (PENDING, SMS, PHOTO, JEFF) immediately tell you what/where/who
- Filter tabs (pending/routed/discarded) are clean
- Route to... dropdown with explicit domain destinations

**Issues**:
- [ ] **Only 1 pending item** — inbox for entire life capture system feels empty (Jeff acknowledged: hasn't been dogfooding yet)
- [ ] **Card is 80% empty space** — small thumbnail top-left, huge white void. Content should fill the card
- [ ] **No text content visible** — only media. What about text-only SMS captures?
- [ ] **No context annotation before routing** — when Jeff picks "Route to... Ideas," can he add a note about why? Context in his head at triage time gets lost
- [ ] **"Capture Triage" title is clinical** — functional but doesn't match Gathering's vocabulary
- [ ] **Where are Seeds?** Quadrant says "capture & seeds" but this page only shows triage. Where's the Seeds list view?

**Critical insight**: Jeff said "I need to dogfood it myself — building is taking all my focus." The capture→triage pipeline is built but untested in real life. A "use it" sprint (text yourself for a week, triage daily) would generate better requirements than any brief.

---

### 5. Glimmer List (Cultivating)

**What it is**: Sparks of interest that glow, then either ignite or fade.

**What works**:
- **The metaphor is beautiful** — Glowing / Ignited / Faded. This vocabulary sets Gathering apart from every bookmarking app
- Link previews work well (Deep Water blog, Light Life)
- SMS origin tagging ("SMS from Jeff") connects to capture pipeline
- "4 glowing" count is a nice summary

**Issues**:
- [ ] **"Untitled" with 6 photos, no words** — what was the glimmer? Without annotation, it's a pile of thumbnails with no meaning
- [ ] **"Buddha" — one word** — what did Jeff notice? A glimmer should capture the moment of noticing
- [ ] **Cards are thin** — feel like bookmarks, not glimmers. A glimmer needs a sentence: "I saw this and it made me think about..."
- [ ] **No visual distinction** between SMS-captured and manually-created glimmers

---

### 6. Ideas & Projects (Cultivating)

**What it is**: Ideas that can be promoted to Projects.

**What works**:
- Search + filters (Status, Visibility) are practical
- Link previews are good
- CAPTURED/PRIVATE tags give provenance
- "reading-list" tag emerging organically — good sign
- "Promote" button implies idea→project lifecycle
- Two creation paths: + New Idea and ⚡ Capture

**Issues**:
- [ ] **"https://birgitta.info/"** as title — raw URL, broken/missing link preview
- [ ] **AWS serverless SMS idea** — this is an engineering task, not a Cultivating "idea." Routing problem: some captures are tasks, not ideas
- [ ] **4 ideas, 0 projects** — promote pipeline hasn't been exercised
- [ ] **Cards are title + tags only** — where's the body content? Can you expand them?

---

### 7. Books (Harvesting) ★ Best Collection Page

**What it is**: Personal bookshelf with cover images and physical locations.

**What works**:
- **Cover images make it feel like a real bookshelf** — not a database
- **Physical location tracking** (Library / South / Shelf 1) is uniquely personal — no other app does this
- Grid layout is right for visual browsing
- 19 books, 50 rooms — real content
- ISBNs linked, metadata complete

**Issues**:
- [ ] Minor: All books show same location (Library / South / Shelf 1) — is that accurate or a default?
- [ ] No reading status, notes, or ratings — these are catalog entries, not reading experiences
- [ ] **Cross-domain opportunity**: Some of these books (Hagakure, Heart of Understanding, Tai Chi) connect to Reflecting/lifes-practice. The graph should surface those connections.

---

### 8. Property (Harvesting)

**What it is**: Roslindale property record.

**What works**:
- Rich structured data (address, acreage, purchase date)
- Public record links (City of Boston, Redfin) — useful external connections
- Photos of the house
- Houses (1) / Gardens (5) / Lands (1) tabs — Garden split already in data
- Built 1920, 13 rooms — history and detail

**Issues**:
- [ ] Image placeholders (+ button, dashed "G" box) look unfinished
- [ ] Only one property — feels like a template waiting for more data. But for Jeff's use case (one home), it works.

---

### 9. Blog Posts (Harvesting)

**What it is**: 41 posts harvested from jeffbridwell.blog (lifes-practice WordPress).

**What works**:
- Sync Now button for manual refresh
- 41 posts is real content
- "Read on lifes-practice →" links to original

**Issues**:
- [ ] **"User 1" instead of Jeff Bridwell** — harvester bug
- [ ] **"Category 33" / "Category 34"** — numeric IDs, not category names. Harvester not pulling names from WordPress
- [ ] **HTML entities not decoded** — `&#8217;` (apostrophes), `&nbsp;` (spaces) showing as raw text
- [ ] **Blog looks better in its native dark theme** on lifes-practice than in Gathering's harvested version
- [ ] **These are Reflecting content wearing Harvesting labels** — "Born, never asked," "Fixing a hole" are deeply personal reflective writing about music and identity

**Recommendation**: Fix harvest quality (author, categories, HTML encoding). Consider whether Blog belongs under Reflecting rather than Harvesting — or cross-linked from both.

---

### 10. Gallery / Sexuality (Harvesting)

**What it is**: Image gallery tied to Sexuality domain. Connects to images-api on secondary Mac.

**Status**: **Broken** — "Failed to load images: Failed to load images"

**Issues**:
- [ ] Non-functional. Likely images-api / secondary Mac connectivity issue
- [ ] Should show clear "offline/unavailable" state, not error dump
- [ ] A-Z alphabet filter and search UI exist but have nothing to filter

**Note**: Jeff will discuss Sexuality domain in more depth later.

---

### 11. Music (Harvesting) ★ Most Impressive by Volume

**What it is**: 4,828 albums harvested from Apple Music.

**What works**:
- **Massive collection with album artwork** — visually rich, browsable
- Sort by Artist, Genre filter, Search
- Album detail page: cover art, track list, play counts, duration
- Harvest provenance footer ("Harvested from jeffs-mini-m1-3-lan")
- Play count data is genuinely interesting (tells Jeff what he listens to most)

**Issues**:
- [ ] **Artist names as URL slugs** — "13th-floor-elevators" should be "13th Floor Elevators"
- [ ] **Some albums missing artwork** — music note placeholder
- [ ] **"Unknown year"** on albums that have known years (Easter Everywhere = 1968)
- [ ] **No cross-domain links** — Jeff writes blog posts about music. Albums should link to related blog posts. This is where the graph creates value beyond a catalog.
- [ ] **"Cover" badge** on some tiles is distracting
- [ ] **No album titles visible** in grid view (just album name + artist slug)

---

### 12. Photos (Harvesting)

**What it is**: 9,106 photos from Apple Photos, paginated grid.

**What works**:
- Screenshot filter works — "599 screenshots hidden" with toggle
- Sort by Date, Albums filter, Search
- Pagination exists (7+ pages)
- Real photos visible (selfie, garden, artwork, books, pets)

**Issues**:
- [ ] **Blank/black thumbnails** — still significant number, especially videos
- [ ] **Screenshots still dominate** even with filter — many visible screenshots (app UI, Grafana dashboards, browser windows) aren't categorized as screenshots by Apple
- [ ] **UUID tooltip on hover** (78FE7D67-5D0B-4424...) — debug info leaking to UI
- [ ] **No time grouping** — flat grid with no month/year headers. 9,000+ photos need temporal organization
- [ ] **Mixed content types** — photos, videos, screenshots all in same grid with no visual distinction except play button on videos
- [ ] **Pagination is dark/hard to read** — numbered buttons with no visible numbers

**Note**: Kade has dual read path approved (ADR-010 / CQRS). Photos browse will read SQLite directly. This should fix many data quality issues.

---

### 13. Reflecting Quadrant

**What it is**: Inner world — Notes, Journal, lifes-practice.

**Status**: Mostly inactive. Notes and Journal are grayed. lifes-practice links to the same blog content as Harvesting/Blog.

**Issues**:
- [ ] **Only one active child** (lifes-practice) and it's the same content as Blog under Harvesting
- [ ] **Notes and Journal have no harvester or UI** — they're placeholders
- [ ] **Hub node displays "SolidCommunity User"** instead of Jeff Bridwell (intermittent bug)
- [ ] **Reflecting is the least developed quadrant** — ironic given Jeff's reflective practice (blog, meditation, lifes-practice framework) is some of his most personal content

**Observation**: The lifes-practice WordPress blog is genuinely a Reflecting artifact. Its native dark theme and "Why am I doing this?" header feel more aligned with Reflecting than the harvested list view under Blog/Harvesting.

---

## Cross-Cutting UX Themes

### 1. Graph Connections Are the Missing Value
Every domain is a standalone catalog right now. The transformative moment is when they link:
- Blog post about 13th Floor Elevators → Music album "Easter Everywhere"
- Book "The Cat Who Taught Zen" → lifes-practice blog post about meditation
- Photo of the garden → Property/Gardens section
- Glimmer about Light Life → Light Life node under Cultivating

**This is where Gathering becomes more than the sum of its parts.** Without cross-domain links, it's just several separate apps under one roof.

### 2. Built but Not Used
Jeff acknowledged: "building is taking all my focus — not using it quite yet." The capture→triage→domain pipeline exists but has minimal real content. A "use it" sprint would generate better product requirements than any brief.

### 3. Harvest Quality Varies Widely
- **Books**: Clean, complete, useful
- **Music**: Massive, mostly good, some metadata gaps (years, artist formatting)
- **Photos**: Large but noisy (screenshots, blanks, no time grouping)
- **Blog**: Broken metadata (User 1, Category 33, HTML entities)
- **Gallery**: Non-functional

### 4. The Vocabulary Is Special
Glimmers that glow/ignite/fade. Seeds. Cultivating vs Harvesting. Gathering itself. This vocabulary gives the product a soul that no competitor has. Protect it.

### 5. Nav Bar vs Graph (Resolved)
DEC-021: Kill the nav bar. Mind map is primary navigation. TheBrain-style active-thought-centered traversal is the future pattern (card #52).

---

## Priority Actions from Walkthrough

| Priority | Action | Domain |
|----------|--------|--------|
| **Now** | Fix hub node name (SolidCommunity User → Jeff Bridwell) | Profile |
| **Now** | Fix "lifes-practice" label → "Life's Practice" | Profile |
| **Now** | Fix blog harvest quality (author, categories, HTML encoding) | Blog |
| **Now** | Fix artist name slugs in Music grid | Music |
| **Next** | Redesign login page around the painting + Gathering identity | Login |
| **Next** | Remove nav bar, make mind map nodes clickable navigation (DEC-021) | Profile |
| **Next** | Strengthen connection lines on mind map | Profile |
| **Next** | Add time grouping to Photos browse | Photos |
| **Next** | TheBrain research for graph traversal UX (card #52) | Profile |
| **Later** | Cross-domain graph links (blog↔music, photo↔property, etc.) | All |
| **Later** | Dogfooding sprint: use capture→triage for 1 week | Gathering |
| **Later** | Redesign Reflecting quadrant (most underdeveloped) | Reflecting |
| **Later** | Fix Gallery / images-api connectivity | Sexuality |

---

*Walkthrough conducted 2026-02-18, ~2:40pm–3:05pm ET*
