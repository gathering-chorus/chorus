# Brief: Vision Pages + Public Hosting

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-26
**Re**: Three new static pages + lightlifeurbangardens.com hosting question

## Context

Jeff articulated his revenue strategy this morning (voice message, transcribed). Three containers:

1. **Gathering + Chorus** — the platform as product (open source / managed / consulting)
2. **Light Life Urban Gardens** — collaborative garden work, 2-4 people, seasonal
3. **Chorus Consulting** — local tech consulting for small businesses, AI-augmented

I built three static HTML pages, all in `jeff-bridwell-personal-site/public/`:
- `gathering-chorus.html` — blue palette, platform positioning
- `lightlife.html` — updated, deepened with Jeff's intentions (was just a landing card before)
- `chorus-consulting.html` — slate palette, consulting business

## Ask

**Jeff wants to share Light Life publicly at `lightlifeurbangardens.com`.** What's the simplest path?

Options I see:
- Cloudflare Pages or similar static host (no Docker needed)
- Subdirectory served via the existing Cloudflare tunnel
- Separate lightweight hosting (GitHub Pages, Netlify)

The page is fully self-contained — no server rendering, no dependencies beyond Google Fonts. It should be hostable anywhere.

Secondary question: should all three pages eventually be publicly reachable? If so, we need a hosting strategy for the consulting and platform pages too.

## Related
- DEC-057 — Product maturity threshold (the system needs hardening before consulting clients touch it)
- #3 — Vision refinement (WIP, Wren)
- #92 — Revenue strategy (updated with full model)
