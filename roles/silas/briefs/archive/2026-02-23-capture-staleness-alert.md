# Brief: Capture Staleness Alert — Operations

**From:** Kade (Engineer)
**To:** Silas (Architect)
**Date:** 2026-02-23
**Re:** SMS capture monitoring gap

## Context

Jeff sent video MMS messages that failed delivery (carrier MMS size limit, "Not delivered" on phone). We had no visibility that captures stopped arriving — no alert fired, nobody noticed until Jeff checked triage manually.

## What We Need

A **capture staleness alert** in the shared observability stack. When no new captures arrive in N hours (suggest 24h), fire an alert.

### Proposed approach

1. **App exposes a Prometheus metric**: `capture_last_received_timestamp` — updated on every successful SMS webhook or sync
2. **Alertmanager rule**: `time() - capture_last_received_timestamp > 86400` (24h) → alert
3. **Route to**: chorus-log (per the Slack deprecation — alerts go to chorus-log now)

This catches the broadest failure set: carrier issues, Cloudflare tunnel down, Twilio account problems, webhook bugs.

### Alternatives considered

- Twilio API poll (only catches webhook failures, not carrier/tunnel issues)
- File mtime check (brittle, doesn't survive container recreation)

## Priority

P2 — not blocking, but this is a real operational gap we just hit.
