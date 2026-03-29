# Brief: Social Posts Harvester — Product Direction

**From:** Wren (PM)
**To:** Kade
**Date:** 2026-02-23
**Re:** Card #96 (shipped) — next steps for Facebook + LinkedIn ingestion

## Context

Great work shipping the social posts collection. Jeff wants to make sure you have clear product direction on where this goes next, since social media harvesting has real constraints.

## What You Shipped (confirmed)

- Manual add UI + platform filter (facebook/linkedin)
- File-based ingest approach (GDPR data export)
- Pod service, views, routes, navbar, mind map node
- No live API — correct call given platform restrictions

## Product Direction

### Facebook
- **Primary path: GDPR data export** — Facebook lets users download their full archive (Settings > Your Information > Download Your Information). This includes posts, photos, comments, reactions. JSON format preferred over HTML.
- **What Jeff wants harvested:** His own posts, shared links, photos with captions. NOT friends' posts or feed content.
- **Key question for you:** Can we build a file uploader that accepts the Facebook GDPR JSON export and parses it into social post pods? The schema is well-documented: `your_posts_1.json` contains posts, `your_photos.json` has photos.

### LinkedIn
- **Primary path: also data export** — LinkedIn Settings > Data Privacy > Get a copy of your data. Includes posts, articles, comments, connections.
- **What Jeff wants harvested:** His posts and articles. These are portfolio-as-pitch material — professional content he created.
- **Key question:** Same pattern — file uploader for LinkedIn's CSV/JSON export?

### What We Do NOT Want
- No OAuth flows or live API integration. Facebook's API is hostile to scraping personal data. LinkedIn's is worse.
- No browser automation / scraping. Violates ToS and is brittle.
- The GDPR export path is legal, stable, and complete.

## Suggested Next Card

If you have bandwidth after current queue: **"Social posts file ingest — Facebook GDPR JSON + LinkedIn data export"**. This would be a file upload → parse → create pods pipeline, similar to how Notes harvester works with `pending-harvest.jsonl`.

## Questions Jeff Mentioned

Jeff said you had questions about the harvester approach. If those are answered above, great. If not, write back or flag for a Clearing session and we'll sort it out.
