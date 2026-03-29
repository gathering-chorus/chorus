# Brief: #626 PostHog self-hosted — install on Bedroom

**From:** Wren
**Date:** 2026-03-01
**Card:** #626
**Priority:** P2
**Blocked by:** #627 (Done — runway clear)

## Context

Silas shipped #627 — Data Center dashboard with node-exporter on both machines. The dashboard confirms Bedroom is the target:
- 15GB RAM free, 32GB total
- 15.2% disk usage, ~1.5TB free
- Library is at 87% disk and loaded with Gathering + Fuseki + Grafana

## What

Install PostHog self-hosted on Bedroom Mac. Instrument both Gathering app and Chorus surfaces (/werk, /flow, clearing) for session replay + pageviews.

## AC

- PostHog running self-hosted on Bedroom (Docker or native — your call)
- Session replay capturing Gathering app pages
- Session replay capturing Chorus surfaces (/werk, /flow, clearing)
- At least one role can play back a Jeff session
- Bedroom stays healthy (check Data Center dashboard after install)

## Constraints

- Self-hosted only — no usage data leaves the network (concentric trust)
- Start with default instrumentation (session replay + pageviews). Custom events deferred.
- PostHog runs Postgres, Redis, ClickHouse, Kafka — monitor RAM after standing up
- LaunchAgent changes go through Silas if needed (CLAUDE.md rule)

## Why

Jeff is the only one who sees the product experience. Session replay lets all three roles watch real sessions — where he clicks, hesitates, hits friction. Feeds DEC-050 (demo quality) and gives Wren usage data for prioritization.
