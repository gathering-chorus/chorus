#!/usr/bin/env bash
# MIGRATE: TypeScript P3 — DEC-100 (no bash APIs)
# smoke-check.sh — Authenticated page verification
#
# Usage:
#   smoke-check.sh /music              Check single page
#   smoke-check.sh /music /photos      Check multiple pages
#   smoke-check.sh --all               Check all known pages
#   smoke-check.sh --auth-only         Check only auth-gated pages
#   smoke-check.sh --public-only       Check only public pages
#
# Returns: 0 if all pass, 1 if any fail
# Output: one line per page — PASS/FAIL with status code and content check

set -euo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"

BASE_URL="${GATHERING_URL:-http://localhost:3000}"
COOKIE_JAR="/tmp/smoke-check-cookie.txt"

# Known pages — public (no auth required) and auth-gated
PUBLIC_PAGES=(
  /
  /about
  /login
  /health:json
)

AUTH_PAGES=(
  # Collections — core domains Jeff uses daily
  /music
  /music/artists
  /photos
  /people
  /books
  /reading
  /cooking
  /todo
  /stories
  /notes
  /glimmers
  /ideas
  /projects
  /intentions
  /socialposts
  /watching
  /sexuality
  /documents
  /gardening
  /values
  /practices
  /seeds
  # Chorus surfaces — team coordination
  /chorus
  /loom
  /werk
  /flow
  /jeff
  /cost
  /hooks
  /decisions
  /experience
  /interaction-patterns
  # Knowledge & ontology
  /model-data
  /chorus-model-data
  /search
  /dashboard
  /practice-spine
  /fitness-functions
  # Admin & system
  /profile
  /blog
  /property
  /gallery
  /harvest-manifests
  /docs
  /self
  /borg-assessment
)

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
NC='\033[0m'

pass=0
fail=0
warn=0

check_page() {
  local raw="$1"
  local path="${raw%%:*}"
  local tag="${raw#*:}"
  [ "$tag" = "$raw" ] && tag=""
  local url="${BASE_URL}${path}"
  local min_size=500
  [ "$tag" = "json" ] && min_size=10

  # First check without following redirects to detect auth gates
  local raw_status
  raw_status=$(curl -s -o /dev/null -w "%{http_code}" \
    -b "$COOKIE_JAR" -c "$COOKIE_JAR" \
    --max-time 10 \
    "$url" 2>/dev/null) || raw_status="000"

  # Get full response (following redirects) for content check
  local status body_file="/tmp/smoke-check-body.tmp"
  status=$(curl -s -o "$body_file" -w "%{http_code}" \
    -b "$COOKIE_JAR" -c "$COOKIE_JAR" \
    -L --max-redirs 5 \
    --max-time 10 \
    "$url" 2>/dev/null) || status="000"

  local body_size=0
  [ -f "$body_file" ] && body_size=$(wc -c < "$body_file" | tr -d ' ')

  # Check for error indicators in body
  local has_error=""
  if [ -f "$body_file" ] && [ "$body_size" -gt 0 ]; then
    if grep -q -i "Internal Server Error\|ECONNREFUSED\|Cannot GET\|500 Error" "$body_file" 2>/dev/null; then
      has_error="error-content"
    fi
  fi

  # Evaluate result
  if [ "$status" = "000" ] || [ "$raw_status" = "000" ]; then
    printf "${RED}FAIL${NC}  %-25s  connection refused\n" "$path"
    fail=$((fail + 1))
  elif [ "$raw_status" = "302" ]; then
    # Auth redirect — route exists, auth gate working
    printf "${GREEN}PASS${NC}  %-25s  302 → login (auth gate)\n" "$path"
    pass=$((pass + 1))
  elif [ "$raw_status" = "301" ]; then
    # Permanent redirect — route exists, redirecting to canonical URL
    printf "${GREEN}PASS${NC}  %-25s  301 → redirect (canonical)\n" "$path"
    pass=$((pass + 1))
  elif [ "$raw_status" = "308" ]; then
    # Permanent redirect (method-preserving) — #2122 Caddy edge-proxy redirects
    # for Borg pages migrated to chorus-api (/borg/*). Path still resolves.
    printf "${GREEN}PASS${NC}  %-25s  308 → redirect (migrated)\n" "$path"
    pass=$((pass + 1))
  elif [ "$raw_status" = "200" ] && [ "$body_size" -gt "$min_size" ] && [ -z "$has_error" ]; then
    printf "${GREEN}PASS${NC}  %-25s  %s %s bytes\n" "$path" "$raw_status" "$body_size"
    pass=$((pass + 1))
  elif [ "$raw_status" = "200" ] && [ "$body_size" -le "$min_size" ]; then
    printf "${YELLOW}WARN${NC}  %-25s  %s %s bytes (suspiciously small)\n" "$path" "$raw_status" "$body_size"
    warn=$((warn + 1))
  elif [ -n "$has_error" ]; then
    printf "${RED}FAIL${NC}  %-25s  %s error content detected\n" "$path" "$raw_status"
    fail=$((fail + 1))
  else
    printf "${RED}FAIL${NC}  %-25s  %s %s bytes\n" "$path" "$raw_status" "$body_size"
    fail=$((fail + 1))
  fi

  rm -f "$body_file"
}

