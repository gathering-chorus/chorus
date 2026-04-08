# Brief: Clearing Mobile Access — C#36

**From:** Wren
**To:** Silas
**Date:** 2026-02-22
**Card:** C#36

## Request

Jeff wants to reach The Clearing from his iPhone and initiate sessions with all three roles — without needing a Claude Code session open first.

## Current Gaps

1. **Localhost binding**: ADR-012 binds all non-app services to 127.0.0.1. The Clearing server can't be reached from LAN (iPhone on WiFi).
2. **Session-dependent launch**: `/clearing` is a Claude Code skill — requires an active session to invoke. No standalone entry point.

## What's Needed

1. **LAN-accessible Clearing server**: Bind to 192.168.86.36 (or 0.0.0.0 with auth). Since ADR-012 exists for security, this needs an auth gate — even a simple shared secret or PIN — before exposing on LAN.
2. **Standalone launcher**: Something Jeff can hit from a phone browser. Could be a simple web page at a known URL that starts a Clearing session and redirects to it. No CLI required.

## Design Considerations

- This is the "direct from anywhere" pattern Jeff keeps asking for. Phone = his primary capture device and his blizzard-day interface.
- Auth matters — don't just open it to the LAN without a gate.
- Keep it simple. A bookmark on his phone home screen that opens a Clearing session would be ideal.
- Consider: should the Clearing run as a persistent lightweight service instead of on-demand? Or keep on-demand with a web-based launcher?

## Priority

P2 — not blocking anything today, but this is a recurring friction point. Jeff's single-piece-flow goal requires phone access.
