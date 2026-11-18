#!/usr/bin/env bats
# @test-type: e2e — after the land: the live door, read-only (negative proofs run anywhere)
# @domain: domains
# @card: #5002
#
# #5002 AC4 — no zz-probe row left outside the security graph. Split from no-ownerless-rows-live.bats so each AC names its own check.
# Skips in a card's werk: it measures what landed.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  DOOR="${ATHENA_MAKE_URL:-http://localhost:3360}"
}

in_werk() { case "${BATS_TEST_DIRNAME}" in */chorus-werk/*) return 0;; esac; return 1; }

probes_left() {
  python3 - "$1" <<'PY'
import sys
left = [f"{c}/{n}" for c, n, g in (l.rstrip("\n").split("\t") for l in open(sys.argv[1]) if l.strip())
        if n.startswith("zz-probe-") and g != "urn:chorus:domains:security"]
print(f"zz-probe rows left: {len(left)}" + (": " + ", ".join(left[:10]) if left else ""))
sys.exit(1 if left else 0)
PY
}

listed_names() { # every listed row name per collection, Version aside (its probe rows go by ofRow, AC4's ledger line)
  python3 - "$DOOR" <<'PY'
import json, sys, urllib.request
door = sys.argv[1]
for p in json.load(urllib.request.urlopen(door + "/", timeout=20))["primitives"]:
    if p["kind"] == "Version":
        continue
    cur = p["collection"] + "?limit=1000"
    while cur:
        d = json.load(urllib.request.urlopen(door + cur, timeout=60))
        for r in d.get("data", []):
            print(f"{p['kind']}\t{r.get('name','')}\t{d.get('servedFrom','')}")
        cur = (d.get("links") or {}).get("next")
PY
}

@test "AC4 no zz-probe row is left outside the security graph" {
  in_werk && skip "after-land check: a card's werk has not landed"
  listed_names > "$BATS_TEST_TMPDIR/names"
  [ -s "$BATS_TEST_TMPDIR/names" ] || { echo "the door listed nothing"; return 1; }
  run probes_left "$BATS_TEST_TMPDIR/names"
  echo "$output"
  [ "$status" -eq 0 ] || return 1
}

@test "AC4 NEGATIVE PROOF: a probe row present is red; a security probe and a clean list pass" {
  printf 'Language\trust\turn:chorus:domains:code\nPrincipal\tzz-probe-x\turn:chorus:domains:security\n' > "$BATS_TEST_TMPDIR/clean"
  run probes_left "$BATS_TEST_TMPDIR/clean"
  [ "$status" -eq 0 ] || return 1
  printf 'Language\tzz-probe-1-language\turn:chorus:domains:code\n' > "$BATS_TEST_TMPDIR/probe"
  run probes_left "$BATS_TEST_TMPDIR/probe"
  [ "$status" -eq 1 ] || return 1
}
