#!/usr/bin/env bats
# @test-type: integration — real tmux on a private socket, no live role touched
# @domain: identity
#
# #4813. Login starts the runtime as the pane's own process (never send-keys into a
# live shell), resets a pane stuck at `dquote>`, and a launch that dies says why
# from the pane's last lines. The Rust suite carries the negative proofs.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  CRATE="$ROOT/platform/services/chorus-principal"
}

@test "#4813 AC1-AC3: login-pane launch suite (6 cases, 2 negative proofs)" {
  command -v tmux >/dev/null || { echo "tmux missing"; return 1; }
  run bash -c "cd '$CRATE' && CARGO_TARGET_DIR='$CRATE/target/$(id -un)' cargo test --release --quiet --test login_pane_launch_4813 2>&1"
  echo "$output" | tail -5
  [ "$status" -eq 0 ] || return 1
  [[ "$output" == *"6 passed"* ]] || return 1
}
