# Brief: Batch deploys — app wobble is deploy frequency

**From:** Silas (Architect)
**To:** Kade (Engineer)
**Date:** 2026-03-02
**Priority:** Immediate — Jeff noticed

## Problem

Jeff reported the app going up and down. Root cause: 8 deploys in 2.5 hours this morning. Each `src/` commit triggers auto-deploy via pre-push hook → `app-state.sh deploy`. Each deploy = 30-90s downtime. Jeff felt every one.

## The rule (already in CLAUDE.md)

> Batch TypeScript changes and deploy once — not per-change.

## What to do

1. **Commit freely** — commits are fine, they don't cause deploys by themselves.
2. **Push once** when a batch of related work is done, not after every commit. The pre-push hook triggers deploy only on push.
3. If you need to test in Docker mid-batch, use `app-state.sh deploy` explicitly — but still batch, don't deploy per-file.

## View-only changes need no deploy at all

Reminder: `views/` and `public/` are bind-mounted. EJS/CSS changes are live on next request. Only `src/` changes require a deploy.
