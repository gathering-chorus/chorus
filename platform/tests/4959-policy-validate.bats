#!/usr/bin/env bats
# @test-type: contract — runs policy-validate over fixture policies and a fixture dataset
# @domain: policies — the product domain this suite guards (#4334)
# @card: #4959
# @owner: silas
# in BATS_TEST_TMPDIR; no store, no door, no network.
# #4959 AC3 — policy-validate reads the policies, runs each check, and fails only
# when a check finds MORE violations than its baseline. It never passes on a check
# it could not run: an unreadable door or a broken query is exit 2, not green (#3734).

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  PV="$REPO/platform/scripts/policy-validate"
  T="$BATS_TEST_TMPDIR"
  # two subjects typed in two graphs: the one-home rule finds 2 violations here
  cat > "$T/data.trig" <<'EOF'
@prefix c: <https://jeffbridwell.com/chorus#> .
<urn:chorus:ontology> { c:pulse a c:Product . c:werk a c:Product . }
<urn:chorus:instances> { c:pulse a c:Product . c:werk a c:Product . }
EOF
  Q='SELECT DISTINCT ?s WHERE { GRAPH <urn:chorus:ontology> { ?s a ?t1 } GRAPH <urn:chorus:instances> { ?s a ?t2 } }'
  export POLICY_DATA="$T/data.trig"
}

# policies.json in the door's GET shape: {"data": [ ... ]}
policies() {
  python3 - "$T/policies.json" "$@" <<'PY'
import json, sys
out, rows = sys.argv[1], []
for spec in sys.argv[2:]:
    name, gate, status, base, kind, check = spec.split("|", 5)
    r = {"name": name, "label": name, "comment": "c"}
    if gate: r["gate"] = gate
    if status: r["status"] = status
    # "s2" sends the baseline as the string "2", the way the door serves integers
    if base: r["baseline"] = base[1:] if base.startswith("s") else int(base)
    if kind: r["checkKind"] = kind; r["checkSpec"] = check
    rows.append(r)
json.dump({"data": rows}, open(out, "w"))
PY
  export POLICIES_JSON="$T/policies.json"
}

@test "a check at its baseline PASSES (exit 0) and says so" {
  policies "one-home|data|ratchet|2|sparql|$Q"
  run "$PV"
  [ "$status" -eq 0 ]
  [[ "$output" == *"PASS one-home 2 <= 2"* ]] || return 1
}

@test "negative: one violation over the baseline FAILS (exit 1) and names the policy" {
  policies "one-home|data|ratchet|1|sparql|$Q"
  run "$PV"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL one-home 2 > 1"* ]] || return 1
}

@test "--gate runs only that gate's policies" {
  policies "one-home|data|active|0|sparql|$Q" "other|arch|active|5|sparql|$Q"
  run "$PV" --gate arch
  [ "$status" -eq 0 ]
  [[ "$output" == *"PASS other"* ]] || return 1
  [[ "$output" != *"one-home"* ]] || return 1
}

@test "a retired policy does not run" {
  policies "one-home|data|retired|0|sparql|$Q"
  run "$PV"
  [ "$status" -eq 0 ]
  [[ "$output" != *"FAIL"* ]] || return 1
}

@test "a plain policy with no check is not counted as passing or failing" {
  policies "english-only|||||"
  run "$PV"
  [ "$status" -eq 0 ]
  [[ "$output" == *"0 checks"* ]] || return 1
}

@test "negative: a broken query is exit 2, never a pass" {
  policies "broken|data|active|99|sparql|SELECT ?s WHERE { this is not sparql"
  run "$PV"
  [ "$status" -eq 2 ]
  [[ "$output" == *"broken"* ]] || return 1
}

@test "negative: an unreadable door is exit 2, never a pass" {
  export POLICIES_JSON="$T/missing.json"
  run "$PV"
  [ "$status" -eq 2 ]
}

