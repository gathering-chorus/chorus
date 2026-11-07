#!/usr/bin/env bats
# @test-type: contract — runs platform/scripts/deploy-drift over DeployTarget fixtures
# and files in BATS_TEST_TMPDIR; no store, no network.
# @domain: deploys
# #4919 AC3 — after a deploy, every DeployTarget is compared with what is really
# in prod; any difference fails, naming the target and what differs. The
# negative proofs are the two drifts of 2026-11-07: a key at mode 660 (07:10)
# and a principal row at hostAccount=jeffbridwell (01:18), plus a settings.json
# edited live so it no longer matches git.

setup() {
  REPO="${CHORUS_ROOT:-$(cd "$BATS_TEST_DIRNAME/../.." && pwd)}"
  DRIFT="$REPO/platform/scripts/deploy-drift"
  T="$BATS_TEST_TMPDIR"
  ME=$(id -un)
  mkdir -p "$T/identity/silas" "$T/git" "$T/live"
  echo '{"k":1}' > "$T/identity/silas/nostr.json"; chmod 600 "$T/identity/silas/nostr.json"
  echo '{"model":"opus"}' > "$T/git/settings.json"; cp "$T/git/settings.json" "$T/live/settings.json"; chmod 644 "$T/live/settings.json"
  echo '"hostAccount": "chorus-silas"' > "$T/principals.json"
  cat > "$T/targets.ttl" <<EOF
@prefix chorus: <https://jeffbridwell.com/chorus#> .
chorus:t-key a chorus:DeployTarget ; chorus:path "$T/identity/silas/nostr.json" ; chorus:ownerAccount "$ME" ;
  chorus:fileMode "0600" ; chorus:source "minted" ; chorus:targetKind "secret" ;
  chorus:healthCheck "true" .
chorus:t-settings a chorus:DeployTarget ; chorus:path "$T/live/settings.json" ; chorus:ownerAccount "$ME" ;
  chorus:fileMode "0644" ; chorus:source "$T/git/settings.json" ; chorus:targetKind "config" ;
  chorus:healthCheck "true" .
chorus:t-principals a chorus:DeployTarget ; chorus:path "$T/principals.json" ; chorus:ownerAccount "$ME" ;
  chorus:fileMode "0644" ; chorus:source "model" ; chorus:targetKind "model" ;
  chorus:healthCheck "grep -q 'hostAccount\": \"chorus-silas' $T/principals.json" .
EOF
  chmod 644 "$T/principals.json"
}

@test "control: prod matches every target, drift exits 0 and says so" {
  run "$DRIFT" "$T/targets.ttl"
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "no drift" || return 1
}

@test "negative proof: a key at mode 660 is drift, named" {
  chmod 660 "$T/identity/silas/nostr.json"
  run "$DRIFT" "$T/targets.ttl"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "t-key.*mode 0660, want 0600" || { echo "$output"; return 1; }
}

@test "negative proof: a principal at hostAccount=jeffbridwell fails its health check, named" {
  echo '"hostAccount": "jeffbridwell"' > "$T/principals.json"
  run "$DRIFT" "$T/targets.ttl"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "t-principals.*health check failed" || { echo "$output"; return 1; }
}

@test "negative proof: a settings.json edited live no longer matches git, named" {
  echo '{"model":"other"}' > "$T/live/settings.json"
  run "$DRIFT" "$T/targets.ttl"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "t-settings.*differs from source" || { echo "$output"; return 1; }
}

@test "negative proof: a missing target is drift, never a pass" {
  rm "$T/identity/silas/nostr.json"
  run "$DRIFT" "$T/targets.ttl"
  [ "$status" -eq 1 ] || { echo "$output"; return 1; }
  echo "$output" | grep -q "t-key.*missing" || { echo "$output"; return 1; }
}

@test "an unreadable or empty targets file errors (exit 2), never reads as no drift" {
  : > "$T/empty.ttl"
  run "$DRIFT" "$T/empty.ttl"
  [ "$status" -eq 2 ] || { echo "$output"; return 1; }
}
