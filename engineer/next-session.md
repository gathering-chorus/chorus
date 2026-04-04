# Kade — Next Session

## Accomplished 2026-04-04
- #2005: foaf prefix added to COMMON_PREFIXES, service.down warn→error in shim.rs
- #2017: Bad URI graph load errors verified resolved by #1995
- #2007: /cs shows photo seeds with media URLs, roles read and describe images
- #1959: Domain crawler v2 — code scan + Loki logs + alerting rules per domain
- #2019: Crawler snapshots indexed into Chorus search for compound loop discovery
- Gemba on Silas: #2009, #2010, #2011, #2008, #2003 — caught perf-baseline false positive, binary path leak, stop word issue
- Reviewed Wren's RUNBOOK.html — 7 corrections sent

## WIP
None.

## Next cards (my queue)
- #1865 Photo detail shows thumbnail instead of full image (P2)
- #1631 Name face clusters (P3)
- #1630 Rebuild semantic embeddings (P2)
- #1619 Stamp records with source-chain provenance (P2)

## Pending
- Verify com.chorus.crawler-index LaunchAgent is running (Silas set up hourly at :15)
- Verify crawler snapshots appear in context_inject results after first hourly run
- 3 pre-existing Rust test failures in search_hierarchy — not mine but worth investigating

## Notes
- shim.rs service health checks now error-level (was warn) — monitor for false positives
- /cs skill updated to instruct roles to read photo images and describe them
- agent-state.sh is the new tool for LaunchAgent lifecycle (replaces raw launchctl)

## Session feedback
- Jeff was mad about #2007 — seeds showing routing tags instead of content. That's the system failing to receive what he gave it. Treat seed content with the same care as a person receiving a message.
