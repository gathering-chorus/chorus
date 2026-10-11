#!/usr/bin/env bash
# @test-type: unit — reads tracked files only; --live adds the store count through the door
# @domain: domains
#
# #4489 — five empty domains are retired (Jeff 2026-10-10, "yes on all"):
# alerts-monitors -> alerts + monitors, athena-domain -> domains,
# chorus-domain -> awareness, convergence-domain -> integrations, gates -> policies.
# A retirement holds only while nothing names them again, so this fails when:
#   - code, a test, a page or a TTL uses one of the five as a domain name
#   - the ledger stops retiring one of them from urn:chorus:domains:domains
# --self-test plants each violation and shows the check goes red (#3734).
# --live also asks the store (4489-domains-apply.py --count) for links still pointing at them.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
NAMES='alerts-monitors|athena-domain|chorus-domain|convergence-domain|gates'
# Not a domain use: a card relation key ('gates' between cards), a source FILE
# name key in the crawler's module table, one history comment, and the ledger
# deployer's own merge fixture (gates-domain into gates, #4338), and this card's own
# negative proof that the retired athena-domain admits no page. TTL comments are history.
ALLOW='^directing/products/cards/src/sdk\.ts:|^platform/services/chorus-crawl/src/domain\.rs:[0-9]+:    \("chorus-domain", "domains"\)|^platform/api/tests/domain-borg-services\.integration\.test\.ts:[0-9]+: \* |^platform/services/athena-deploy/src/lib\.rs:|^platform/api/tests/discover-pages-athena-4489\.test\.ts:'

uses() { # $1 = repo root; prints each use of a retired name as a domain
  local E='([^-a-zA-Z0-9]|$)'
  git -C "$1" grep -nE "['\"]($NAMES)['\"]|(^|[^:a-z])chorus:($NAMES)$E|chorus#($NAMES)$E|@domain: ($NAMES)$E|\"producer\": \"($NAMES)\"" \
    -- '*.ts' '*.rs' '*.py' '*.sh' '*.bats' '*.ttl' '*.js' 'platform/api/public/*.html' 'directing/*.html' 'designing/schemas/spine-events.json' \
    ':!*llvm*' ':!*/node_modules/*' ':!platform/tests/4489-retired-domains-stay-gone.test.sh' \
    ':!platform/scripts/4489-domains-apply.py' 2>/dev/null | grep -vE "$ALLOW" | grep -vE '^[^:]+\.ttl:[0-9]+:[[:space:]]*#' 
}

ledger_missing() { # $1 = ledger file; prints each of the five with no retirement from the domains graph
  python3 - "$1" <<'EOF'
import json, sys
B = "https://jeffbridwell.com/chorus#"
want = {"alerts-monitors", "athena-domain", "chorus-domain", "convergence-domain", "gates"}
seen = set()
for line in open(sys.argv[1]):
    line = line.strip()
    if not line:
        continue
    r = json.loads(line)
    s = r.get("retire_subject") or r.get("merge_subject") or ""
    if r.get("graph") == "urn:chorus:domains:domains" and s.startswith(B):
        seen.add(s[len(B):])
for n in sorted(want - seen):
    print(n)
EOF
}

check() { # $1 = repo root
  local root="$1" bad=0 out
  out="$(uses "$root")"
  if [ -n "$out" ]; then echo "FAIL: a retired domain is still used as a domain:"; echo "$out" | head -20; bad=1; fi
  out="$(ledger_missing "$root/designing/schemas/model-retirements.jsonl")"
  if [ -n "$out" ]; then echo "FAIL: the ledger does not retire from urn:chorus:domains:domains: $out"; bad=1; fi
  return $bad
}

if [ "${1:-}" = "--self-test" ]; then
  T="$(mktemp -d)"; trap 'rm -rf "$T"' EXIT
  pass=0; fail=0
  plant() { # $1 = label, $2 = file, $3 = content
    rm -rf "$T/r"; mkdir -p "$T/r"; git -C "$T/r" init -q
    mkdir -p "$T/r/designing/schemas" "$T/r/$(dirname "$2")"
    grep . "$ROOT/designing/schemas/model-retirements.jsonl" > "$T/r/designing/schemas/model-retirements.jsonl"
    printf '%s\n' "$3" > "$T/r/$2"
    git -C "$T/r" add -A
    if { check "$T/r" || true; } | grep -q FAIL; then echo "PASS NEGATIVE: $1 is red"; pass=$((pass+1)); else echo "FAIL NEGATIVE: $1 went green"; fail=$((fail+1)); fi
  }
  plant "a TS domain id 'gates'" platform/api/src/x.ts "const d = 'gates';"
  plant "a TTL edge to chorus:chorus-domain" roles/x/ontology/x.ttl "chorus:x chorus:hasDomain chorus:chorus-domain ."
  plant "a test tagged @domain: athena-domain" platform/tests/x.sh "# @domain: athena-domain"
  plant "an event produced by convergence-domain" designing/schemas/spine-events.json '{"producer": "convergence-domain"}'
  # the ledger negative: drop alerts-monitors from a copy of the ledger
  rm -rf "$T/r"; mkdir -p "$T/r/designing/schemas"; git -C "$T/r" init -q
  grep -v '"retire_subject": "https://jeffbridwell.com/chorus#alerts-monitors", "graph": "urn:chorus:domains:domains"' \
    "$ROOT/designing/schemas/model-retirements.jsonl" > "$T/r/designing/schemas/model-retirements.jsonl"
  if { check "$T/r" || true; } | grep -q 'ledger does not retire.*alerts-monitors'; then echo "PASS NEGATIVE: a ledger without alerts-monitors is red"; pass=$((pass+1)); else echo "FAIL NEGATIVE: a ledger without alerts-monitors went green"; fail=$((fail+1)); fi
  # the control: the alerts-monitors-domain twin (retired by #4125) is a different name and stays green
  rm -rf "$T/r"; mkdir -p "$T/r/designing/schemas" "$T/r/roles/x/ontology"; git -C "$T/r" init -q
  grep . "$ROOT/designing/schemas/model-retirements.jsonl" > "$T/r/designing/schemas/model-retirements.jsonl"
  printf '%s\n' "chorus:alerts-monitors-domain chorus:hosts chorus:x ." > "$T/r/roles/x/ontology/y.ttl"
  git -C "$T/r" add -A
  if check "$T/r" >/dev/null; then echo "PASS CONTROL: alerts-monitors-domain is not alerts-monitors"; pass=$((pass+1)); else echo "FAIL CONTROL: alerts-monitors-domain read as alerts-monitors"; fail=$((fail+1)); fi
  echo "=== Results: $pass passed, $fail failed ==="
  [ "$fail" -eq 0 ]
  exit $?
fi

check "$ROOT" || exit 1
echo "PASS: no code, test, page or TTL uses alerts-monitors, athena-domain, chorus-domain, convergence-domain or gates as a domain"
echo "PASS: the ledger retires all five from urn:chorus:domains:domains"
if [ "${1:-}" = "--live" ]; then
  python3 "$ROOT/platform/scripts/4489-domains-apply.py" --count | tail -1 || exit 1
fi
