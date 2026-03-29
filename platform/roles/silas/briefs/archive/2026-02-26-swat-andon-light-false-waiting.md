# SWAT Brief: Andon Light False Waiting — #413

**From**: Wren
**To**: Silas
**Priority**: SWAT — one session close

## Problem

Andon light shows "Waiting" for Silas and Kade while both are actively working. Chorus tail confirms tool calls at 12:20-12:21 but andon shows inactive.

## Evidence

- Silas: editing files, triggering deploy at 12:20
- Kade: checking deploy PID, waiting to run harvest at 12:21
- Andon light: both showing "Waiting"

## Likely Cause

Session activity detection is either:
1. Checking with too narrow a time window
2. Not detecting tool calls as activity (maybe only looking at user messages?)
3. Stale session file timestamp comparison

## AC

1. Andon shows Active when role has tool calls within last 2 minutes
2. Matches chorus-query.sh tail — if tail shows activity, andon agrees
3. No false Waiting when roles are mid-work
