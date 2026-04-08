# Spike: Service Redistribution — Library to Bedroom

**Card:** #1432 | **Date:** 2026-03-16 | **Author:** Silas

## TL;DR

**Don't move anything.** The Docker-to-native migration already solved the memory pressure. Library runs 30 LaunchAgents at ~2.5GB total — healthy on 16GB. Moving observability to Bedroom saves 623MB but adds LAN-hop latency and split-brain failure modes. Not worth it.

## Current State

Docker is **off** on Library. All 18 former containers migrated to native LaunchAgents. This is a significant change from what `infrastructure-constraints.md` documents (still says 18 Docker containers).

### Library Memory Budget (native)

| Service | RSS | Category |
|---------|-----|----------|
| Fuseki (Xmx2g) | 889MB | Core |
| Gathering App (Node) | 708MB | Core |
| Grafana | 187MB | Observability |
| Prometheus | 147MB | Observability |
| Loki | 121MB | Observability |
| Promtail | 65MB | Observability |
| WordPress | 58MB | Content |
| MySQL | 55MB | Content |
| Vikunja | 50MB | Coordination |
| CSS (SOLID Server) | 42MB | Core |
| PHP | 39MB | Content |
| Blackbox Exporter | 34MB | Observability |
| Alertmanager | 27MB | Observability |
| Node Exporter | 23MB | Observability |
| mysqld Exporter | 19MB | Observability |
| Chorus API | 12MB | Coordination |
| Clearing | 3MB | Coordination |
| **Total** | **~2,479MB** | |

Non-Gathering (Chrome ~1.2GB, Claude sessions ~680MB, WindowServer ~153MB, OS ~2GB) brings total to ~6.5GB of 16GB. **~9.5GB headroom.**

### Bedroom State (32GB M2 Pro)

- 4 Gathering LaunchAgents (images-api x2, volume-keepalive, ollama)
- Load: 2.47 on 12 cores — plenty of headroom
- 1.6TB free internal SSD
- Top non-Gathering memory: ApolloOne (1.2GB), MacKeeper AV (657MB), Firefox (1.3GB), Spotlight (770MB)

## Move Candidates Evaluated

### Observability Stack → Bedroom

| Metric | Value |
|--------|-------|
| Memory freed on Library | 623MB |
| Services affected | 7 (Prometheus, Grafana, Loki, Promtail, Alertmanager, Blackbox, Node Exporter, mysqld Exporter) |
| Latency impact | Dashboards add ~3ms LAN hop. Scrape targets become cross-network. |
| Risk | Split-brain: if network drops, monitoring goes blind while services stay up. Prometheus scraping localhost is zero-latency; scraping 192.168.86.36 from Bedroom adds network dependency. |
| **Verdict** | **Don't move.** 623MB savings doesn't justify added failure mode. |

### WordPress + MySQL → Bedroom

| Metric | Value |
|--------|-------|
| Memory freed on Library | 171MB |
| Latency impact | Negligible for a blog. |
| Risk | Low — WordPress is independent. But it adds ops surface on Bedroom. |
| **Verdict** | **Not worth the complexity for 171MB.** |

## Better Optimizations (no redistribution needed)

1. **Fuseki heap reduction**: Configured at 2GB (`-Xmx2g`), using 889MB. Reduce to `-Xmx1.5g` → 500MB recovered headroom. Low risk — current working set fits.
2. **App memory investigation**: 708MB RSS for an Express app seems high. Could be query result caching or a leak. Worth profiling.
3. **Update infrastructure-constraints.md**: Still documents 18 Docker containers. Reality is 30 native LaunchAgents, zero Docker. This doc is the most stale in the system.

## Recommendation

| Action | Priority | Memory Impact |
|--------|----------|--------------|
| Update infrastructure-constraints.md to reflect native reality | Now | — |
| Reduce Fuseki Xmx from 2g to 1.5g | Next session | +500MB headroom |
| Profile app Node.js memory | Later | TBD |
| Revisit redistribution if Library hits 85% memory | Trigger-based | — |

**Stay list:** Everything stays on Library. Bedroom stays as storage + media serving (C4).
**Move list:** Empty. No redistribution needed.
