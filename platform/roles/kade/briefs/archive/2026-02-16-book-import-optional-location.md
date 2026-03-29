# Brief: Book Import — Make Location Optional

**From**: Wren (PM)
**To**: Kade (Engineer)
**Date**: 2026-02-16
**Priority**: P2 — Next (queue after music harvester)
**Board**: #49

---

## Problem

The book upload workflow currently requires room, bookcase, and shelf before you can start importing. Jeff has **30 boxes of unshelved books** plus a library room of shelved books. He wants to ingest them all NOW without worrying about where they live physically, then add shelf locations later by photographing spines in place.

The current workflow assumes: "I'm standing at a bookcase, scanning what's on shelf 3."
The real workflow is: "I have boxes of books, get them in the system, I'll shelve them later."

## Changes

### 1. Make location block optional on upload session creation

In `book.schemas.ts`:
- `createSessionSchema`: Make `room`, `bookcase`, `shelf` all optional (or make the entire location object optional)
- Books created without location get no `jb:locationRoom` / `jb:locationBookcase` / `jb:locationShelf` triples

In `book-upload.ejs`:
- Remove `required` attribute from room/bookcase/shelf in Step 1
- Allow proceeding to Step 2 with no location selected
- Maybe add a "No location yet" or "Unshelved" option

### 2. Add location later via edit

The edit modal in `collection-books.ejs` already supports updating location (location is optional in `bookUpdateSchema`). So the "add shelf later" workflow already works — you just edit the book and add location.

### 3. Bulk location assignment (v2, not this card)

Future: photograph a shelf of spines → Claude Vision identifies books → bulk-assign location to all matched books. This is the "second pass" workflow Jeff described. Not in scope for this card — just noting the intent.

## What Done Looks Like

- Jeff can start a book upload session without selecting room/bookcase/shelf
- Books import successfully with no location data
- Books show up in the collection view (maybe with an "Unshelved" indicator)
- Jeff can later edit individual books to add location
- Existing workflow (location on import) still works for people who want it

## Files to Touch

- `src/validation/book.schemas.ts` — `createSessionSchema` location fields → optional
- `views/book-upload.ejs` — Step 1 form, remove required attributes
- `src/handlers/book-upload.handler.ts` — Handle missing location gracefully
- `src/services/book-pod.service.ts` — Skip location triples when not provided
- `views/collection-books.ejs` — Show "Unshelved" or similar when no location

---

— Wren
