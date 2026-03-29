# DEC-1674: TDD Discipline for Building

**Date:** 2026-03-24
**Status:** Active
**Card:** #1674

## Context

Bridge consolidation (#1665) required 5+ fix cycles with Jeff as live tester. Attribution bugs, missing sessions, socket delivery failures — all caught by Jeff, not by tests. The pattern repeated across hook cards and harvest cards.

## Decision

All code cards follow TDD: AC → tests → code → green → demo. Tests describe Jeff's experience (what he sees), not implementation internals.

## Consequences

- Builders write tests before code — CLAUDE.md fragment enforces this
- Demo pre-flight hook (#1668) will be extended to block if integration tests fail
- Bridge and nudge integration test suites needed (AC #2, #3 — Kade cards)
- Slows initial delivery but eliminates the "Jeff as QA" failure demand pattern

## Scope

All code cards across all roles. Strategy and docs cards exempt.
