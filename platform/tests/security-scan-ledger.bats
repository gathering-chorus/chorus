#!/usr/bin/env bats
# @test-type: unit — the ledger checker on a fixture report; trivy on a tmpdir (secret scanner, no DB)
# @domain: security
# @card: #4879 · owner: silas
# What Jeff sees: the weekly security scan reports a real finding, never a
# phantom. On 11-01 one exempted finding read as both "new" and "stale" because
# semgrep names a rule by the folder it ran from, and trivy stopped inside a
# cargo build dir and measured nothing.

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CHECK="$ROOT/platform/scripts/sast-ledger-check.py"

setup() {
  T="$BATS_TEST_TMPDIR"
  cat > "$T/ledger.json" <<'J'
{"exemptions":[{"path":"directing/clearing/src/server.ts","rule":"platform.security.semgrep.chorus-caller-supplied-identity-to-sink","reason":"fixture","since":"2026-09-01","owner":"abby-normal"}]}
J
}

report() { # report <rule id as semgrep printed it> [path]
  printf '%s\n   ❯❯❱ %s\n' "${2:-directing/clearing/src/server.ts}" "$1"
}

@test "an exempted finding matches its ledger entry whatever folder semgrep ran from" {
  for rule in platform.security.semgrep.chorus-caller-supplied-identity-to-sink \
              Users.jeffbridwell.CascadeProjects.chorus.platform.security.semgrep.chorus-caller-supplied-identity-to-sink \
              security.semgrep.chorus-caller-supplied-identity-to-sink; do
    run python3 "$CHECK" "$T/ledger.json" 1 <<< "$(report "$rule")"
    [ "$status" -eq 0 ] || { echo "$rule → $output"; return 1; }
  done
}

@test "negative proof: an unexempted finding still fails, under any prefix" {
  run python3 "$CHECK" "$T/ledger.json" 1 <<< "$(report Users.x.platform.security.semgrep.chorus-caller-supplied-identity-to-sink)
$(report Users.x.platform.security.semgrep.chorus-other-rule directing/clearing/src/other.ts)"
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"other.ts"*"chorus-other-rule"* ]] || { echo "$output"; return 1; }
}

@test "negative proof: a ledger entry with no finding still reads stale" {
  run python3 "$CHECK" "$T/ledger.json" 1 <<< "$(report Users.x.platform.security.semgrep.chorus-other-rule directing/clearing/src/other.ts)"
  [ "$status" -ne 0 ] || return 1
  [[ "$output" == *"STALE"*"server.ts"* ]] || { echo "$output"; return 1; }
}

@test "trivy skips cargo's build caches, and still finds the same secret elsewhere in target/ (negative proof)" {
  command -v trivy >/dev/null || skip "trivy not installed"
  mkdir -p "$T/a/target/release/.fingerprint/x" "$T/a/target/release/incremental/y" "$T/a/target/release/deps"
  # a fake AWS key, assembled at run time so no secret literal sits in the repo
  local k="AKIA""IOSFODNN7EXAMPLQ" v="wJalrXUtnFEMI/K7MDENG/""bPxRfiCYEXAMPLEKEY"
  printf '%s\naws_secret_access_key = "%s"\n' "$k" "$v" > "$T/a/target/release/deps/leak.env"
  cp "$T/a/target/release/deps/leak.env" "$T/a/target/release/.fingerprint/x/leak.env"
  cp "$T/a/target/release/deps/leak.env" "$T/a/target/release/incremental/y/leak.env"
  skip_dirs=$(python3 -c 'import re,sys; print(re.search(r"TRIVY_SKIP_DIRS:-([^}]*)\}", open(sys.argv[1]).read()).group(1))' "$ROOT/platform/scripts/test-security-scan.sh")
  [ "$skip_dirs" = "**/.fingerprint,**/incremental" ] || { echo "skip=$skip_dirs"; return 1; }
  run env TRIVY_SKIP_DIRS="$skip_dirs" trivy fs --scanners secret --format json --quiet "$T/a"
  targets=$(echo "$output" | python3 -c 'import json,sys; print(" ".join(r["Target"] for r in json.load(sys.stdin).get("Results",[])))')
  [[ "$targets" == *"target/release/deps/leak.env"* ]] || { echo "$targets"; return 1; }
  [[ "$targets" != *".fingerprint"* && "$targets" != *"incremental"* ]] || { echo "$targets"; return 1; }
}
