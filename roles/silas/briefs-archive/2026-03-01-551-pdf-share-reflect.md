# Brief: #551 — Universal PDF + Share + Send to Reflect

**From**: Wren | **To**: Silas | **Card**: #551 (WIP) | **Priority**: P2

## What

Three buttons on all content pages: PDF download, OS share, and Send to Reflect. Kade has the detailed engineering brief in his inbox too (`engineer/briefs/2026-02-28-551-pdf-share-all-pages.md`), but Jeff wants you to start since you built the /self page and know the Reflect endpoint.

## Context

Jeff is already manually sharing content with Reflect — he attached PRODUCT_VISION.pdf yesterday and Reflect couldn't read it. The attach UI exists but the backend doesn't extract file content. This card wires up:

1. **PDF + Share** on all content pages (extend existing html2pdf.js + Web Share API pattern from /self and /about)
2. **Send to Reflect** — button on content cards/docs that sends the text to /self chat as context

## Existing Pattern

- `/self` (views/self.ejs ~357-407): per-card PDF + Share buttons, `.btn-action` / `.action-toolbar`
- `/about` (views/about.ejs ~99-128): whole-doc PDF, no share button yet

## Key Decision

The "Send to Reflect" button should POST the content as text, not as a file attachment. Reflect doesn't need to parse a PDF — just inject the markdown/text as context in the conversation. This is simpler than #562 (file reading) and solves the immediate need.

## AC
- Every content page has PDF + Share buttons (consistent styling)
- About docs and story cards have "Send to Reflect" that opens /self with content pre-loaded
- Views are bind-mounted — EJS/CSS changes are live. Only TypeScript needs deploy.
- All scripts use CSP nonce

## Also

#562 (Reflect reads attached files) is the companion card — when Jeff uses the existing attach UI, extract text server-side so Mistral can read it. Separate card, but related. Start with #551 (the outward share + Send to Reflect), then #562 is the natural next step.
