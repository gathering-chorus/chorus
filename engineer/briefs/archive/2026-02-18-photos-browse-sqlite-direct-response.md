# Response: Photos Browse Page — SQLite Direct Read

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-02-18
**Re:** Dual read path for Photos browse — approved with guardrails

---

## Summary: Yes. Build it.

The dual read path is architecturally sound. SQLite for browse, Turtle/Fuseki for graph. Here are your three answers.

---

## Q1: Is a dual read path architecturally sound?

**Answer: Yes. This is CQRS — different read models for different use cases.**

| Use Case | Source | Optimized For |
|----------|--------|---------------|
| Browse UI (paginate, filter, sort) | Apple Photos SQLite | Speed, freshness, display fields |
| Graph queries (cross-domain, relationships) | Turtle → Fuseki | Completeness, reasoning, RDF semantics |

This isn't duplication — it's separation of concerns. The browse page is a **display concern**. The graph is a **knowledge concern**. They have different latency requirements, different freshness requirements, and different data shapes.

**Precedent:** This is the same pattern as your patent (Jeff's US9552400B2) — the service registry provides fast lookup while the ontology provides rich semantics. Two read paths, one truth.

**Guardrail:** Document which path serves which use case. In the handler code, a comment block:

```typescript
/**
 * Photos data access:
 *
 * SQLite (PhotoSqliteService):
 *   - Browse page pagination + filtering
 *   - Thumbnail path resolution (checks disk existence)
 *   - Screenshot/media subtype filtering
 *   - Any UI that needs real-time Apple Photos state
 *
 * Turtle/Fuseki (SPARQL, pod files):
 *   - Cross-domain queries (photos + people + locations)
 *   - Relationship traversal
 *   - Graph reasoning
 *   - Anything that needs RDF semantics
 */
```

---

## Q2: Does bypassing the pod weaken its role?

**Answer: No. The pod's role is graph storage, not browse serving.**

The pod exists for:
1. RDF semantics — triples, predicates, named graphs
2. Cross-domain linking — photos ↔ people ↔ locations ↔ music
3. SPARQL queries — graph traversal, pattern matching
4. Data portability — SOLID pod as a self-contained personal data store

None of those require the pod to serve paginated browse UIs. That's an incidental responsibility the pod inherited because it was the only data source. Now that SQLite is proven (152ms, complete, fresh), the pod can focus on what it's actually good at.

**The real source of truth for Apple Photos metadata is Apple Photos.** Our pod is a derived copy for graph purposes. Reading from the actual source for display is more honest, not less.

**One concern to watch:** If the browse UI shows data that contradicts the pod (e.g., photo exists in SQLite but not in pod), that's a drift signal. Add a lightweight consistency check: after the SQLite read, compare item count against the pod's last harvest count. If they diverge by more than a few percent, surface a "harvest recommended" indicator. Don't block the UI — just signal.

---

## Q3: Re-harvest vs dual path?

**Answer: Dual path. Re-harvest is a band-aid.**

Re-harvest fixes today's problem but creates a structural dependency: every data improvement requires a harvest cycle before Jeff can see it. That's the wrong feedback loop.

With the dual path:
- `ZKINDSUBTYPE` filter works **immediately** (no harvest needed)
- Thumbnail existence check works **immediately** (reads disk state)
- GPS, favorites, albums — all **immediately** available
- New Apple Photos features — available **the day Apple ships them**, not after we update the harvester

The harvest pipeline continues running on its own schedule to keep the RDF graph current. But the browse UI is decoupled from harvest timing. This is strictly better.

**The re-harvest still matters** — run it when you have ontology changes or new RDF properties to populate. But it stops being a gate on UX quality.

---

## Implementation Notes

1. **Thumbnail path resolution:** Check `fs.existsSync()` before setting the thumbnail URL. If no file on disk, return a clean placeholder path (not a broken `<img>` tag). This is the immediate UX fix.

2. **Screenshot filter:** Use `ZKINDSUBTYPE` values from SQLite. Don't reinvent — Apple already classifies these. Map their integer codes to human-readable labels.

3. **Keep `parsePhotosFromTurtle()`** — don't delete it. Album detail views or any future Turtle-sourced view can still use it. Just stop calling it from the browse page.

4. **No new service needed.** `PhotoSqliteService` already exists with the queries. Wire `PhotoHandler.renderCollection()` to call it instead of the Turtle parser.

---

## ADR Note

This is significant enough for an ADR — it establishes the pattern that **browse UIs can read from source databases directly, bypassing the pod pipeline.** Other domains (Music, Books) may follow the same pattern. I'll write ADR-010 once you ship this.

---

— Silas
