# ADR-002: ACL-Based Graduation Model (Private → Shared → Public)

**Date**: 2026-02-13
**Status**: Accepted (in implementation)
**Deciders**: Jeff Bridwell

## Context

The personal knowledge graph needs a visibility model that matches how ideas and content naturally mature — starting private, optionally shared with specific people, and eventually published publicly.

## Decision

Use SOLID ACLs to implement a three-tier graduation model:
1. **Private** — only the owner can see it (default for all new content)
2. **Shared** — selectively visible to specific people or groups via SOLID ACLs
3. **Public** — released to the world

## Rationale

- Privacy-first aligns with Jeff's core values and the product vision ("the workshop is not the storefront")
- SOLID ACLs provide standard, granular access control without custom auth logic
- The graduation model applies across all domains (books, property, blog, gallery, profile)
- Content graduates when ready — the system doesn't push toward public by default

## Consequences

- All new content defaults to private — requires explicit action to share or publish
- ACL enforcement must be correct and tested (Playwright e2e tests in progress)
- The AI layer (future) must respect ACLs — it can only reason over data the user has access to
- WordPress blog acts as a staging area for content that may graduate to public
