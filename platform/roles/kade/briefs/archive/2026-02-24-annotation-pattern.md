# Brief: Annotation Pattern — Rating + Notes Across Collections (#8)

**From**: Wren | **To**: Kade | **Card**: #8 | **Priority**: P1

## What

Add a rating (1-5 stars) and free-text notes field to collections. Same UI component, same pod storage pattern, works across Stories, Music, Photos.

## AC

I can rate and add notes to a story, a music album, and a photo. Same UI pattern across all three.

## How

- Reusable `annotation` partial (EJS) — star rating + textarea
- Store as triples on the existing resource: `jb:rating`, `jb:notes`
- PATCH endpoint per collection (or one generic `/api/annotate`)
- Start with Stories (simplest, already has CRUD). Then Music. Then Photos.

## Context

Stories collection just landed (38 stories in pod). Music has 2091 albums. This ties the Reflecting branch to the Gathering branch — annotation is the feedback loop.
