#!/usr/bin/env bats
# @test-type: contract — runs policy-validate over fixture policies; no store, no door, no network.
# @domain: policies
# @card: 5020 · owner: kade
# What Jeff sees: `policy-validate --domain <name>` is the lint of one domain. It runs the
# policies that apply to that domain (appliesTo names it, or is unset: every domain) and
# lists each red with its rule and what broke it.
#
# AC1: --domain scopes by appliesTo; a {domain} check runs for that domain and holds at 0;
# a full run sums a {domain} check over every domain at its baseline. NEGATIVE: a failing
# row for x makes `--domain x` exit 1, and the same row does not touch y.
# AC4: no new runner: the lint is policy-validate's own flag.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PV="$REPO/platform/scripts/policy-validate"
  T="$BATS_TEST_TMPDIR"
  export POLICY_DOMAINS="x y z" TEXTFILE_DIR="$T/prom"
}

# rows: name|appliesTo (comma list, "" = every domain)|baseline|command
policies() {
  python3 - "$T/policies.json" "$@" <<'PY'
import json, sys
rows = []
for spec in sys.argv[2:]:
    name, applies, base, cmd = spec.split("|", 3)
    r = {"name": name, "label": f"rule of {name}", "comment": "c", "gate": "data", "status": "ratchet",
         "baseline": base, "checkKind": "command", "checkSpec": cmd}
    if applies:
        r["appliesTo"] = [f"chorus:{a}" for a in applies.split(",")]
    rows.append(r)
json.dump({"data": rows}, open(sys.argv[1], "w"))
PY
  export POLICIES_JSON="$T/policies.json"
}

# a command that prints one violation for domain x and none for any other
ONLY_X='[ "{domain}" = x ] && echo "x row broken" || true'

@test "AC1: --domain x runs the global rows and x's rows, not y's" {
  policies "global||0|true" "for-x|x|0|true" "for-y|y|0|true" "for-x-and-z|x,z|0|true"
  run "$PV" --domain x
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '^PASS global ' || return 1
  echo "$output" | grep -q '^PASS for-x ' || return 1
  echo "$output" | grep -q '^PASS for-x-and-z ' || return 1
  ! echo "$output" | grep -q 'for-y' || return 1
  echo "$output" | grep -q '3 checks, 0 above baseline, 0 could not run (domain x)' || return 1
}

@test "AC1: a {domain} check runs with the name filled in, holds at 0 for one domain, and lists its rule and violations" {
  policies "per-domain||5|$ONLY_X"
  run "$PV" --domain x
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL per-domain 1 > 0' || return 1
  echo "$output" | grep -q '  rule: rule of per-domain' || return 1
  echo "$output" | grep -q '  - x: x row broken' || return 1
}

@test "AC1 NEGATIVE (Silas): the row failing for x fails --domain x and passes --domain y" {
  policies "per-domain||0|$ONLY_X"
  run "$PV" --domain x
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  run "$PV" --domain y
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '^PASS per-domain 0 <= 0' || return 1
}

@test "AC1: a full run sums a {domain} check over every domain and holds the sum at its baseline" {
  policies "per-domain||1|echo {domain}"
  run "$PV"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '^FAIL per-domain 3 > 1' || return 1
  policies "per-domain||3|echo {domain}"
  run "$PV"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
}

@test "AC1: a full run runs a scoped {domain} check only over its appliesTo" {
  policies "scoped|x,z|2|echo {domain}"
  run "$PV"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q '^PASS scoped 2 <= 2' || return 1
}

@test "AC1 NEGATIVE: an unreadable domain list is exit 2, never a pass" {
  policies "per-domain||9|echo {domain}"
  POLICY_DOMAINS="" run "$PV"
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q 'BROKEN per-domain' || return 1
}

@test "AC1 NEGATIVE: a lint with no check for the domain is exit 2, and a bad name is refused" {
  policies "for-y|y|0|true"
  run "$PV" --domain x
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
  run "$PV" --domain 'x;rm'
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}

@test "AC1: --prom on a domain lint writes its own file and leaves the full run's alone" {
  policies "per-domain||0|$ONLY_X"
  run "$PV" --domain y --prom
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [ -s "$T/prom/chorus_policy_domain_y.prom" ]
  [ ! -e "$T/prom/chorus_policy.prom" ]
}

@test "AC4: no new runner: the lint is policy-validate's --domain, and the tree has no domain-lint tool" {
  grep -q -- '--domain <name>' "$PV"
  run bash -c "cd '$REPO' && git ls-files platform/scripts | grep -iE 'domain-?lint'"
  [ -z "$output" ] || { echo "$output"; return 1; }
}
