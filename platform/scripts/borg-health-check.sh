#!/usr/bin/env bash
# borg-health-check.sh (#2124) — verify /borg/* pages have data, not just 200.
# Reads a contract describing each page's backing API and an assertion over
# the response. Hits the API, evaluates the assertion, prints PASS/FAIL per
# probe. Exits non-zero if any probe fails.
#
# Callers: deep-health.sh loops output into FAILURES. Debug with CLI.
# Overrides (for tests): BORG_HEALTH_API_BASE, BORG_HEALTH_CONTRACT.

set -euo pipefail

# #2571 — source-from-substrate replaces #1917's ${CHORUS_ROOT:-mac-path} default
source "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh"
CONTRACT="${BORG_HEALTH_CONTRACT:-${CHORUS_ROOT}/platform/scripts/borg-health-contract.json}"
API_BASE="${BORG_HEALTH_API_BASE:-http://localhost:3340}"

if [ ! -f "$CONTRACT" ]; then
  echo "borg-health: contract missing at $CONTRACT" >&2
  exit 2
fi

PROBE_COUNT=$(python3 -c "import json; print(len(json.load(open('$CONTRACT'))['probes']))")
FAILURES=0

if [ "$PROBE_COUNT" -eq 0 ]; then
  echo "borg-health: 0/0 probes passed"
  exit 0
fi

for i in $(seq 0 $((PROBE_COUNT - 1))); do
  entry=$(python3 -c "
import json
p = json.load(open('$CONTRACT'))['probes'][$i]
print(p.get('page',''))
print(p.get('api',''))
print(p.get('assert',''))
print(p.get('assert_content_type',''))
print(p.get('tracking_card',''))
")
  page=$(echo "$entry" | sed -n '1p')
  api=$(echo "$entry"  | sed -n '2p')
  assertion=$(echo "$entry" | sed -n '3p')
  ctype_prefix=$(echo "$entry" | sed -n '4p')
  tracking_card=$(echo "$entry" | sed -n '5p')
  url="${API_BASE}${api}"

  # A tracking card means this failure is known and being tracked — report as WARN
  # not FAIL so downstream alerting doesn't treat it as a new issue.
  fail_label="FAIL"
  [ -n "$tracking_card" ] && fail_label="WARN"

  if [ -n "$ctype_prefix" ]; then
    headers=$(curl -sI --max-time 5 "$url" 2>/dev/null || true)
    code=$(echo "$headers" | head -1 | awk '{print $2}')
    code=${code:-000}
    ctype=$(echo "$headers" | awk 'tolower($1) == "content-type:" {print $2}' | tr -d '\r\n')
    if [ "$code" != "200" ]; then
      echo "$fail_label $page — $api returned ${code:-000}"
      FAILURES=$((FAILURES + 1))
      continue
    fi
    case "$ctype" in
      ${ctype_prefix}*)
        echo "PASS $page — content-type $ctype"
        ;;
      *)
        echo "$fail_label $page — content-type '$ctype' does not start with '$ctype_prefix'"
        FAILURES=$((FAILURES + 1))
        ;;
    esac
    continue
  fi

  body_file=$(mktemp)
  code=$(curl -s --max-time 5 -o "$body_file" -w "%{http_code}" "$url" 2>/dev/null)
  code=${code:-000}
  if [ "$code" != "200" ]; then
    echo "$fail_label $page — $api returned $code"
    FAILURES=$((FAILURES + 1))
    rm -f "$body_file"
    continue
  fi

  result=$(BODY_FILE="$body_file" ASSERTION="$assertion" python3 <<'PY' 2>&1
import json, os, sys
try:
    d = json.load(open(os.environ['BODY_FILE']))
except Exception as e:
    print('ERR body not json: ' + str(e))
    sys.exit(0)
try:
    ok = bool(eval(os.environ['ASSERTION']))
except Exception as e:
    print('ERR assertion raised: ' + str(e))
    sys.exit(0)
print('OK' if ok else 'NO')
PY
)
  rm -f "$body_file"

  case "$result" in
    OK)
      echo "PASS $page — $assertion"
      ;;
    NO)
      echo "$fail_label $page — assertion false: $assertion"
      FAILURES=$((FAILURES + 1))
      ;;
    *)
      echo "$fail_label $page — $result"
      FAILURES=$((FAILURES + 1))
      ;;
  esac
done

if [ "$FAILURES" -gt 0 ]; then
  echo "borg-health: ${FAILURES}/${PROBE_COUNT} probe(s) failed"
  exit 1
fi
echo "borg-health: ${PROBE_COUNT}/${PROBE_COUNT} probes passed"
exit 0
