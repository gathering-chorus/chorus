# Style lint findings — 29 warnings, 2 failures

Run `bash ../product-manager/scripts/style-lint.sh` to reproduce.

## Failures (404)
- /music/harvest — route missing (#1207)
- /photos/harvest — route missing (#1207)

## Warnings by category

### no-footer (7 pages)
/model-data-hub, /garden-observability, /practice-spine, /voice-analytics, /chorus-model-data, /gathering-docs/claude-hooks, /v1-acceptance-criteria.html, /gathering-docs/next-sequence

### expected-dark-theme (8 pages)
Manifest says dark but page doesn't render dark:
/knowledge-graph, /codebase-graph, /self, /photos, /notes/harvest, /flow, /practice-spine.html, /wardley-map.html, /chorus-spine.html, /self-ontology-sketch.html, /self-relationships.html

### expected-doc-chrome-nav (14 pages)
Manifest says doc-chrome but page doesn't use it:
/about/GATHERING_README, /about/CHORUS_README, /about/EMERGENT_ARCHITECTURE_PAPER, /about/INTERACTION_PATTERNS, /about/DECISIONS, /about/NUDGE_BRIDGE, /operations.html, /practice-spine.html, /wardley-map.html, /chorus-spine.html, /werk-process.html, /self-ontology-sketch.html, /self-relationships.html, /value-stream-render.html, /attention-analytics.html

## Gate
#1171 stays open until `style-lint.sh` passes with 0 FAIL and warnings addressed.
Cards: #1207 (harvest 404s), plus existing cards #1195-1200 overlap with some of these.
