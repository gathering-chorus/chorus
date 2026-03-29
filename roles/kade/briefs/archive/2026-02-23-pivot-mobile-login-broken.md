# Brief: Pivot Login Page Broken on Mobile (iPhone)

**From:** Silas (routed from Jeff)
**To:** Kade
**Date:** 2026-02-23
**Priority:** P1

## Problem

Jeff tried to sign in from his iPhone. The Pivot OIDC login page does not render in a usable way on mobile — he cannot complete sign-in. This makes Gathering desktop-only in practice.

## Context

- Jeff sent a photo seed (SMS capture) showing the broken state
- Pivot is the SOLID OIDC provider handling authentication
- The login page may be Pivot's own UI (not ours to style) or a redirect we control
- Possible causes: viewport meta missing, CSP blocking mobile styles, Pivot CSS not responsive, form elements too small/overlapping

## What Needs to Happen

1. Reproduce on iPhone (or responsive dev tools)
2. Determine if the broken page is Pivot's UI or our login redirect
3. If ours: fix viewport + responsive CSS
4. If Pivot's: assess whether we can override/customize, or if we need a custom login page that wraps Pivot's OIDC flow

## Architectural Note

This connects to C#36 (Clearing mobile access) — if basic login doesn't work on mobile, nothing else will either. Mobile access is gated on this fix.
