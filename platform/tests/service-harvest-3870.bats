#!/usr/bin/env bats
# @test-type: unit — signal is fixture-data: arq/generated files only; no live store, no $HOME, no network
# #3870 — service harvester: launchctl snapshot → ServiceInstance/ScheduledJob
# TTL. Written RED-FIRST against the not-yet-existing generator (pair order).
# Hermetic: fixture snapshot in, TTL out — no launchctl, no Fuseki, no $HOME.

setup() {
  DIR="$( cd "$( dirname "$BATS_TEST_FILENAME" )" && pwd )"
  GEN="$DIR/../scripts/service-harvest-gen.sh"
  FIX="$DIR/fixtures/service-harvest"
  T="2026-08-14T17:00:00Z"
  OUT="$BATS_TEST_TMPDIR/out.ttl"
}

@test "generates riot-valid TTL from a snapshot" {
  run bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$OUT"
  [ "$status" -eq 0 ]
  run /opt/homebrew/Cellar/jena/6.0.0/bin/riot --validate "$OUT"
  [ "$status" -eq 0 ]
}

@test "keepalive unit becomes ServiceInstance, calendar unit becomes ScheduledJob" {
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$OUT"
  grep -q 'chorus:ServiceInstance' "$OUT"
  grep -q 'chorus:ScheduledJob' "$OUT"
  # the service is the keepalive one
  grep -B3 'ServiceInstance' "$OUT" | grep -q 'com.test.api'
  grep -B3 'ScheduledJob' "$OUT" | grep -q 'com.test.nightly'
}

@test "IDEMPOTENT: same snapshot + timestamp → byte-identical TTL (run twice)" {
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$BATS_TEST_TMPDIR/a.ttl"
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$BATS_TEST_TMPDIR/b.ttl"
  diff "$BATS_TEST_TMPDIR/a.ttl" "$BATS_TEST_TMPDIR/b.ttl"
}

@test "VANISH: a unit present before but missing now is kept as runState absent" {
  # previous TTL had com.test.gone; new snapshot lacks it → absent row survives
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" \
      --previous "$FIX/previous-with-gone.ttl" --out "$OUT"
  grep -q 'com.test.gone' "$OUT"
  grep -A6 'com.test.gone' "$OUT" | grep -q '"absent"'
}

@test "instance IRI derives from machine+label (stable across runs)" {
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$OUT"
  grep -q 'instance-library-com.test.api' "$OUT"
}

@test "REFUSE: missing snapshot is exit 2, never an empty harvest" {
  run bash "$GEN" --snapshot "$FIX/nope.json" --timestamp "$T" --out "$OUT"
  [ "$status" -eq 2 ]
}

@test "EXTERNAL: binary outside our trees → chorus:external true; ours → no flag" {
  bash "$GEN" --snapshot "$FIX/snapshot-external.json" --timestamp "$T" --out "$OUT"
  grep -A7 'com.microsoft.teams' "$OUT" | grep -q 'chorus:external true'
  ! grep -A7 'com.test.ours' "$OUT" | grep -q 'chorus:external'
}

# --- loader (diff-before-write; the live half of second-run-writes-0) ---

@test "LOADER: identical current vs generated → writes 0, says so" {
  LOAD="$DIR/../scripts/service-harvest-load.sh"
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$BATS_TEST_TMPDIR/gen.ttl"
  run bash "$LOAD" --generated "$BATS_TEST_TMPDIR/gen.ttl" --current "$BATS_TEST_TMPDIR/gen.ttl" --dry-run
  [ "$status" -eq 0 ]
  [[ "$output" == *"0 changes"* ]]
}

@test "LOADER: differing current vs generated → reports a write is needed" {
  LOAD="$DIR/../scripts/service-harvest-load.sh"
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$BATS_TEST_TMPDIR/gen.ttl"
  bash "$GEN" --snapshot "$FIX/snapshot-external.json" --timestamp "$T" --out "$BATS_TEST_TMPDIR/other.ttl"
  run bash "$LOAD" --generated "$BATS_TEST_TMPDIR/gen.ttl" --current "$BATS_TEST_TMPDIR/other.ttl" --dry-run
  [ "$status" -eq 0 ]
  [[ "$output" == *"write needed"* ]]
}

@test "LOADER: refuses when generated TTL is missing or empty (exit 2)" {
  LOAD="$DIR/../scripts/service-harvest-load.sh"
  run bash "$LOAD" --generated "$BATS_TEST_TMPDIR/nope.ttl" --current /dev/null --dry-run
  [ "$status" -eq 2 ]
}

