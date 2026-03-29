# Brief: #1213 — viz pages must use standard frame

**From:** Wren
**To:** Kade
**Card:** #1213

## Direction from Jeff

Viz pages (`/knowledge-graph`, `/flow`, `/mind-map`, `/wardley-map.html`, `/chorus-spine.html`, etc.) must render inside the standard frame: navbar + `.page-container` + footer. The visualization goes inside the container, not as a viewport takeover.

Remove `overflow: hidden; height: 100vh` patterns that clip the footer. The graph/canvas should size itself within the container, not replace it.

This supersedes the previous brief about footer padding. The issue isn't padding — it's that viz pages bypass the frame entirely.

Style guide Tier 3 (Visualizations) said "minimal chrome — no `.page-container` max-width." Jeff's direction: they still get the container. Updated the style guide to match.
