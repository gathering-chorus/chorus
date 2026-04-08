# Brief: Deploy Visibility + Session Persistence

**From:** Wren (PM)
**To:** Silas (Architect)
**Date:** 2026-03-02
**Priority:** P1 — Jeff hits this daily

## Context

Jeff observed that deploys kill his auth session and he has no visibility into when deploys happen. Combined with 28-second SOLID auth round-trips, every deploy costs him real time and creates diagnostic confusion ("is this a bug or did someone deploy?"). This is failure demand — Jeff absorbing diagnostic cost that the system should handle.

You found yesterday that `app-state.sh` isn't emitting logs for deploys. That's the first fix.

## Two Problems

### 1. Deploy Logging (quick fix)

`app-state.sh deploy` should emit a spine event on every deploy:

```
chorus-log.sh deploy.completed <role> sha=<SHA> duration=<seconds>
```

This gives us:
- Boot summary shows last deploy time
- Activity stream shows who deployed and when
- We can correlate Jeff's "weird behavior" reports with deploy timing
- Foundation for deploy frequency tracking

### 2. Session Persistence Across Deploys (investigation)

Jeff's SOLID auth session is lost on every container cycle. Questions:

- What is the current Express session store? (in-memory default? connect-redis? file?)
- If in-memory, sessions die with the container — that explains the forced re-login
- What would it take to persist sessions across deploys? (Redis is already running for other things, or a file-based store mapped to a bind mount)
- What's the blast radius of changing the session store?

This is the higher-value fix. If sessions survive deploys, the 28-second auth penalty only happens once per browser session, not once per deploy.

## Ask

1. **Fix deploy logging first** — quick win, immediate visibility
2. **Investigate session store** — report back what we're using and what the options are before making changes

Don't block on #2 to ship #1.
