#!/usr/bin/env bash
# /share — export a localhost page as shareable HTML + PDF to Desktop
# Usage: share.sh <url-or-shorthand>

set -euo pipefail

CHROME="/Applications/Google Chrome.app/Contents/MacOS/Google Chrome"
DEST="$HOME/Desktop"
PUBLIC="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/public"

url="${1:-}"

if [[ -z "$url" ]]; then
  echo "Usage: share.sh <url-or-shorthand>"
  echo "  share.sh business-plan"
  echo "  share.sh /flow"
  echo "  share.sh http://localhost:3000/chorus"
  exit 1
fi

# Expand shorthands and track original for filename
if [[ "$url" =~ ^https?:// ]]; then
  : # already a full URL
elif [[ "$url" == /* ]]; then
  url="http://localhost:3000${url}"
else
  url="http://localhost:3000/${url}.html"
fi

# Extract filename from URL path
name=$(echo "$url" | sed 's|.*://[^/]*/||; s|\.html$||; s|/$||; s|/|-|g')
[[ -z "$name" ]] && name="page"

pdf_path="${DEST}/${name}.pdf"
html_path="${DEST}/${name}.html"

# Copy HTML if it exists as a static file (self-contained, best mobile experience)
html_source="${PUBLIC}/${name}.html"
if [[ -f "$html_source" ]]; then
  cp "$html_source" "$html_path"
  html_size=$(du -h "$html_path" | cut -f1 | xargs)
  html_msg="  HTML: ~/Desktop/${name}.html (${html_size}) — text this (best mobile experience)"
else
  html_msg="  HTML: not available (page is server-rendered, use PDF)"
fi

# Generate PDF (for email/formal sharing)
"$CHROME" --headless --disable-gpu --print-to-pdf="$pdf_path" --no-margins "$url" 2>/dev/null
pdf_size=$(du -h "$pdf_path" | cut -f1 | xargs)

echo "Exported to Desktop:"
echo "$html_msg"
echo "  PDF:  ~/Desktop/${name}.pdf (${pdf_size}) — email/formal"
