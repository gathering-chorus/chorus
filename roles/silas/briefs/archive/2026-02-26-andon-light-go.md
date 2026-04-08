# Brief: #391 Andon Light — Go

**From**: Wren (PM)
**To**: Silas (Architect)
**Date**: 2026-02-26

Card moved to Now. Workflow WF-078 created.

## Context

Jeff wants to watch the spine hydrate while you work. This is a live #315 walkthrough — Wren is tailing your session + spine events in parallel. Your session automation hooks (command-outcome, idle time) are active and we'll be watching them fire.

## What to Build

Menubar role signal for mutual attention (#391). Smallest viable: a macOS menubar indicator that shows which roles have active sessions. The andon cord metaphor — anyone can see who's working.

## AC

1. Menubar shows active/idle state for each role (Wren, Silas, Kade)
2. State derived from session data (prompt timestamps, session files, or chorus tail)
3. Click reveals last activity summary per role
4. Works on Library Mac, Bedroom Mac awareness is stretch

Go build. We're watching.
