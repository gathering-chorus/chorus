#!/usr/bin/env bats
# @test-type: unit — signal is fixture-data: jena update over files only; no live store, no $HOME, no network
# #4089 — the services harvester replaces the CLASSES it emits, never the graph.
# Wren's ask (2026-09-03 10:16): a negative proof that a hand-authored Commitment
# row in urn:chorus:domains:services survives a harvest cycle — that is the
# state the fix exists to protect. The old wholesale replace is applied to the
# same fixture and shown to lose the row, so the fixture can tell the two apart.
setup() {
  LOAD="$BATS_TEST_DIRNAME/../scripts/service-harvest-load.sh"
  UPDATE="$(command -v update || true)"
  [ -n "$UPDATE" ] || skip "jena update CLI not on PATH"
  G="urn:chorus:domains:services"
  cat > "$BATS_TEST_TMPDIR/generated.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:unit-new a chorus:ServiceInstance ; chorus:label "new instance" .
chorus:job-new  a chorus:ScheduledJob ; chorus:label "new job" .
TTL
  # the graph BEFORE the cycle: a stale instance (ours to replace), a hand-authored
  # Commitment and a Service row (co-tenants, not ours)
  cat > "$BATS_TEST_TMPDIR/before.trig" <<'TRIG'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
<urn:chorus:domains:services> {
  chorus:unit-stale a chorus:ServiceInstance ; chorus:label "stale instance" .
  chorus:commitment-keep-me a chorus:Commitment ; chorus:statement "hand-authored, must survive" .
  chorus:service-tests a chorus:Service ; chorus:label "Tests" .
}
TRIG
}

@test "the harvest update replaces only the harvested classes; the Commitment and Service rows survive" {
  run bash "$LOAD" --generated "$BATS_TEST_TMPDIR/generated.ttl" --print-update
  [ "$status" -eq 0 ] || { echo "$output"; false; }
  printf '%s\n' "$output" > "$BATS_TEST_TMPDIR/u.ru"
  after="$("$UPDATE" --data="$BATS_TEST_TMPDIR/before.trig" --update="$BATS_TEST_TMPDIR/u.ru" --dump 2>/dev/null)"
  echo "$after" | grep -q "commitment-keep-me"   || { echo "$after"; false; }
  echo "$after" | grep -q "service-tests"        || { echo "$after"; false; }
  echo "$after" | grep -q "unit-new"             || { echo "$after"; false; }
  echo "$after" | grep -q "job-new"              || { echo "$after"; false; }
  ! echo "$after" | grep -q "unit-stale"         || { echo "$after"; false; }
}

@test "NEGATIVE PROOF: the old wholesale replace loses the Commitment on the same fixture" {
  # what the loader did before #4089 (graph-store PUT = clear + insert), as an update
  printf 'PREFIX chorus: <https://jeffbridwell.com/chorus#>\nCLEAR GRAPH <%s> ;\nINSERT DATA { GRAPH <%s> { chorus:unit-new a chorus:ServiceInstance } }\n' "$G" "$G" > "$BATS_TEST_TMPDIR/old.ru"
  after="$("$UPDATE" --data="$BATS_TEST_TMPDIR/before.trig" --update="$BATS_TEST_TMPDIR/old.ru" --dump 2>/dev/null)"
  ! echo "$after" | grep -q "commitment-keep-me" || { echo "the fixture cannot detect the loss"; echo "$after"; false; }
  echo "$after" | grep -q "unit-new"
}

@test "a co-tenant row alone is not a difference: unchanged harvest over a graph with a Commitment writes nothing" {
  cat > "$BATS_TEST_TMPDIR/current.ttl" <<'TTL'
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:unit-new a chorus:ServiceInstance ; chorus:label "new instance" .
chorus:job-new  a chorus:ScheduledJob ; chorus:label "new job" .
chorus:commitment-keep-me a chorus:Commitment ; chorus:statement "hand-authored, must survive" .
TTL
  run bash "$LOAD" --generated "$BATS_TEST_TMPDIR/generated.ttl" --current "$BATS_TEST_TMPDIR/current.ttl" --dry-run
  [ "$status" -eq 0 ] || { echo "$output"; false; }
  echo "$output" | grep -q "0 changes" || { echo "$output"; false; }
}

@test "a harvest that declares no class is refused, never written as delete-nothing-insert-all" {
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:x chorus:label "typeless" .\n' > "$BATS_TEST_TMPDIR/typeless.ttl"
  run bash "$LOAD" --generated "$BATS_TEST_TMPDIR/typeless.ttl" --print-update
  [ "$status" -eq 2 ]
  echo "$output" | grep -q "owns no class"
}

@test "NEGATIVE PROOF: a harvest that types a subject as chorus:Service is REFUSED, never a wider delete" {
  printf '@prefix chorus: <https://jeffbridwell.com/chorus#> .\nchorus:unit-new a chorus:ServiceInstance .\nchorus:service-oops a chorus:Service ; chorus:label "emitted by mistake" .\n' > "$BATS_TEST_TMPDIR/oops.ttl"
  run bash "$LOAD" --generated "$BATS_TEST_TMPDIR/oops.ttl" --print-update
  [ "$status" -eq 2 ]
  echo "$output" | grep -q "does not own"
  ! echo "$output" | grep -q "DELETE"
}
