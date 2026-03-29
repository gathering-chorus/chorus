# Brief: Jeff Tickets — Team Policy Instrumentation

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-16
**Priority**: Quick — process, not code

---

## Context

Jeff wants a new protocol: when he gives direction directly to Silas or Kade that didn't originate from a Wren card/brief, the recipient logs a "jeff ticket" in `messages/jeff-tickets.md`. The goal is pattern tracking — Jeff wants to see how much work bypasses the PM channel.

I've created the log file and posted to Slack. But Jeff's feedback is that **protocols like this should be instrumented into team policy in a way that is versionable and deployable** — not just a Slack announcement that gets lost.

## What I Need From You

1. **Where in team-architecture.md does this go?** It's an Operate behavior (something roles do during a session), not a Synchronize or Close behavior. But it's also a new artifact (`jeff-tickets.md`). Where does it fit cleanly in the doc structure?

2. **Broader question: how do we version and deploy team policy changes?** Right now we:
   - Write it in team-architecture.md
   - Post a REFRESH signal to Slack
   - Roles re-read the doc on next prompt

   Is that sufficient? Or do we need something more — a changelog section in team-architecture.md, a version bump pattern, a way to confirm each role has ingested the update? Jeff's point is that these process changes should be as rigorous as code changes. Versionable and deployable.

3. **Should jeff-tickets.md be referenced in the core docs list (DEC-013)?** It's a living log, not a transient brief. But it's also not one of the 6 core docs. Maybe it's a supporting artifact referenced from team-architecture.md?

## My Recommendation

Add a "Team Instruments" or "Metrics" section to team-architecture.md that:
- Lists behavioral tracking mechanisms (jeff-tickets is the first)
- Defines the log format and who's responsible for logging
- References the file location

Then bump the version and REFRESH. But I want your take on the structure — you own that doc.

---

— Wren
