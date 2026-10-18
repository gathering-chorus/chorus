#!/usr/bin/env bats
# @test-type: unit — stand-in git and dagu record their calls; no live scheduler, no second OS user
# @domain: pipelines
# @card: 4602 · owner: kade
# What Jeff sees (10-18 08:18): `werk-v2 4591 abby-normal` from his shell
# exited silently and nothing was enqueued. Abby's werk is owned by another OS
# user, so git called it "dubious ownership" and exited 129 inside the slot
# refresh; set -e ended pipeline-start before the enqueue with no message.
# Now pipeline-start trusts the card's own werk (and no other) for git, and a
# git that still refuses is named, never silent.
#
# The stand-in git behaves like git on a werk another user owns: it refuses
# (exit 129) unless the call trusts exactly that werk through GIT_CONFIG_*.

setup() {
  ROOT="$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)"
  export CHORUS_HOME="$BATS_TEST_TMPDIR/home" CHORUS_WERK_BASE="$BATS_TEST_TMPDIR/werks"
  W="$CHORUS_WERK_BASE/abby-normal-4591"
  mkdir -p "$CHORUS_HOME" "$W/platform/pipelines" "$CHORUS_WERK_BASE/abby-normal-bin" "$BATS_TEST_TMPDIR/bin"
  touch "$W/platform/pipelines/cicd.yaml"
  export DAGU="$BATS_TEST_TMPDIR/dagu"
  printf '#!/bin/sh\necho "$@" > "%s/args"\n' "$BATS_TEST_TMPDIR" > "$DAGU"; chmod +x "$DAGU"
  cat > "$BATS_TEST_TMPDIR/bin/git" <<EOF
#!/bin/sh
if [ "\${GIT_CONFIG_KEY_0:-}" = safe.directory ] && [ "\${GIT_CONFIG_VALUE_0:-}" = "$W" ]; then
  echo platform/clearing/v2/app.js; exit 0
fi
echo "fatal: detected dubious ownership in repository at '$W'" >&2
echo "warning: Not a git repository. Use --no-index to compare two paths outside a working tree" >&2
exit 129
EOF
  chmod +x "$BATS_TEST_TMPDIR/bin/git"
  export PATH="$BATS_TEST_TMPDIR/bin:$PATH"
}

@test "a werk another user owns is enqueued from Jeff's shell" {
  run "$ROOT/platform/scripts/werk-v2" 4591 abby-normal
  [ "$status" -eq 0 ] || { echo "$output"; return 1; }
  [[ "$(cat "$BATS_TEST_TMPDIR/args")" == *" enqueue $W/platform/pipelines/cicd.yaml -- CARD=4591 ROLE=abby-normal" ]] || return 1
}

@test "negative proof: a git that still refuses is named, and nothing is enqueued" {
  printf '#!/bin/sh\necho "fatal: detected dubious ownership" >&2\nexit 129\n' > "$BATS_TEST_TMPDIR/bin/git"
  run "$ROOT/platform/scripts/werk-v2" 4591 abby-normal
  [ "$status" -ne 0 ]
  [[ "$output" == *"git could not read #4591's diff in $W: fatal: detected dubious ownership; nothing was enqueued"* ]] || { echo "$output"; return 1; }
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}

@test "negative proof: without the trust, the other user's werk is never enqueued" {
  old="$BATS_TEST_TMPDIR/pipeline-start-old"
  # the same starter with the trust taken out
  sed -e 's/GIT_CONFIG_COUNT=1 GIT_CONFIG_KEY_0=safe.directory GIT_CONFIG_VALUE_0="\$werk" \\//' \
      -e 's/^  if ! diff_names="\$(\(.*\)$/  if ! diff_names="$(\1/' "$ROOT/platform/scripts/pipeline-start" > "$old"
  grep -q 'GIT_CONFIG_VALUE_0' "$old" && { echo "the trust was not removed"; return 1; }
  chmod +x "$old"
  run "$old" cicd 4591 abby-normal
  [ "$status" -ne 0 ]
  [ ! -f "$BATS_TEST_TMPDIR/args" ]
}
