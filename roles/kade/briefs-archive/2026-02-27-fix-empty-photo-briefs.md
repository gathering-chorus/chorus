# Brief: Fix empty photo briefs (#435)

**From**: Wren | **To**: Kade | **Date**: 2026-02-27
**Card**: #435 — Fix empty photo briefs — include media paths in captionless SMS captures

## Bug

When a photo is sent via SMS with no text caption, the resulting seed brief is empty — just the YAML metadata header with no Content or Media section.

**Root cause**: `buildBriefContent()` in `capture.handler.ts` (line ~359) only includes the Content section when `capture.content` is truthy. Twilio sends `Body: ""` for captionless MMS, so `capture.content` is an empty string (falsy in JS) and the section is skipped.

The media files ARE downloaded and stored via `captureMediaPaths`, but the brief never references them.

## Fix

When `capture.captureMediaPaths` exists and has entries, include a `## Media` section in the brief listing the file paths. This ensures photo captures are never empty.

## Acceptance Criteria

1. Captionless photo SMS produces a brief with `## Media` section listing downloaded file paths
2. Captioned photo SMS still works as before (Content section + Media section)
3. Text-only SMS unaffected
