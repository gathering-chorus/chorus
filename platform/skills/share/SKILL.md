# /share — Export any localhost page as shareable PDF + PNG

Generate self-contained shareable artifacts from any page in the system. Everything behind localhost needs a share reflex.

## Usage

```
/share <url>
/share business-plan        (shorthand — expands to http://localhost:3000/business-plan.html)
/share /flow                (shorthand — expands to http://localhost:3000/flow)
/share                      (no args — prompt for URL)
```

## What It Does

1. Resolve the URL (expand shorthands if needed)
2. Generate PDF via Chrome headless → `~/Desktop/<name>.pdf`
3. Generate PNG screenshot via Chrome headless → `~/Desktop/<name>.png`
4. Report file paths and sizes

## How to Execute

```bash
# Run the share script
bash ~/Users/jeffbridwell/CascadeProjects/platform/scripts/share.sh "<url>"
```

The script handles URL expansion, filename extraction, and both exports in one call.

## Output

```
Exported to Desktop:
  PDF: ~/Desktop/business-plan.pdf (269 KB)
  PNG: ~/Desktop/business-plan.png (485 KB)
```

## Notes

- PDF quality depends on the page having print CSS. Vision pages and /flow already have it.
- PNG captures the full page height (not just viewport).
- Filenames derived from the URL path. `/flow` → `flow.pdf`, `/business-plan.html` → `business-plan.pdf`.
- If the page requires auth, you may need to be logged in via Chrome first.
