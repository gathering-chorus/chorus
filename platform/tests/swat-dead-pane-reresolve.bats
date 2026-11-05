#!/usr/bin/env bats
# @test-type: unit — the principal's pane refresh and pulse's dead-pane verdict, on fixtures
# @domain: messages
# @card: #4889 · owner: silas
# What Jeff sees: a role's address follows the role like a phone number (Jeff 07:48).
# One path: the role's live registration. platform/services/chorus-principal/src/rows.rs
# (repaned_presence, called from lib.rs on every turn and every sweep) re-records the
# pane the role's tmux session shows; the principal is the Presence's only writer.
# platform/pulse/src/delivery-worker.ts makes a gone pane one loud not-delivered (no
# retry, no other pane), and platform/pulse/src/service.ts / presence-target.ts no longer
# hold the #4362 tmux fallback (rolePaneFromTmux).
ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
PR="$ROOT/platform/services/chorus-principal"
PULSE="$ROOT/platform/pulse"

# pulse sources (not tests) that still find a pane on their own, one per line
own_pane_lookup() {
  grep -rlE "rolePaneFromTmux|paneFromTmuxListing|list-panes" "$1" --include='*.ts' | grep -v '\.test\.ts$' || true
}

@test "a moved pane is re-recorded by the principal; the same pane, or no pane id, writes nothing" {
  run env CARGO_BUILD_JOBS=2 CARGO_TARGET_DIR="$PR/target/$(id -un)" nice cargo test -q --manifest-path "$PR/Cargo.toml" --lib a_moved_pane_is_taken_4889
  [ "$status" -eq 0 ] || { echo "$output" | tail -20; return 1; }
  [[ "$output" == *"1 passed"* ]] || { echo "$output" | tail -5; return 1; }
}

@test "a dead pane is one not-delivered with its reason; a live pane delivers" {
  run bash -c "cd '$PULSE' && npx jest src/dead-pane-4889.test.ts 2>&1"
  [ "$status" -eq 0 ] || { echo "$output" | tail -30; return 1; }
}

@test "pulse has no pane lookup of its own: the #4362 tmux fallback is gone" {
  out="$(own_pane_lookup "$PULSE/src")"
  [ -z "$out" ] || { echo "still finds a pane itself: $out"; return 1; }
}

@test "negative proof: a planted tmux lookup is found" {
  d="$BATS_TEST_TMPDIR/src"; mkdir -p "$d"
  echo "const p = rolePaneFromTmux(to, run);" > "$d/service.ts"
  echo "rolePaneFromTmux in a test is fine" > "$d/x.test.ts"
  [ "$(own_pane_lookup "$d")" = "$d/service.ts" ] || return 1
}
