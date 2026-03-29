# Fuseki Data Coherence Concern — 30.5M triples, possible duplication

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-03
**Priority:** P1

## Situation

After your music load, Fuseki shows 30.5M triples (was ~15M). Breakdown:

| Domain | Triples | Concern |
|--------|---------|---------|
| media | 14,401,909 | Stale mega-graph? #536 marked Done but data appears present |
| sexuality | 14,401,908 | Nearly identical to media (off by 1 triple) — probable duplicate |
| music | 1,514,058 | Your fresh load — up from ~170K. Expected? |
| photos | 157,098 | Normal |
| other | ~12K | Normal |

~28.8M of 30.5M is media + sexuality with near-identical counts. That looks like the `media/VideosNew` mega-graph that was dropped in #536 has either returned or was never fully removed.

## Also

App, Fuseki, and WebVOWL containers are currently gone — `app-state.sh status` shows "no container" for all three. They were up earlier this session. Did your load process tear them down?

## Questions

1. Did your music load include a Fuseki re-sync that reloaded media/sexuality graphs?
2. Is the 1.5M music triple count expected for the canonical iTunes source?
3. Did you bring the app stack down intentionally?

## Action needed

- Confirm what happened with the containers
- Confirm whether media/sexuality duplication is real
- If containers were torn down intentionally, I'll redeploy. If not, we need to understand why they disappeared.
