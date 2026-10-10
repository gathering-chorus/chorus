#!/usr/bin/env bats
# @test-type: unit — the check under test reads a fake HOME and a saved Loki answer; the rest of chorus-health still reads live, read-only
# @domain: services
# #4472 — chorus-health deployed-equals-running compares the installed chorus-hooks with the
# last binary.deployed event that installed THAT file. A werk-slot install (target=werk) and a
# chorus-hook-shim install are other files and must not count.

setup() {
  ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  T="$(mktemp -d)"
  mkdir -p "$T/home/.chorus/bin"
  cp /bin/echo "$T/home/.chorus/bin/chorus-hooks"
  HOOKS_CDHASH=$(codesign -d --verbose=4 "$T/home/.chorus/bin/chorus-hooks" 2>&1 | grep "CandidateCDHash sha256=" | awk '{print $NF}' | cut -d= -f2)
  [ -n "$HOOKS_CDHASH" ]
}

teardown() { rm -rf "$T"; }

# fixture <ts binary target cdhash>... → a Loki query_range answer
fixture() {
  python3 - "$T/deploys.json" "$@" <<'PY'
import json, sys
out, rest = sys.argv[1], sys.argv[2:]
vals = []
for i in range(0, len(rest), 4):
    ts, b, t, c = rest[i:i+4]
    ev = {"event": "binary.deployed", "binary": b, "cdhash": c}
    if t != "-": ev["target"] = t
    vals.append([ts, json.dumps(ev)])
json.dump({"data": {"result": [{"values": vals}]}}, open(out, "w"))
PY
}

line() {
  HOME="$T/home" CHORUS_HEALTH_DEPLOYS_FIXTURE="$T/deploys.json" bash "$ROOT/platform/scripts/chorus-health" 2>&1 | grep "deployed-equals-running"
}

@test "a later werk-slot install does not read as drift" {
  fixture 2 chorus-hooks werk 803afa5f990dab38b4d69c8e0c0b51fffb09eb9c 1 chorus-hooks - "$HOOKS_CDHASH"
  out=$(line)
  printf '%s\n' "$out" | grep -qF "cdhash matches"
}

@test "a later chorus-hook-shim install does not read as drift" {
  fixture 2 chorus-hook-shim - 803afa5f990dab38b4d69c8e0c0b51fffb09eb9c 1 chorus-hooks - "$HOOKS_CDHASH"
  out=$(line)
  printf '%s\n' "$out" | grep -qF "cdhash matches"
}

@test "negative proof: a real install with another cdhash IS drift" {
  fixture 2 chorus-hooks - 803afa5f990dab38b4d69c8e0c0b51fffb09eb9c 1 chorus-hooks - "$HOOKS_CDHASH"
  out=$(line)
  printf '%s\n' "$out" | grep -qF "binary drift"
}

@test "an install that recorded no cdhash cannot verify, it does not pass" {
  fixture 1 chorus-hooks - unknown
  out=$(line)
  printf '%s\n' "$out" | grep -qF "cannot verify"
}
