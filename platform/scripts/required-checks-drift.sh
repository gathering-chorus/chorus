#!/usr/bin/env bash
# required-checks-drift.sh — detect drift between three sources for the
# canonical CI required-checks list (#2500). Per DEC-2525 the canonical
# list is small (today: unit-tests + cargo-test) and lives in
# platform/state/required-checks.json. Three places that can drift:
#
#   1. quality.yml — must contain a job for each canonical name
#   2. GitHub branch-protection required_status_checks contexts
#   3. GitHub rulesets required_status_checks
#
# This script extracts canonical (1) and (2,3) and reports drift.
# CI is authoritative on `main`; this drift detector runs in pre-commit
# (warning, doc-coherence-ratchet wires it) and on schedule for visibility.
#
# Subcommands:
#   list-canonical                 print canonical required-check names
#   list-workflow <yaml>           print job names from a workflow YAML
#   diff-workflow <yaml>           fail if any canonical name missing from workflow
#   diff-protection <repo>         fail if branch-protection drifts (gh required)
#   diff-rulesets <repo>           fail if rulesets drift (gh required)
#   diff-all <repo>                run all three drift checks
#
# Env: REQUIRED_CHECKS_FILE — path to canonical JSON
#      (default: platform/state/required-checks.json)

set -euo pipefail

DEFAULT_FILE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../state" 2>/dev/null && pwd)/required-checks.json"
CANONICAL="${REQUIRED_CHECKS_FILE:-$DEFAULT_FILE}"

usage() {
  cat <<'EOF' >&2
Usage:
  required-checks-drift.sh list-canonical
  required-checks-drift.sh list-workflow <quality.yml-path>
  required-checks-drift.sh diff-workflow <quality.yml-path>
  required-checks-drift.sh diff-protection <owner/repo>
  required-checks-drift.sh diff-rulesets <owner/repo>
  required-checks-drift.sh diff-all <owner/repo>

Reads $REQUIRED_CHECKS_FILE (default: platform/state/required-checks.json).
EOF
}

require_canonical() {
  if [ ! -f "$CANONICAL" ]; then
    echo "required-checks-drift: canonical file not found at $CANONICAL (fail-closed)" >&2
    exit 1
  fi
}

list_canonical() {
  require_canonical
  python3 - "$CANONICAL" <<'PY'
import json, sys
with open(sys.argv[1]) as f:
    data = json.load(f)
for e in data.get("required", []):
    print(e.get("name", ""))
PY
}

list_workflow() {
  local yaml="${1:-}"
  if [ -z "$yaml" ] || [ ! -f "$yaml" ]; then
    echo "required-checks-drift: list-workflow needs a YAML file path" >&2
    exit 1
  fi
  python3 - "$yaml" <<'PY'
import sys, re
# Light YAML parse — extract top-level job names under `jobs:`. Avoids a yaml
# dependency; relies on the workflow's actual indent shape (2 spaces under
# `jobs:`). Fallback: any line matching `^  ([a-z][a-z0-9-]+):` after `jobs:`.
in_jobs = False
job_re = re.compile(r'^  ([a-zA-Z][a-zA-Z0-9_-]+):\s*$')
with open(sys.argv[1]) as f:
    for line in f:
        if line.startswith("jobs:"):
            in_jobs = True
            continue
        if in_jobs:
            if line and not line[0].isspace() and line.strip():
                # Hit a top-level key after jobs: → out of section
                in_jobs = False
                continue
            m = job_re.match(line.rstrip("\n"))
            if m:
                print(m.group(1))
PY
}

