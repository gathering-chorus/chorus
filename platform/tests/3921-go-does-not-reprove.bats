#!/usr/bin/env bats
# @test-type: unit — hermetic: extracts the real proven-round step from werk.yml,
# runs it against a fixture witness + a fixture git repo. No services.
#
# #3921 introduced the skip; #3943 moved its input to the werk-demo WITNESS.
# The launcher rewrites the run pin at go-launch (phase=running, fresh patchId),
# so a pin-based compare NEVER fired — observed live on #3925, #3945, #3929
# lands (three full re-proves of presented rounds in one evening). The witness
# (ops/logs/werk-demo.jsonl demo.presented lines) is written by werk-demo at
# present time and nothing rewrites it. Jeff, 2026-08-20: "my go triggers
# another loop on every fucking card."
#
# The guarded states (#3943 AC3): identical→skip Half-A entirely;
# drifted-since-present→REFUSE the go (supersede guard), never silently re-prove.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"   # this werk, never CHORUS_ROOT
  WF="$REPO/.github/workflows/werk.yml"
  T="$(mktemp -d)"; export HOME="$T/home"; mkdir -p "$HOME/.chorus/werk-runs"
  export CHORUS_HOME="$T/chorus_home"; mkdir -p "$CHORUS_HOME/ops/logs"
  WITNESS="$CHORUS_HOME/ops/logs/werk-demo.jsonl"
  # fixture werk repo
  export CHORUS_WERK_BASE="$T/werks"; mkdir -p "$CHORUS_WERK_BASE/kade-42"
  git -C "$CHORUS_WERK_BASE/kade-42" init -q -b kade/42 .
  ( cd "$CHORUS_WERK_BASE/kade-42" && echo x > f && git add . \
    && git -c user.email=t@t -c user.name=t commit -q -m "kade: #42" )
  SHA=$(git -C "$CHORUS_WERK_BASE/kade-42" rev-parse HEAD)
  export ROLE=kade CARD_ID=42 GITHUB_OUTPUT="$T/out"
  # extract the real step body — a copy would drift (#3701 lesson)
  awk '$0 ~ "^      - name: " {inst=($0=="      - name: proven-round"); inr=0; next}
       inst && $0 ~ "^        run: \\|" {inr=1; next}
       inr {if ($0 ~ /^        [a-z_-]+:/) {inr=0; next} sub(/^          /,""); print}' \
    "$WF" > "$T/step.sh"
  [ -s "$T/step.sh" ] || { echo "extract failed — step gone, gate must go red" >&2; return 1; }
}

teardown() { rm -rf "$T"; }

witness() { # witness <event> <card_id> <patch_id>
  printf '{"ts":1,"event":"%s","role":"kade","card_id":%s,"patch_id":"%s"}\n' \
    "$1" "$2" "$3" >> "$WITNESS"
}
pin() { printf '{"phase":"%s","patchId":"sha:%s"}' "$1" "$2" > "$HOME/.chorus/werk-runs/42.json"; }

@test "go on a witnessed sha-identical round skips the re-prove" {
  witness demo.presented 42 "$SHA"
  GO=true run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'skip_prove=true' "$GITHUB_OUTPUT"
}

@test "REGRESSION (#3943): launcher pin-clobber cannot defeat the skip" {
  # the exact violation observed on #3925/#3945/#3929: pin says running with a
  # fresh patchId while the witness holds the presented round. Witness wins.
  witness demo.presented 42 "$SHA"
  pin running "0000000000000000000000000000000000000000"
  GO=true run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'skip_prove=true' "$GITHUB_OUTPUT"
}

@test "last presented line wins over an older stale one" {
  witness demo.presented 42 "1111111111111111111111111111111111111111"
  witness demo.presented 42 "$SHA"
  GO=true run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'skip_prove=true' "$GITHUB_OUTPUT"
}

@test "NEGATIVE PROOF: one commit of drift REFUSES the go, never re-proves" {
  witness demo.presented 42 "$SHA"
  ( cd "$CHORUS_WERK_BASE/kade-42" && echo drift >> f \
    && git -c user.email=t@t -c user.name=t commit -qam "kade: #42 drift" )
  GO=true run bash -e "$T/step.sh"
  [ "$status" -ne 0 ]
  echo "$output" | grep -q 'go-refused'
}

@test "NEGATIVE PROOF: go with no witness for the card REFUSES, never re-proves" {
  witness demo.presented 7 "$SHA"   # another card's present is not ours
  GO=true run bash -e "$T/step.sh"
  [ "$status" -ne 0 ]
  echo "$output" | grep -q 'go-refused'
}

@test "malformed witness lines are skipped, not fatal" {
  echo 'not json at all' >> "$WITNESS"
  witness demo.presented 42 "$SHA"
  GO=true run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'skip_prove=true' "$GITHUB_OUTPUT"
}

@test "present-only runs (no go) never take the fast path and never refuse" {
  GO=false run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'skip_prove=false' "$GITHUB_OUTPUT"
}

@test "NEGATIVE PROOF: uncommitted drift at go time REFUSES — commit runs after the compare" {
  witness demo.presented 42 "$SHA"
  echo uncommitted >> "$CHORUS_WERK_BASE/kade-42/f"
  GO=true run bash -e "$T/step.sh"
  [ "$status" -ne 0 ]
  echo "$output" | grep -q 'go-refused: werk has uncommitted changes'
}
