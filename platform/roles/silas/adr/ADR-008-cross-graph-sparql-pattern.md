# ADR-008: Cross-Graph SPARQL Query Pattern

**Date**: 2026-02-17
**Status**: Accepted
**Decider**: Silas (architectural pattern), validated by Kade (implementation)
**References**: Music harvester (#47), ADR-003 (visibility enforcement)

## Context

The Gathering knowledge graph stores resources in **named graphs** — one graph per resource (album, artist, track, book, idea, etc.). This design supports per-resource ACLs (ADR-003), provenance tracking, and selective loading.

During the music harvester implementation (5,844 albums, 54,331 tracks, 7,327 artists), Kade discovered that SPARQL queries joining data across resources require explicit multi-graph patterns. A single `GRAPH ?g { ... }` block cannot join data from different named graphs.

This pattern will affect every harvester and every browse view that displays related entities.

## Problem

Given this data distribution:

```
Graph: /music/albums/beatles/abbey-road
  <album:abbey-road> a jb:Album ;
      dcterms:title "Abbey Road" ;
      jb:albumArtist <artist:beatles> .

Graph: /music/artists/beatles
  <artist:beatles> a jb:Artist ;
      dcterms:title "The Beatles" .
```

This query **fails to return the artist name**:
```sparql
# WRONG — single GRAPH block can't cross graph boundaries
SELECT ?title ?artistName WHERE {
  GRAPH ?g {
    ?album a jb:Album ;
           dcterms:title ?title ;
           jb:albumArtist ?artist .
    ?artist dcterms:title ?artistName .
  }
}
```

The `?artist dcterms:title ?artistName` triple is in a different graph than the album triples. A single `GRAPH ?g` binds to one graph at a time.

## Decision

Use **multiple GRAPH blocks with shared variables** for all cross-resource queries. Each entity type gets its own GRAPH block. Variables shared between blocks create the join.

### The Pattern

```sparql
SELECT ?title ?artistName WHERE {
  GRAPH ?g1 {
    ?album a jb:Album ;
           dcterms:title ?title ;
           jb:albumArtist ?artist .
  }
  GRAPH ?g2 {
    ?artist dcterms:title ?artistName .
  }
}
```

**How it works:**
- `GRAPH ?g1` iterates over all graphs looking for Album triples
- `GRAPH ?g2` iterates over all graphs looking for the artist URI bound by `?artist`
- The shared `?artist` variable creates the join between the two graph blocks
- Fuseki optimizes this — it doesn't do a full cross-product

### Rule of Thumb

**One GRAPH block per entity type being queried.** If your query touches albums and artists, you need two GRAPH blocks. If it touches albums, artists, and genres, you need three.

## Examples

### Album → Artist (Music)
```sparql
SELECT ?albumTitle ?artistName WHERE {
  GRAPH ?g1 {
    ?album a jb:Album ;
           dcterms:title ?albumTitle ;
           jb:albumArtist ?artist .
  }
  GRAPH ?g2 {
    ?artist dcterms:title ?artistName .
  }
}
```

### Album → Tracks with per-track Artist (Compilations)
```sparql
SELECT ?albumTitle ?trackTitle ?trackArtistName WHERE {
  GRAPH ?g1 {
    ?album a jb:Album ;
           dcterms:title ?albumTitle ;
           jb:hasTrack ?track .
  }
  GRAPH ?g2 {
    ?track dcterms:title ?trackTitle ;
           jb:byArtist ?trackArtist .
  }
  GRAPH ?g3 {
    ?trackArtist dcterms:title ?trackArtistName .
  }
}
```

### Book → Room (Cross-Collection, Future)
```sparql
SELECT ?bookTitle ?roomName WHERE {
  GRAPH ?g1 {
    ?book a jb:Book ;
          dcterms:title ?bookTitle ;
          jb:onShelf ?shelf .
  }
  GRAPH ?g2 {
    ?shelf jb:inBookcase ?bookcase .
    ?bookcase jb:inRoom ?room .
  }
  GRAPH ?g3 {
    ?room dcterms:title ?roomName .
  }
}
```

### Capture → Routed Destination (Cross-Collection)
```sparql
SELECT ?captureText ?destTitle ?destType WHERE {
  GRAPH ?g1 {
    ?capture a jb:CaptureItem ;
             jb:captureText ?captureText ;
             jb:routedTo ?dest .
  }
  GRAPH ?g2 {
    ?dest dcterms:title ?destTitle ;
          a ?destType .
  }
}
```

## Alternatives Considered

### 1. Default graph (no named graphs)
Put all triples in one graph. Simplest queries but loses isolation, per-resource ACLs, and provenance. **Rejected** — incompatible with visibility model (ADR-003).

### 2. Denormalized summary graphs
Maintain a separate "browse" graph with pre-joined data (album title + artist name in one graph). Faster reads but creates sync complexity and data duplication. **Deferred** — not needed at current scale. Reconsider if query latency becomes a problem at 1M+ triples.

### 3. SPARQL property paths across graphs
Some SPARQL engines support property paths that cross graph boundaries. Fuseki's behavior here is implementation-dependent and not portable. **Rejected** — prefer explicit, predictable patterns.

## Consequences

### Positive
- Query pattern is **reusable** — template works for all harvesters and browse views
- **ACL-compatible** — each GRAPH block respects its graph's access rules
- **Clear mental model** — "one GRAPH block per entity type"
- **Fuseki handles it efficiently** at current scale (54k triples, sub-second response)

### Negative
- **More verbose queries** — every relationship traversal adds a GRAPH block
- **Learning curve** — developers must understand graph boundaries
- **Performance monitoring needed** — multiple graph lookups per query, needs tracking as dataset grows

## Performance Expectations

| Dataset Size | Expected Behavior | Action |
|-------------|-------------------|--------|
| < 100k triples | Sub-second, no concerns | Current state |
| 100k - 1M triples | Likely fine, monitor P95 | Photos harvester range |
| 1M - 5M triples | May need Fuseki text index for search queries | Add `text:query` for full-text |
| 5M+ triples | Benchmark cross-graph joins, consider summary graphs | Revisit Alternative #2 |

The performance baseline script (engineer/briefs/2026-02-17-performance-baseline.md) will track cross-graph query latency at each scale milestone.