@test "a command check counts the lines it prints as violations" {
  policies "two-lines|code|active|1|command|printf 'a\nb\n'"
  run "$PV"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL two-lines 2 > 1"* ]] || return 1
}

@test "a baseline the door serves as a string (\"2\") is read as 2" {
  policies "one-home|data|ratchet|s2|sparql|$Q"
  run "$PV"
  [ "$status" -eq 0 ]
  [[ "$output" == *"PASS one-home 2 <= 2"* ]] || return 1
}

@test "negative: a check with no baseline is exit 2, never a pass" {
  policies "nobase|data|active||sparql|$Q"
  run "$PV"
  [ "$status" -eq 2 ]
}

@test "negative: a command that cannot run (exit 127, no output) is exit 2, never 0 <= baseline" {
  policies "gone|code|active|5|command|no-such-command-4959"
  run "$PV"
  [ "$status" -eq 2 ]
  [[ "$output" == *"BROKEN gone"* ]] || return 1
}

@test "a test check: a failing suite is one violation over a 0 baseline" {
  printf '#!/usr/bin/env bats\n@test "red" { false; }\n' > "$T/red.bats"
  policies "red-suite|quality|active|0|test|$T/red.bats"
  run "$PV"
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL red-suite 1 > 0"* ]] || return 1
}

@test "a test check: a passing suite is 0 violations" {
  printf '#!/usr/bin/env bats\n@test "green" { true; }\n' > "$T/green.bats"
  policies "green-suite|quality|active|0|test|$T/green.bats"
  run "$PV"
  [ "$status" -eq 0 ]
}

@test "negative: a test check whose suite is missing is exit 2, never a violation count" {
  policies "missing-suite|quality|active|0|test|$T/nope.bats"
  run "$PV"
  [ "$status" -eq 2 ]
}

@test "--prom writes each policy's violations and baseline for node-exporter" {
  export TEXTFILE_DIR="$T/tf"
  policies "one-home|data|ratchet|2|sparql|$Q"
  run "$PV" --prom
  [ "$status" -eq 0 ]
  grep -q '^chorus_policy_violations{policy="one-home",gate="data"} 2$' "$T/tf/chorus_policy.prom" || return 1
  grep -q '^chorus_policy_baseline{policy="one-home",gate="data"} 2$' "$T/tf/chorus_policy.prom" || return 1
  grep -q '^chorus_policy_checks 1$' "$T/tf/chorus_policy.prom" || return 1
}

@test "negative: --prom on a run that could not measure leaves the old file untouched (the stale rule fires)" {
  export TEXTFILE_DIR="$T/tf"
  mkdir -p "$T/tf"
  echo "old" > "$T/tf/chorus_policy.prom"
  touch -t 202601010000 "$T/tf/chorus_policy.prom"
  policies "broken|data|active|0|sparql|SELECT ?s WHERE { nope"
  run "$PV" --prom
  [ "$status" -eq 2 ]
  [ "$(cat "$T/tf/chorus_policy.prom")" = "old" ]
  [ -z "$(find "$T/tf" -name 'chorus_policy.prom' -newer "$T/data.trig")" ]
}

@test "--runtime runs only the runtime checks, as commands (AC6, for borg-prove in prod)" {
  policies "bind-local|arch|active|0|runtime|printf 'svc-a binds 0.0.0.0\n'" "one-home|data|active|0|sparql|$Q"
  run "$PV" --runtime
  [ "$status" -eq 1 ]
  [[ "$output" == *"FAIL bind-local 1 > 0"* ]] || return 1
  [[ "$output" != *"one-home"* ]] || return 1
}

@test "without --runtime a runtime check is skipped, never counted" {
  policies "bind-local|arch|active|0|runtime|printf 'x\n'"
  run "$PV"
  [ "$status" -eq 0 ]
  [[ "$output" == *"SKIP bind-local"* ]] || return 1
}
