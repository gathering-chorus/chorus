# Brief: Deploy system-model-thinking page into About section

**From:** Wren | **To:** Kade | **Priority:** P2

## What

Wren wrote a creative thinking page for the system model (`system-model-thinking.html`). It needs to be deployed into the About section. Changes already made:

1. **HTML with OG share tags** — `public/gathering-docs/system-model-thinking.html` (already live as static file)
2. **Handler updated** — `src/handlers/about.handler.ts`:
   - Added `SYSTEM_MODEL` to SECTION_MAP under 'Strategy & Vision'
   - Added hardcoded link to thinking page under 'Product' section
3. **Source copy** — `data/about/system-model-thinking.html` (canonical copy with OG tags)

## What's Needed

`npm run build` + `app-state.sh deploy` to pick up the handler changes. The static file is already serving.

## Share Tags Pattern

This is the first page with OG meta tags. The pattern for future pages:

```html
<meta property="og:title" content="Page Title" />
<meta property="og:description" content="Description" />
<meta property="og:type" content="article" />
<meta property="og:url" content="https://jeffbridwell.com/path" />
<meta name="twitter:card" content="summary" />
<meta name="twitter:title" content="Page Title" />
<meta name="twitter:description" content="Description" />
```

Standalone HTML pages add tags in their own `<head>`. Template-rendered pages will need `page-head.ejs` extended (future card).

## Verify After Deploy

- `/gathering-docs/system-model-thinking.html` renders (already works)
- `/about` shows SYSTEM_MODEL under Strategy & Vision
- `/api/about-sections` includes thinking page under Product hardcoded links
