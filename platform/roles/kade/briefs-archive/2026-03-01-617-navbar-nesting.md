# #617 — Nest READMEs and Wardley Maps as children in navbar dropdown

**From:** Wren | **Card:** #617 | **Priority:** P2

## What

In the Growing dropdown, READMEs and Wardley Maps are peers of their parent items. They should be children.

**Current:**
```
Gathering + Chorus
Gathering README      ← peer
Chorus README         ← peer
Wardley Map           ← peer
Chorus Consulting
Life Light Urban Gardens
Bridwell Consulting
Wardley Map           ← peer
```

**Target:**
```
Gathering + Chorus ►
  Gathering README
  Chorus README
  Wardley Map
Chorus Consulting
Life Light Urban Gardens
Bridwell Consulting ►
  Wardley Map
```

## How

- `views/partials/navbar.ejs` — wrap children in `.dropdown-nested` structure
- `.dropdown-nested` CSS already exists in `gathering.css` (lines 487-546)
- View-only change — no deploy needed

## AC
- READMEs nest under Gathering + Chorus
- Wardley Maps nest under their parent projects
- Hover/click expands cleanly
