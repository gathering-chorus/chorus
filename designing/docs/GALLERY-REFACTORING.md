# Gallery Refactoring Plan

**Last updated**: 2026-03-12 by Wren (PM) — #1290 freshness audit (verified: all 6 planned items still pending)

> Tracks technical debt and planned improvements for the gallery collection.

---

## Completed

### Tag Sync: Static JSON → Dynamic Spotlight (2026-02-11)

**Problem**: Tags were stored in a pre-generated `data/image-tags.json` (900KB, 20,962 entries) that drifted out of sync with actual Finder tags on the filesystem. Counts diverged (e.g., Green: 20,143 in JSON vs 17,796 actual). New tags (like 💄 emoji) never appeared.

**Solution**: Added `/api/tags` endpoint to `media-server` that queries macOS Spotlight (`mdfind`) for each configured tag. GalleryService fetches tags from media-server on first `listImages()` call, with 5-minute cache TTL.

**Architecture**:
```
Browser → personal-site:3000 /api/gallery/images
  → GalleryService.listImages()
    → media-server:8082 /api/tags  (Spotlight mdfind, cached 5min)
    → images-api:8081 /gallery/images
    → merge tags into response
  → { images: [{name, tags: ['green','💄']}, ...] }
```

**Config**: `MEDIA_FINDER_TAGS` env var on media-server controls which tags to scan (default: `Red,Orange,Yellow,Green,Blue,Purple,Gray,💄`). `TAG_CACHE_TTL_MS` controls cache duration (default: 300000ms / 5min).

---

## Planned

### 1. images-api Consolidation

**Current state**: The `images-api` (port 8081) is a separate Python/Flask service running on the remote Mac that reads the `/Volumes/VideosNew/Models` directory and returns image metadata. It predates the media-server.

**Problem**: Two services on the remote Mac serve the same directory — images-api lists files, media-server serves them. This is redundant.

**Plan**: Migrate the directory listing from images-api into media-server as a `/api/images` (GET, no param) endpoint. This consolidates to a single service on the remote Mac. The images-api can then be retired.

**Blocked by**: Need to audit all images-api consumers (content queries, search) before removing it.

---

### 2. Video Content Evaluation

**Current state**: Video streaming (`/videos/direct`) is implemented in media-server but the gallery UI only proxies images. Video content pages exist (`content-gallery.ejs`) but performance hasn't been evaluated.

**Plan**:
- Test video streaming performance over LAN (direct play vs transcoding)
- Evaluate whether to proxy video through Express or serve direct from media-server
- Consider HLS/DASH for adaptive bitrate on larger files

---

### 3. Content Gallery Tag Filtering

**Current state**: The main gallery (`collection-gallery.ejs`) has tag filtering. The content gallery (`content-gallery.ejs`) shows content items for a specific image but does not filter by `content_tags`.

**Plan**: Add tag filtering to content gallery view, matching the original `personal-website` implementation.

---

### 4. Tag Management UI

**Current state**: Tags are read-only — they reflect macOS Finder tags applied on the remote Mac. There's no way to add/remove tags from the gallery UI.

**Plan** (low priority): Add tag editing capability:
- `PUT /api/tags/:filename` on media-server to set tags via `xattr`
- UI buttons to tag/untag images from the gallery
- Consider whether SOLID pod metadata should mirror Finder tags

---

### 5. Thumbnail Caching

**Current state**: Every image request goes through `sharp` resize on the Express server (CPU-intensive for 21K images). No disk cache.

**Plan**: Add a disk-based thumbnail cache:
- Cache resized images to `data/gallery-cache/{width}/{filename}.webp`
- Serve cached versions on subsequent requests
- Invalidate based on source file mtime from media-server

---

### 6. Search Improvements

**Current state**: Gallery search is client-side substring matching on filenames only.

**Plan**:
- Add server-side search with tag-aware filtering
- Consider Spotlight `mdfind` for content-based search on the remote Mac
- Full-text search across content names
