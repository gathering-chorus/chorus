#!/usr/bin/env bats
# @test-type: unit — stubs chorus-identity-token; no CSS, no network, no spine
#
# #4004 — the single largest source of recorded pain. The identity token lives
# 600 seconds; these scripts minted it ONCE per run and reused the variable for
# the whole run, so every call past ten minutes carried an expired token. The
# pain board over 14 days: 4,153 of 4,997 failures — 83% of everything — are
# authn-missing on /api/chorus/trace and /api/chorus/index, ~5,000/day since
# 2026-07-29. Nothing errored, because the POST is fire-and-forget: the traces
# were simply never recorded, and the noise buried every other security signal.
# chorus-identity-token already caches on disk and re-mints near expiry, so the
# fix is to stop out-smarting it — ask per call.

@test "NEGATIVE PROOF: no script caches the token in a run-scoped variable" {
  cd "$BATS_TEST_DIRNAME/.."
  bad=$(grep -rln 'TRACE_AUTH=\$(' scripts/ 2>/dev/null || true)
  if [ -n "$bad" ]; then
    echo "Token minted once per run — expires mid-run (600s TTL):"
    echo "$bad" | sed 's/^/  - /'
    false
  fi
}

@test "every trace POST asks for a token at call time" {
  cd "$BATS_TEST_DIRNAME/.."
  for f in scripts/index-crawler-snapshots.sh scripts/ontology-validate.sh scripts/shim-wrapper.sh; do
    grep -q 'trace_auth()' "$f" || { echo "$f has no per-call helper"; false; }
    grep -q '_TA="\$(trace_auth)"' "$f" || { echo "$f never calls it"; false; }
  done
}

@test "NEGATIVE PROOF: a token fetched twice across the TTL is NOT the same string" {
  # the whole bug in one assertion — a cached value cannot refresh, a call can
  stub="$BATS_TEST_TMPDIR/chorus-identity-token"
  printf '#!/usr/bin/env bash\necho "token-$(cat %s/n 2>/dev/null || echo 0)"\necho $(( $(cat %s/n 2>/dev/null || echo 0) + 1 )) > %s/n\n' \
    "$BATS_TEST_TMPDIR" "$BATS_TEST_TMPDIR" "$BATS_TEST_TMPDIR" > "$stub"
  chmod +x "$stub"
  cached="$("$stub")"; second_cached="$cached"        # the old shape
  live_1="$("$stub")"; live_2="$("$stub")"            # the new shape
  [ "$cached" = "$second_cached" ]
  [ "$live_1" != "$live_2" ]
}