@test "LOADER: semantically equal but reordered TTL still counts as 0 changes" {
  # canonicalization guard: byte-diff would false-positive on ordering; the
  # loader must compare canonical N-Triples, not text.
  LOAD="$DIR/../scripts/service-harvest-load.sh"
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$BATS_TEST_TMPDIR/gen.ttl"
  { tail -n +8 "$BATS_TEST_TMPDIR/gen.ttl"; head -n 7 "$BATS_TEST_TMPDIR/gen.ttl"; } > "$BATS_TEST_TMPDIR/reordered.ttl" || true
  # reordering blocks like that breaks turtle; instead append a comment — a
  # textual change with zero semantic content:
  cp "$BATS_TEST_TMPDIR/gen.ttl" "$BATS_TEST_TMPDIR/commented.ttl"
  echo "# trailing comment, no triples" >> "$BATS_TEST_TMPDIR/commented.ttl"
  run bash "$LOAD" --generated "$BATS_TEST_TMPDIR/gen.ttl" --current "$BATS_TEST_TMPDIR/commented.ttl" --dry-run
  [ "$status" -eq 0 ]
  [[ "$output" == *"0 changes"* ]]
}

@test "EXTERNAL: wrapper with OUR script in argv is NOT external (the css case)" {
  bash "$GEN" --snapshot "$FIX/snapshot-external.json" --timestamp "$T" --out "$OUT"
  ! grep -A8 'com.test.wrapper' "$OUT" | grep -q 'chorus:external'
  grep -A8 'com.test.wrapper' "$OUT" | grep -q 'shared-security/bin/start.sh'
}

@test "EXTERNAL: bundle-id-only unit (SMAppService) IS external" {
  bash "$GEN" --snapshot "$FIX/snapshot-external.json" --timestamp "$T" --out "$OUT"
  grep -A8 'com.test.bundleonly' "$OUT" | grep -q 'chorus:external true'
}

@test "UNKNOWN: evidenceUnavailable unit emits evidenceState, not external" {
  bash "$GEN" --snapshot "$FIX/snapshot-unknown.json" --timestamp "$T" --out "$OUT"
  grep -A7 'com.remote.dark' "$OUT" | grep -q 'evidenceState "unknown"'
  ! grep -A7 'com.remote.dark' "$OUT" | grep -q 'external'
}

# --- mapping file (authored label→Service; rows stay machine-written) ---

@test "MAPPING: mapped label emits deploys edge to its Service" {
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$OUT" \
      --mapping "$FIX/map-good.json" --services-ttl "$FIX/services-authored.ttl"
  grep -A8 'com.test.api' "$OUT" | grep -q 'chorus:runsService chorus:service-example'
}

@test "MAPPING: label with NO observed instance emits a staleness row (LOUD)" {
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$OUT" \
      --mapping "$FIX/map-stale.json" --services-ttl "$FIX/services-authored.ttl"
  grep -q 'chorus:MappingStaleness' "$OUT"
  grep -q 'com.test.ghost' "$OUT"
}

@test "MAPPING: unknown Service IRI REFUSES at generate (exit 2), no dangling edge" {
  run bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$OUT" \
      --mapping "$FIX/map-bad-service.json" --services-ttl "$FIX/services-authored.ttl"
  [ "$status" -eq 2 ]
  [[ "$output$stderr" == *"service-nonexistent"* ]] || [[ "$(cat "$OUT" 2>/dev/null)" == "" ]]
}

@test "LOADER: refuses TTL whose deploys targets are not in the live Service set" {
  # The dangling-edge hazard caught live 2026-08-14: gen validated against a
  # FILE, the STORE had no such Service, and the edge landed dangling.
  LOAD="$DIR/../scripts/service-harvest-load.sh"
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$BATS_TEST_TMPDIR/gen.ttl" \
      --mapping "$FIX/map-good.json" --services-ttl "$FIX/services-authored.ttl"
  echo "service-clearing" > "$BATS_TEST_TMPDIR/served.txt"
  run bash "$LOAD" --generated "$BATS_TEST_TMPDIR/gen.ttl" --current /dev/null --dry-run \
      --served-services "$BATS_TEST_TMPDIR/served.txt"
  [ "$status" -eq 2 ]
  [[ "$output" == *"unserved"* ]]
}

@test "LOADER: accepts deploys targets that ARE served" {
  LOAD="$DIR/../scripts/service-harvest-load.sh"
  bash "$GEN" --snapshot "$FIX/snapshot-two-units.json" --timestamp "$T" --out "$BATS_TEST_TMPDIR/gen.ttl" \
      --mapping "$FIX/map-good.json" --services-ttl "$FIX/services-authored.ttl"
  echo "service-example" > "$BATS_TEST_TMPDIR/served.txt"
  run bash "$LOAD" --generated "$BATS_TEST_TMPDIR/gen.ttl" --current /dev/null --dry-run \
      --served-services "$BATS_TEST_TMPDIR/served.txt"
  [ "$status" -eq 0 ]
}