# Parse arguments
pages=()
mode="custom"
scope_files=""
scope_card=""

for arg in "$@"; do
  case "$arg" in
    --all)        mode="all" ;;
    --nav)        mode="nav" ;;
    --auth-only)  mode="auth" ;;
    --public-only) mode="public" ;;
    --card=*)     scope_card="${arg#--card=}" ;;
    --files=*)    scope_files="${arg#--files=}" ;;
    --help|-h)
      echo "smoke-check.sh — page verification"
      echo ""
      echo "Usage:"
      echo "  smoke-check.sh /music /photos   Check specific pages"
      echo "  smoke-check.sh --all            Check all known pages"
      echo "  smoke-check.sh --auth-only      Check auth-gated pages only"
      echo "  smoke-check.sh --public-only    Check public pages only"
      echo "  smoke-check.sh --card <id>      Scope smoke by card's blast radius (#2229)"
      echo "  smoke-check.sh --files <paths>  Scope smoke by explicit file list"
      echo ""
      echo "Scope logic (--card / --files):"
      echo "  - If any file affects app pages (views, chorus-api handlers) → run --all"
      echo "  - If all files are non-app (scripts/hooks/rust/skills/configs/docs) → skip"
      exit 0
      ;;
    /*)           pages+=("$arg") ;;
    *)            echo "Unknown arg: $arg" >&2; exit 1 ;;
  esac
done

# --- Nav tree walker ---
NAV_TREE="/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/src/config/nav-tree.json"

extract_nav_links() {
  python3 -c "
import json, sys
with open('$NAV_TREE') as f:
    tree = json.load(f)

def extract(node, results):
    if isinstance(node, dict):
        h = node.get('href','')
        if h and h != '#':
            ext = 'external' if node.get('external') else 'internal'
            results.append((node.get('label','?'), h, ext))
        for child in node.get('children', []):
            extract(child, results)
        for leaf in node.get('leaves', []):
            extract(leaf, results)

results = []
for branch in tree.get('branches', []):
    extract(branch, results)

# Deduplicate by href
seen = set()
for label, href, kind in results:
    if href not in seen:
        seen.add(href)
        print(f'{kind}|{label}|{href}')
" 2>/dev/null
}

check_external() {
  local label="$1" url="$2"
  local status
  status=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 -L "$url" 2>/dev/null) || status="000"
  if [ "$status" -ge 200 ] && [ "$status" -lt 400 ]; then
    printf "${GREEN}PASS${NC}  %-40s  %s\n" "$label" "$status"
    pass=$((pass + 1))
  else
    printf "${RED}FAIL${NC}  %-40s  %s  %s\n" "$label" "$status" "$url"
    fail=$((fail + 1))
  fi
}

# --- Blast-radius scoping (#2229) ---
# file_affects_app: return 0 if file path could affect a user-facing page,
# 1 if clearly non-app (scripts, hooks, skills, configs, rust, docs).
# Patterns match anywhere in the path via leading-asterisk.
file_affects_app() {
  local f="$1"
  case "$f" in
    # Non-app: no user-facing page affected
    *platform/scripts/*) return 1 ;;
    *chorus-hooks/*) return 1 ;;
    *chorus-inject/*) return 1 ;;
    *skills/*) return 1 ;;
    *designing/*) return 1 ;;
    *roles/*) return 1 ;;
    *briefs/*) return 1 ;;
    *decisions/*) return 1 ;;
    *seeds/*) return 1 ;;
    *.md) return 1 ;;
    *jest.config.js) return 1 ;;
    *.toml) return 1 ;;
    *.rs) return 1 ;;
    # Config files that can affect build output reaching the app
    *package.json) return 0 ;;
    # App-affecting: views, routes, handlers, config for app
    *jeff-bridwell-personal-site/*) return 0 ;;
    *platform/api/src/*) return 0 ;;
    *directing/clearing/src/*) return 0 ;;
    *platform/apps/*) return 0 ;;
    *) return 1 ;;  # default: non-app, conservative
  esac
}

# If --card given, resolve to file list via blast-radius comment
if [ -n "$scope_card" ]; then
  CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
  blast=$(bash "$CHORUS_ROOT/platform/scripts/cards" view "$scope_card" 2>/dev/null | grep -A 200 'Blast Radius' | grep -E '^\s+[a-z]' | head -100 || true)
  if [ -n "$blast" ]; then
    scope_files=$(echo "$blast" | tr -s ' ' '\n' | grep -E '\.' | head -50 | tr '\n' ' ')
  fi
fi

# If we have a file scope, decide: all | skip
if [ -n "$scope_files" ]; then
  any_app=0
  file_count=0
  for f in $scope_files; do
    file_count=$((file_count + 1))
    if file_affects_app "$f"; then
      any_app=1
      break
    fi
  done
  if [ "$any_app" = "0" ]; then
    echo "smoke-check: $file_count file(s) in scope, none affect app pages — skipping smoke"
    echo "  (scripts / hooks / skills / rust / configs / docs do not need page verification)"
    exit 0
  else
    # Conservative: if any file might touch app, run full smoke
    echo "smoke-check: $file_count file(s) in scope, app-affecting detected — running --all"
    mode="all"
  fi
fi

# Build page list based on mode
case "$mode" in
  all)    pages=("${PUBLIC_PAGES[@]}" "${AUTH_PAGES[@]}") ;;
  auth)   pages=("${AUTH_PAGES[@]}") ;;
  public) pages=("${PUBLIC_PAGES[@]}") ;;
  nav)    ;; # handled below
  custom)
    if [ ${#pages[@]} -eq 0 ]; then
      echo "Usage: smoke-check.sh /path [/path2 ...] | --all | --nav | --auth-only | --public-only" >&2
      exit 1
    fi
    ;;
esac

if [ "$mode" = "nav" ]; then
  # Walk every link in the nav tree
  internal_links=()
  external_links=()
  while IFS='|' read -r kind label href; do
    if [ "$kind" = "internal" ]; then
      internal_links+=("$href")
    else
      external_links+=("$label|$href")
    fi
  done < <(extract_nav_links)

  total=$((${#internal_links[@]} + ${#external_links[@]}))
  echo "Nav walk: ${BASE_URL} (${total} links — ${#internal_links[@]} internal, ${#external_links[@]} external)"
  echo ""

  echo "--- Internal pages ---"
  for page in "${internal_links[@]}"; do
    check_page "$page"
  done

  echo ""
  echo "--- External links ---"
  for entry in "${external_links[@]}"; do
    label="${entry%%|*}"
    url="${entry#*|}"
    check_external "$label" "$url"
  done
else
  # Run standard checks
  echo "Smoke check: ${BASE_URL} (${#pages[@]} pages)"
  echo ""

  for page in "${pages[@]}"; do
    check_page "$page"
  done
fi

echo ""
echo "Results: ${pass} pass, ${fail} fail, ${warn} warn"

# Exit non-zero if any failures
[ "$fail" -eq 0 ] || exit 1
