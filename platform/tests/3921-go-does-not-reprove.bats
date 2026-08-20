#!/usr/bin/env bats
# @test-type: unit — hermetic: extracts the real proven-round step from werk.yml,
# runs it against fixture pins + a fixture git repo. No services.
#
# #3921 — a GO on a presented, sha-identical round must SKIP the re-prove.
# Before this every go re-ran build+test (p95 25min) on the exact commit the
# human had just seen. Jeff, 2026-08-20: "my go triggers another loop on every
# fucking card." The guarded pair of states: identical→skip, drifted→full path.

setup() {
  REPO="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"   # this werk, never CHORUS_ROOT
  WF="$REPO/.github/workflows/werk.yml"
  T="$(mktemp -d)"; export HOME="$T/home"; mkdir -p "$HOME/.chorus/werk-runs"
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

pin() { printf '{"phase":"%s","patchId":"sha:%s"}' "$1" "$2" > "$HOME/.chorus/werk-runs/42.json"; }

@test "go on a presented sha-identical round skips the re-prove" {
  pin presented "$SHA"
  GO=true run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'skip_prove=true' "$GITHUB_OUTPUT"
}

@test "NEGATIVE PROOF: one commit of drift forces the full path" {
  pin presented "$SHA"
  ( cd "$CHORUS_WERK_BASE/kade-42" && echo drift >> f \
    && git -c user.email=t@t -c user.name=t commit -qam "kade: #42 drift" )
  GO=true run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'skip_prove=false' "$GITHUB_OUTPUT"
}

@test "NEGATIVE PROOF: a running (unpresented) pin never skips" {
  pin running "$SHA"
  GO=true run bash -e "$T/step.sh"
  grep -q 'skip_prove=false' "$GITHUB_OUTPUT"
}

@test "present-only runs (no go) never take the fast path" {
  pin presented "$SHA"
  GO=false run bash -e "$T/step.sh"
  grep -q 'skip_prove=false' "$GITHUB_OUTPUT"
}
