#!/bin/bash
# style-lint.sh — Check every page in the style manifest for consistency
# Usage: bash scripts/style-lint.sh [--fix]
# Checks: page responds 200, nav type matches, theme class present, footer exists

set -euo pipefail

BASE_URL="http://localhost:3000"
MANIFEST="$HOME/CascadeProjects/jeff-bridwell-personal-site/data/style-manifest.json"

if [ ! -f "$MANIFEST" ]; then
  echo "ERROR: style-manifest.json not found" >&2
  exit 1
fi

PASS=0
WARN=0
FAIL=0
SKIP=0
TOTAL=0

# Generate page list from manifest
PAGES=$(python3 -c "
import json
with open('$MANIFEST') as f:
    m = json.load(f)
for spoke in m['spokes']:
    for page in spoke['pages']:
        tooling = 'true' if page.get('tooling') else 'false'
        print(f\"{page['route']}|{page['label']}|{spoke['name']}|{page.get('theme','light')}|{page.get('nav','main')}|{tooling}\")
")

echo "Style lint: $(echo "$PAGES" | wc -l | tr -d ' ') pages from style-manifest.json"
echo "---"

while IFS='|' read -r route label spoke theme nav tooling; do
  TOTAL=$((TOTAL + 1))
  url="${BASE_URL}${route}"
  issues=""

  # Check 1: Page responds without following redirects
  raw_status=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 "$url" 2>/dev/null || echo "000")

  # Auth-protected pages redirect to /login — skip content checks
  if [ "$raw_status" = "302" ] || [ "$raw_status" = "301" ]; then
    SKIP=$((SKIP + 1))
    printf "  %-12s %-30s %s  [auth-required]\n" "SKIP" "$label" "$route"
    continue
  fi

  # Follow redirects for non-auth pages and get body
  status=$(curl -s -L -o /tmp/style-lint-body.html -w '%{http_code}' --max-time 10 "$url" 2>/dev/null || echo "000")
  if [ "$status" != "200" ]; then
    issues="${issues} [HTTP ${status}]"
  fi

  if [ "$status" = "200" ]; then
    body=$(cat /tmp/style-lint-body.html)

    # Check 2: Footer present
    if ! echo "$body" | grep -qi '<footer'; then
      issues="${issues} [no-footer]"
    fi

    # Check 3: Theme — dark pages should have data-theme="dark" or dark body class
    if [ "$theme" = "dark" ]; then
      if ! echo "$body" | grep -qiE '(data-theme="dark"|class="[^"]*dark|background.*#1a1a|background.*#16213)'; then
        issues="${issues} [expected-dark-theme]"
      fi
    fi

    # Check 4: Nav type — doc-chrome pages use different nav
    if [ "$nav" = "doc-chrome" ]; then
      if ! echo "$body" | grep -qi 'doc-chrome\|gathering-docs'; then
        issues="${issues} [expected-doc-chrome-nav]"
      fi
    fi

    # Check 5: Page title not empty/default (macOS grep — no -P)
    title=$(echo "$body" | sed -n 's/.*<title>\([^<]*\)<\/title>.*/\1/p' | head -1)
    if [ -z "$title" ] || [ "$title" = "undefined" ]; then
      issues="${issues} [empty-title]"
    fi
  fi

  # Report
  if [ -z "$issues" ]; then
    PASS=$((PASS + 1))
    printf "  %-12s %-30s %s\n" "OK" "$label" "$route"
  elif echo "$issues" | grep -q 'HTTP'; then
    FAIL=$((FAIL + 1))
    printf "  %-12s %-30s %s %s\n" "FAIL" "$label" "$route" "$issues"
  else
    WARN=$((WARN + 1))
    printf "  %-12s %-30s %s %s\n" "WARN" "$label" "$route" "$issues"
  fi

done <<< "$PAGES"

rm -f /tmp/style-lint-body.html

echo "---"
echo "Total: $TOTAL | Pass: $PASS | Warn: $WARN | Fail: $FAIL | Skip: $SKIP (auth-required)"

if [ "$FAIL" -gt 0 ]; then
  exit 1
fi
