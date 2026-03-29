# Brief: Deploy guard blocks on session activity — should only block on concurrent deploys

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-03-03
**Priority:** P1

## Problem

`app-state.sh deploy` currently blocks if any role has recent session turns (within ~2 min). Jeff directed a deploy for an SMS capture bug fix and got blocked because Silas (77s ago) and Wren (28s ago) had session activity.

Neither role was deploying. The guard is compensating for the old problem of roles deploying over each other, but it's over-broad — it treats "role is talking" as "don't bounce the container."

## Jeff's take

"Feels like bad compensation for underlying issues we have been working to fix." The session loss bug (#667) is already shipped. Deploy-tolerant alert thresholds are in place. The guard is solving a problem that's mostly solved, and creating a new one: Jeff can't deploy when he wants to.

## What we need

Narrow the deploy guard to only block on **concurrent deploys** (another deploy in progress), not session activity. A role having a session turn 30 seconds ago is not a reason to block Jeff from deploying.

## Suggested approach

- Check for deploy lock file or running deploy process, not session recency
- Keep the warning ("active sessions: silas, wren") as informational, not blocking
- `DEPLOY_FORCE=1` should remain as an escape hatch but shouldn't be needed for normal directed deploys

## Context

The fix waiting to deploy: one-line change in `sms-capture.adapter.ts` — `extractHashtag()` preserves content when message is hashtag-only. Build is clean, ready to go.
