# Brief: #551 — Universal PDF + Share + Send to Reflect

**From**: Wren | **To**: Kade | **Card**: #551 | **Priority**: P2

## What

Extend the PDF download + share buttons to all content pages, and add a new "Send to Reflect" action that pushes content into the /self chat.

## Why

Jeff is already manually copying content into Reflect to give it context. This automates that pattern. The outward share (PDF + OS share sheet) makes every page portable. The inward share (Send to Reflect) lets Jeff ground Reflect with real product docs and stories.

## Existing Pattern (reuse this)

Two implementations already exist:

### /self (views/self.ejs, lines ~357-407)
- **PDF**: html2pdf.js v0.10.2 (CDN), per-card generation, `.btn-pdf` on `.action-toolbar`
- **Share**: Web Share API (`navigator.share`), graceful degradation, `.btn-share`
- **UI**: 28px action buttons, absolute positioned top-right of cards
- **Print**: `@media print { .action-toolbar { display: none !important; } }`

### /about (views/about.ejs, lines ~99-128)
- **PDF**: Same html2pdf.js, single "PDF" text button, captures `#doc-content`
- **Share**: Not implemented yet
- **Pagebreak**: `['avoid-all', 'css', 'legacy']` — better for long docs

## What to Build

### 1. Normalize the pattern
Create a shared partial or inline snippet that any view can include. Consistent button styling (`.btn-action`, `.action-toolbar`). Both PDF + Share on every content page.

### 2. Pages that need buttons added
- `/about/:slug` — add Share button (PDF already exists)
- Story cards on domain pages (music, photos, sexuality, blog, etc.)
- `/profile`
- Any page with content cards

### 3. Send to Reflect (new)
A third button on content cards and docs that:
- POSTs the card/doc text content to the /self chat endpoint as context
- Opens /self in a new tab (or navigates) with the content pre-loaded
- Use case: Jeff reads a product doc, clicks "Send to Reflect", Reflect now has that context for conversation

The endpoint details — check how /self currently receives messages. The send action should inject the content as a user message or system context so Reflect can reference it.

## Acceptance Criteria
- Every content page has PDF download button (html2pdf.js, A4, consistent styling)
- Every content page has OS share button (Web Share API, graceful degradation)
- About docs and story cards have "Send to Reflect" button that opens /self with content pre-loaded
- Buttons use consistent .btn-action / .action-toolbar pattern
- Print media query hides all toolbars
- All inline scripts use CSP nonce (`nonce="<%= locals.cspNonce %>"`)

## Technical Notes
- html2pdf.js CDN: `https://cdn.jsdelivr.net/npm/html2pdf.js@0.10.2/dist/html2pdf.bundle.min.js`
- Views are bind-mounted — no deploy needed for EJS/CSS changes
- If you need a new API endpoint for Reflect context injection, that's TypeScript → deploy required
- CSP allows `cdn.jsdelivr.net` only

## When
After current harvest batches settle. This is Now, not urgent.
