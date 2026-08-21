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
  # origin/main fixture: the werk repo's own main-less history needs a merge-base,
  # so fabricate origin/main as the first commit and add the card commit on top.
  git -C "$CHORUS_WERK_BASE/kade-42" update-ref refs/remotes/origin/main HEAD
  ( cd "$CHORUS_WERK_BASE/kade-42" && echo card-work > w && git add . \
    && git -c user.email=t@t -c user.name=t commit -q -m "kade: #42 work" )
  # the witness records the CONTENT patch-id (git patch-id of merge-base..HEAD) —
  # the REAL semantic (run 46's refusal was sha-vs-patch-id confusion; fixture
  # now mirrors the captured line {"round":"<sha12>","patch_id":"<patch-id>"}).
  SHA=$(git -C "$CHORUS_WERK_BASE/kade-42" diff origin/main..HEAD | git patch-id --stable | awk '{print $1}')
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

@test "present-only with NO witness takes the full path and never refuses" {
  GO=false run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'skip_prove=false' "$GITHUB_OUTPUT"
}

@test "content-identical re-present skips the prove (#3955 AC3 — ceremony not re-paid)" {
  witness demo.presented 42 "$SHA"
  GO=false run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'skip_prove=true' "$GITHUB_OUTPUT"
}

@test "NEGATIVE PROOF: dirty werk re-present takes the FULL path (commit will change content)" {
  witness demo.presented 42 "$SHA"
  echo dirty >> "$CHORUS_WERK_BASE/kade-42/f"
  GO=false run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'skip_prove=false' "$GITHUB_OUTPUT"
}

@test "NEGATIVE PROOF: changed-content re-present takes the FULL path" {
  witness demo.presented 42 "1111111111111111111111111111111111111111"
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

@test "already-landed werk (tree == origin/main) sets already_landed and skips nothing else" {
  # #3955 AC1 — run 53's wedge: a retry AFTER its own squash-merge rebase-conflicts
  # forever. When the werk tree is content-identical to origin/main the land must
  # jump commit/push/merge (already_landed=true) and go straight to deploy+accept.
  witness demo.presented 42 "$SHA"
  # simulate the squash landing: origin/main advances to the werk's exact tree
  git -C "$CHORUS_WERK_BASE/kade-42" update-ref refs/remotes/origin/main HEAD
  GO=true run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'already_landed=true' "$GITHUB_OUTPUT"
  grep -q 'skip_prove=true' "$GITHUB_OUTPUT"
}

@test "NEGATIVE PROOF: divergent tree does not claim already_landed" {
  witness demo.presented 42 "$SHA"
  GO=true run bash -e "$T/step.sh"
  [ "$status" -eq 0 ]
  grep -q 'already_landed=false' "$GITHUB_OUTPUT"
}
