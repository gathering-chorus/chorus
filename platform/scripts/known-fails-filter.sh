#!/usr/bin/env bash
# known-fails-filter.sh — pre-commit honors carded test failures (#2497).
#
# Reads platform/state/known-fails.json (or path in $KNOWN_FAILS_FILE),
# filters known failures from test output, returns 0 only if all failures
# are allowlisted.
#
# CI does NOT use this — main is authoritative; allowlist is local-only.
#
# Subcommands:
#   is-allowed <framework> <test_id>
#       exit 0 if allowlisted, 1 otherwise
#   filter-cargo <output-file>
#       parse cargo test output, subtract allowlisted failures,
#       exit 0 if all failures allowlisted, 1 otherwise
#   filter-jest <output-file>
#       same shape as filter-cargo, for jest output
#   list-cards
#       print unique card_ids referenced by allowlist (one per line)
#   count
#       print total entry count
#
# Allowlist schema:
#   {"schema_version": 1, "entries": [
#     {"test_id": "...", "framework": "cargo|jest", "card_id": NNNN,
#      "reason": "...", "filed_at": "ISO 8601"}
#   ]}

set -euo pipefail

DEFAULT_FILE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../state" 2>/dev/null && pwd)/known-fails.json"
ALLOWLIST="${KNOWN_FAILS_FILE:-$DEFAULT_FILE}"

usage() {
  cat <<'EOF' >&2
Usage:
  known-fails-filter.sh is-allowed <framework> <test_id>
  known-fails-filter.sh filter-cargo <output-file>
  known-fails-filter.sh filter-jest  <output-file>
  known-fails-filter.sh list-cards
  known-fails-filter.sh count

Reads $KNOWN_FAILS_FILE (default: platform/state/known-fails.json).
EOF
}

require_allowlist() {
  if [ ! -f "$ALLOWLIST" ]; then
    echo "known-fails-filter: allowlist not found at $ALLOWLIST (fail-closed)" >&2
    exit 1
  fi
}

is_allowed() {
  local framework="${1:-}"
  local test_id="${2:-}"
  if [ -z "$framework" ] || [ -z "$test_id" ]; then
    usage
    exit 1
  fi
  require_allowlist
  python3 - "$ALLOWLIST" "$framework" "$test_id" <<'PY'
import json, sys
path, framework, test_id = sys.argv[1], sys.argv[2], sys.argv[3]
with open(path) as f:
    data = json.load(f)
for e in data.get("entries", []):
    if e.get("framework") == framework and e.get("test_id") == test_id:
        sys.exit(0)
sys.exit(1)
PY
}

list_cards() {
  require_allowlist
  python3 - "$ALLOWLIST" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
seen = []
for e in data.get("entries", []):
    cid = e.get("card_id")
    if cid is not None and cid not in seen:
        seen.append(cid)
for c in seen:
    print(c)
PY
}

count_entries() {
  require_allowlist
  python3 - "$ALLOWLIST" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
print(len(data.get("entries", [])))
PY
}

filter_cargo() {
  local out_file="${1:-}"
  if [ -z "$out_file" ] || [ ! -f "$out_file" ]; then
    echo "known-fails-filter: filter-cargo needs an output file path" >&2
    exit 1
  fi
  require_allowlist
  python3 - "$ALLOWLIST" "$out_file" "cargo" <<'PY'
import json, re, sys
allowlist_path, out_path, framework = sys.argv[1], sys.argv[2], sys.argv[3]

with open(allowlist_path) as f:
    data = json.load(f)
allowed = {e["test_id"] for e in data.get("entries", []) if e.get("framework") == framework}

# cargo test FAILED line: "test <name> ... FAILED"
fail_re = re.compile(r'^test (\S+) \.\.\. FAILED\s*$')

failures = []
with open(out_path) as f:
    for line in f:
        m = fail_re.match(line.rstrip())
        if m:
            failures.append(m.group(1))

unallowed = [t for t in failures if t not in allowed]
allowed_hits = [t for t in failures if t in allowed]

if unallowed:
    print(f"known-fails-filter: {len(unallowed)} non-allowlisted failure(s):")
    for t in unallowed:
        print(f"  - {t}")
    if allowed_hits:
        print(f"known-fails-filter: ({len(allowed_hits)} allowlisted skipped)")
    sys.exit(1)
else:
    if allowed_hits:
        print(f"known-fails-filter: {len(allowed_hits)} allowlisted, all failures suppressed")
    else:
        print("known-fails-filter: no failures detected")
    sys.exit(0)
PY
}

filter_jest() {
  local out_file="${1:-}"
  if [ -z "$out_file" ] || [ ! -f "$out_file" ]; then
    echo "known-fails-filter: filter-jest needs an output file path" >&2
    exit 1
  fi
  require_allowlist
  python3 - "$ALLOWLIST" "$out_file" "jest" <<'PY'
import json, re, sys
allowlist_path, out_path, framework = sys.argv[1], sys.argv[2], sys.argv[3]

with open(allowlist_path) as f:
    data = json.load(f)
allowed = {e["test_id"] for e in data.get("entries", []) if e.get("framework") == framework}

# jest failure line shape: "  ✕ <full test name> (Nms)" or "FAIL <test_path>"
# We accept either formal test paths (path.test.ts > suite > case) or jest-styled "✕ name"
fail_res = [
    re.compile(r'^\s+✕\s+(.+?)(?:\s+\(\d+\s*ms\))?\s*$'),
    re.compile(r'^FAIL\s+(.+)$'),
]

failures = []
with open(out_path) as f:
    for line in f:
        for r in fail_res:
            m = r.match(line.rstrip())
            if m:
                failures.append(m.group(1).strip())
                break

unallowed = [t for t in failures if t not in allowed]
allowed_hits = [t for t in failures if t in allowed]

if unallowed:
    print(f"known-fails-filter: {len(unallowed)} non-allowlisted failure(s):")
    for t in unallowed:
        print(f"  - {t}")
    if allowed_hits:
        print(f"known-fails-filter: ({len(allowed_hits)} allowlisted skipped)")
    sys.exit(1)
else:
    if allowed_hits:
        print(f"known-fails-filter: {len(allowed_hits)} allowlisted, all failures suppressed")
    else:
        print("known-fails-filter: no failures detected")
    sys.exit(0)
PY
}

cmd="${1:-}"
shift || true

case "$cmd" in
  is-allowed)    is_allowed "$@" ;;
  filter-cargo)  filter_cargo "$@" ;;
  filter-jest)   filter_jest "$@" ;;
  list-cards)    list_cards ;;
  count)         count_entries ;;
  ""|help|-h|--help) usage; exit 1 ;;
  *) usage; exit 1 ;;
esac
