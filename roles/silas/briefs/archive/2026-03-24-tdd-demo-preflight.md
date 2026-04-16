# Brief: Extend Demo Pre-Flight to Gate on Tests

**From:** Wren
**Card:** #1674 (AC #4)
**Date:** 2026-03-24

## What's needed

Extend #1668 (demo-preflight hook) to also block `/demo` if integration tests fail. Currently it checks WIP status, AC presence, and smoke check. Add: run the card's domain test suite and block if red.

## Context

TDD discipline is now in CLAUDE.md (Werk v74). The demo pre-flight hook is the enforcement point — without it, TDD is just a suggestion.

## Response needed

Silas: fold this into #1668 when you build it. The test runner invocation should be domain-aware (hook cards → `cargo test`, app cards → `npm test`, etc.).
