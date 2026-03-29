# Brief: Glimmer List + Style Guide

**From**: Wren (PM)
**To**: Kade (Engineer)
**Date**: 2026-02-15
**Priority**: P2 — triage UX and visual consistency
**Context**: Jeff tested the triage page and had two findings: (1) needs a "Glimmer List" routing destination, and (2) pages need a shared style guide so they hold together visually.

---

## 1. Glimmer List — New Routing Destination

### What it is

A glimmer is something bright, fleeting, maybe random — worth reviewing from time to time because maybe there's fire. It's pre-idea: not formed enough for Ideas, too interesting to discard, not specific enough for Read List or Watch List.

Think of it as: "I don't know what this is yet, but it sparkled."

Jeff's words: "a glimmer is something bright yet maybe random and fleeting — worth review from time to time — maybe there is fire."

### What to build

**Add to triage dropdown** in `capture-triage.ejs` line 410-413:
```html
<optgroup label="Collections">
    <option value="ideas">Ideas</option>
    <option value="projects">Projects</option>
    <option value="reading-list">Reading List</option>
    <option value="watch-list">Watch List</option>
    <option value="glimmers">Glimmer List</option>
</optgroup>
```

**Backend routing**: Same pattern as reading-list/watch-list. Route the capture to a `/glimmers/` collection in the pod. If that collection doesn't exist yet, create it on first route (same pattern as other collections).

**Glimmer List view**: Not needed in v1. Jeff can browse glimmers through the triage page (filter by routed → glimmers) or through the pod directly. A dedicated view can come later if the collection grows.

### Design note

The glimmer pattern maps to Jeff's garden metaphor: seeds you pick up and put in your pocket. Some sprout, some don't. The Glimmer List is the pocket.

---

## 2. Style Guide — Shared CSS

### Problem

Every page defines its own `<style>` block with its own CSS variables. The triage page uses `--accent-color: #6366F1` (indigo), the ideas page uses `--primary-color: #3498db` (blue). Fonts, spacing, card styles, button styles all vary per page. It doesn't feel like one app.

### What to build

Create `public/css/gathering.css` — a shared stylesheet that all pages include. Not a framework, just enough to hold things together.

**Design tokens** (use the triage/dashboard set as the baseline — it's the most polished):

```css
:root {
    /* Typography */
    --font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
    --font-size-base: 15px;
    --font-size-sm: 13px;
    --font-size-lg: 18px;
    --font-size-xl: 24px;
    --line-height: 1.6;

    /* Colors */
    --color-primary: #1A202C;       /* Text / headings */
    --color-accent: #6366F1;        /* Indigo — interactive elements */
    --color-accent-hover: #4F46E5;  /* Darker indigo */
    --color-success: #10B981;       /* Emerald — confirmations */
    --color-warning: #F59E0B;       /* Amber — cautions */
    --color-danger: #EF4444;        /* Red — destructive */
    --color-muted: #718096;         /* Secondary text */

    /* Surfaces */
    --bg-page: #F7FAFC;            /* Page background */
    --bg-card: #FFFFFF;            /* Card background */
    --bg-card-hover: #FAFBFC;     /* Card hover */
    --border-color: #E2E8F0;      /* Borders */
    --border-radius: 0.5rem;      /* Card corners */
    --border-radius-sm: 0.25rem;  /* Badge corners */

    /* Spacing */
    --space-xs: 0.25rem;
    --space-sm: 0.5rem;
    --space-md: 1rem;
    --space-lg: 1.5rem;
    --space-xl: 2rem;

    /* Shadows */
    --shadow-sm: 0 1px 3px rgba(0,0,0,0.08);
    --shadow-md: 0 2px 8px rgba(0,0,0,0.12);
}
```

**Shared component styles** (card, button, badge, nav):

```css
/* Cards */
.card {
    background: var(--bg-card);
    border: 1px solid var(--border-color);
    border-radius: var(--border-radius);
    padding: var(--space-lg);
    box-shadow: var(--shadow-sm);
}

/* Buttons */
.btn {
    padding: var(--space-sm) var(--space-md);
    border-radius: var(--border-radius-sm);
    font-size: var(--font-size-sm);
    font-weight: 500;
    cursor: pointer;
    border: 1px solid transparent;
}
.btn-primary { background: var(--color-accent); color: white; }
.btn-primary:hover { background: var(--color-accent-hover); }
.btn-danger { background: var(--color-danger); color: white; }
.btn-secondary { background: #EDF2F7; color: var(--color-primary); border-color: var(--border-color); }

/* Badges */
.badge {
    display: inline-block;
    padding: 2px 8px;
    border-radius: 999px;
    font-size: var(--font-size-sm);
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.5px;
}
```

### Migration approach

1. Create `public/css/gathering.css` with tokens + shared components
2. Add `<link rel="stylesheet" href="/css/gathering.css">` to `partials/head.ejs`
3. Gradually remove duplicate inline styles from each view as you touch them
4. Don't rewrite all 18 views at once — migrate opportunistically

### What this is NOT

- Not a design system with documentation
- Not a CSS framework
- Not a complete restyling of every page
- Just enough shared tokens and components to make pages feel like siblings

---

## Sequencing

1. **Glimmer List** (~15 min) — dropdown change + backend routing
2. **Style guide CSS** (~30 min) — create shared file, wire into head partial
3. **Migrate triage page** (~15 min) — swap inline tokens for shared CSS, verify nothing breaks

The style guide migration of other pages happens organically as they're touched.

— Wren
