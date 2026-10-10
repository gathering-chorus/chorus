#!/usr/bin/env bats
# @test-type: unit — hermetic: the crawler's --check-mapping seam reads files only, no walk, no store
# @domain: services
#
# #4472 AC3 — platform/config/service-instance-map.json links each of our
# launchd labels to the Service it runs (designing/data/service-instances.ttl),
# or names it "none". The crawl refuses a mapping that targets a Service nobody
# authored, and one that lists a werk slot (<label>.werk.<role>), which is
# service-werk by rule and goes stale whenever no card is up.

setup() {
  BIN="${CHORUS_CRAWL_BIN:-$BATS_TEST_DIRNAME/../services/chorus-crawl/target/release/chorus-crawl}"
  [ -x "$BIN" ] || BIN="$BATS_TEST_DIRNAME/../services/chorus-crawl/target/debug/chorus-crawl"
  [ -x "$BIN" ] || skip "chorus-crawl not built at $BIN"
  ROOT="$BATS_TEST_DIRNAME/../.."
  MAP="$ROOT/platform/config/service-instance-map.json"
  TTL="$ROOT/designing/data/service-instances.ttl"
}

@test "the real mapping names only authored Services, or none" {
  run "$BIN" services --check-mapping --mapping "$MAP" --services-ttl "$TTL"
  [ "$status" -eq 0 ] || { echo "$output"; false; }
  printf "%s\n" "$output" | grep -qF -- "every target an authored Service or none"
}

@test "NEGATIVE PROOF: a target nobody authored is refused" {
  printf '{"com.chorus.x": "service-nobody-wrote-this"}\n' > "$BATS_TEST_TMPDIR/map.json"
  run "$BIN" services --check-mapping --mapping "$BATS_TEST_TMPDIR/map.json" --services-ttl "$TTL"
  [ "$status" -eq 2 ]
  printf "%s\n" "$output" | grep -qF -- "unknown Service(s): service-nobody-wrote-this"
}

@test "NEGATIVE PROOF: a werk-slot entry is refused" {
  printf '{"com.chorus.api.werk.silas": "service-werk"}\n' > "$BATS_TEST_TMPDIR/map.json"
  run "$BIN" services --check-mapping --mapping "$BATS_TEST_TMPDIR/map.json" --services-ttl "$TTL"
  [ "$status" -eq 2 ]
  printf "%s\n" "$output" | grep -qF -- "werk-slot labels need no mapping entry"
}

@test "none names a unit with no design and is accepted" {
  printf '{"com.chorus.bare": "none"}\n' > "$BATS_TEST_TMPDIR/map.json"
  run "$BIN" services --check-mapping --mapping "$BATS_TEST_TMPDIR/map.json" --services-ttl "$TTL"
  [ "$status" -eq 0 ] || { echo "$output"; false; }
}