diff_workflow() {
  local yaml="${1:-}"
  require_canonical
  if [ -z "$yaml" ] || [ ! -f "$yaml" ]; then
    echo "required-checks-drift: diff-workflow needs a YAML file path" >&2
    exit 1
  fi

  local canonical_list
  canonical_list=$(list_canonical)
  local workflow_list
  workflow_list=$(list_workflow "$yaml")

  local missing=()
  while IFS= read -r name; do
    [ -z "$name" ] && continue
    if ! echo "$workflow_list" | grep -qFx "$name"; then
      missing+=("$name")
    fi
  done <<< "$canonical_list"

  if [ ${#missing[@]} -gt 0 ]; then
    echo "required-checks-drift: DRIFT — canonical jobs missing from $yaml:"
    for m in "${missing[@]}"; do
      echo "  - $m"
    done
    return 1
  fi
  echo "required-checks-drift: workflow OK — all canonical jobs present"
}

diff_protection() {
  local repo="${1:-}"
  require_canonical
  if [ -z "$repo" ]; then
    echo "required-checks-drift: diff-protection needs <owner/repo>" >&2
    exit 1
  fi

  local canonical_list
  canonical_list=$(list_canonical | sort)
  local protection_list
  protection_list=$(gh api "repos/${repo}/branches/main/protection" 2>/dev/null \
    | python3 -c "import json,sys; d=json.load(sys.stdin); [print(c) for c in d.get('required_status_checks', {}).get('contexts', [])]" 2>/dev/null \
    | sort)

  if [ "$canonical_list" = "$protection_list" ]; then
    echo "required-checks-drift: branch-protection OK — matches canonical"
    return 0
  fi
  echo "required-checks-drift: DRIFT — branch-protection diverges from canonical"
  echo "  canonical: $(echo "$canonical_list" | tr '\n' ',' | sed 's/,$//')"
  echo "  protection: $(echo "$protection_list" | tr '\n' ',' | sed 's/,$//')"
  return 1
}

diff_rulesets() {
  local repo="${1:-}"
  require_canonical
  if [ -z "$repo" ]; then
    echo "required-checks-drift: diff-rulesets needs <owner/repo>" >&2
    exit 1
  fi

  local canonical_list
  canonical_list=$(list_canonical | sort)
  local ruleset_list
  ruleset_list=$(gh api "repos/${repo}/rulesets" 2>/dev/null \
    | python3 -c "
import json, sys
d = json.load(sys.stdin)
ids = [r['id'] for r in d if isinstance(r, dict)]
print('\n'.join(str(i) for i in ids))
" 2>/dev/null)

  local found=""
  while IFS= read -r rid; do
    [ -z "$rid" ] && continue
    found+=$(gh api "repos/${repo}/rulesets/${rid}" 2>/dev/null \
      | python3 -c "
import json, sys
d = json.load(sys.stdin)
for r in d.get('rules', []):
    if r.get('type') == 'required_status_checks':
        for c in r.get('parameters', {}).get('required_status_checks', []):
            print(c.get('context', ''))
" 2>/dev/null)
    found+=$'\n'
  done <<< "$ruleset_list"
  found=$(echo "$found" | grep -v '^$' | sort -u)

  if [ "$canonical_list" = "$found" ]; then
    echo "required-checks-drift: rulesets OK — matches canonical"
    return 0
  fi
  echo "required-checks-drift: DRIFT — rulesets diverge from canonical"
  echo "  canonical: $(echo "$canonical_list" | tr '\n' ',' | sed 's/,$//')"
  echo "  rulesets:  $(echo "$found" | tr '\n' ',' | sed 's/,$//')"
  return 1
}

diff_all() {
  local repo="${1:-}"
  local fail=0
  local workflow="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/.github/workflows/quality.yml"
  if [ -f "$workflow" ]; then
    diff_workflow "$workflow" || fail=1
  else
    echo "required-checks-drift: workflow not found at $workflow"
    fail=1
  fi
  diff_protection "$repo" || fail=1
  diff_rulesets "$repo" || fail=1
  return $fail
}

cmd="${1:-}"
shift || true

case "$cmd" in
  list-canonical)   list_canonical ;;
  list-workflow)    list_workflow "$@" ;;
  diff-workflow)    diff_workflow "$@" ;;
  diff-protection)  diff_protection "$@" ;;
  diff-rulesets)    diff_rulesets "$@" ;;
  diff-all)         diff_all "$@" ;;
  ""|help|-h|--help) usage; exit 1 ;;
  *) usage; exit 1 ;;
esac
