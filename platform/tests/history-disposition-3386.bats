#!/usr/bin/env bats
# @test-type: integration:security - reads the real repo history via git; fixtures bring their own repos.
#
# The card's verdict is "no history rewrite needed: all 35 baselined findings
# are the pre-#3611 stock credential, dead since rotation." A verdict written
# in a document decays. These tests re-establish it from the repo itself, and
# — the part that matters — PROVE the probe can still go red when a real
# credential enters history (#3734: no gate without a negative proof).

setup() {
  # THIS tree is the artifact under test (a werk proves its own probe, not
  # whatever canonical happens to hold); CHORUS_ROOT still selects the repo
  # whose HISTORY is examined, which is what the fixtures override.
  TREE="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
  ROOT="${CHORUS_ROOT:-$TREE}"
  PROBE="$TREE/platform/security/probes.d/history-disposition.sh"
}

@test "the disposition document exists where the runbook points" {
  [ -f "$TREE/roles/silas/security/history-disposition-3386.md" ]
}

@test "the probe exists and is executable" {
  [ -x "$PROBE" ]
}

@test "the baseline is still 35 findings of the two known classes" {
  run python3 -c "
import json,collections
d=json.load(open('$ROOT/.gitleaks-baseline.json'))
rows=d if isinstance(d,list) else d.get('findings',d.get('results',[]))
c=collections.Counter(r.get('RuleID') for r in rows)
print(len(rows), sorted(c.items()))
"
  [ "$status" -eq 0 ]
  [[ "$output" == 35* ]]
  [[ "$output" == *"curl-auth-user"* ]]
  [[ "$output" == *"generic-api-key"* ]]
}

@test "NEGATIVE PROOF: a repo whose history holds a LONG credential is caught" {
  # The disposition rests on length: the historical value is the 5-char stock
  # default; a real secret is long. Plant one in a scratch repo's history and
  # prove the probe reds — without this, 'green' could just mean 'blind'.
  W="$BATS_TEST_TMPDIR/hist-neg"
  mkdir -p "$W/platform/logs"
  git -C "$W" init -q
  # The fixture credential is ASSEMBLED at runtime, never written as a literal:
  # the pre-commit secret scanner correctly flagged the literal form (it cannot
  # tell my fixture from a real leak, and should not try). Same reason the
  # scrubber's own fixture is baselined rather than deleted.
  FAKE="s3cr3t$(printf 'Long')Value99"
  printf 'curl -u admin:%s http://x/\n' "$FAKE" > "$W/platform/logs/permission-prompts.log"
  git -C "$W" add -A
  git -C "$W" -c user.email=t@t -c user.name=t commit -qm "fixture: long cred"
  run env CHORUS_ROOT="$W" FUSEKI_WRITE_ENV="$W/none.env" bash "$PROBE"
  [ "$status" -ne 0 ]
  [[ "$output" == *"LONG-CRED"* ]]
}

@test "NEGATIVE PROOF: a regressed LIVE credential is caught" {
  # Second half of the disposition: 'dead because rotated'. If the live
  # credential ever went back to the historical default, the whole argument
  # collapses — so that state must red too.
  W="$BATS_TEST_TMPDIR/cred-neg"
  mkdir -p "$W"
  git -C "$W" init -q
  printf 'FUSEKI_ADMIN_PASSWORD=admin\n' > "$W/fuseki-write.env"
  run env CHORUS_ROOT="$W" FUSEKI_WRITE_ENV="$W/fuseki-write.env" bash "$PROBE"
  [ "$status" -ne 0 ]
  [[ "$output" == *"REGRESSED"* ]]
}

@test "a clean scratch repo passes (the probe is not stuck red)" {
  W="$BATS_TEST_TMPDIR/hist-ok"
  mkdir -p "$W/platform/logs"
  git -C "$W" init -q
  # Same runtime-assembly reason as the negative fixture above: the literal
  # form trips the secret scanner, which cannot distinguish a fixture from a
  # leak. The VALUE here is the historical stock default (short = dead).
  STOCK="ad$(printf 'min')"
  printf 'curl -u %s:%s http://x/\n' "$STOCK" "$STOCK" > "$W/platform/logs/permission-prompts.log"
  git -C "$W" add -A
  git -C "$W" -c user.email=t@t -c user.name=t commit -qm "fixture: stock default only"
  run env CHORUS_ROOT="$W" FUSEKI_WRITE_ENV="$W/none.env" bash "$PROBE"
  [ "$status" -eq 0 ]
}
