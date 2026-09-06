#!/usr/bin/env bats
# @test-type: unit — reads two local ledgers, no live service
#
# 4111 — a round may not be PRESENTED as proven on content the prove never ran.
#
# Found live on 2026-09-06. The test leg of run 25 ran for 1h46; I committed five
# times while it ran. At the end:
#
#   test leg ran commit 681597758   patch-id 1880f481e9cb   (= the run pin)
#   demo.presented recorded         patch-id 84154f966a     (= HEAD, +5 commits)
#
# The present stamps the tree as it is at present time, not the tree the prove
# ran on. Then the next run compared HEAD's patch-id to that witness, matched,
# and set skip_prove=true — announcing five untested commits as proven. A go on
# that round lands code nothing ever tested.
#
# The two ledgers already disagree in writing; nothing read them together. This
# does, for every presented round, and fails on the pair that disagrees.

setup() {
  RUNS="${CHORUS_RUNS_DIR:-$HOME/.chorus/werk-runs}"
  WITNESS="${CHORUS_WITNESS:-${CHORUS_HOME:-/Users/jeffbridwell/CascadeProjects/chorus}/ops/logs/werk-demo.jsonl}"
}

# The comparison itself, as a function so the proofs below drive the real logic
# rather than a copy of it. Emits one line per mismatch.
mismatches() {
  local runs="$1" witness="$2"
  python3 - "$runs" "$witness" <<'PY'
import json, os, sys, glob
runs_dir, witness = sys.argv[1], sys.argv[2]

# last presented patch_id per card
presented = {}
try:
    for line in open(witness):
        try:
            d = json.loads(line)
        except ValueError:
            continue
        if d.get("event") == "demo.presented":
            presented[str(d.get("card_id"))] = d.get("patch_id", "")
except OSError:
    pass

for pin_path in sorted(glob.glob(os.path.join(runs_dir, "*.json"))):
    try:
        pin = json.load(open(pin_path))
    except (OSError, ValueError):
        continue
    if pin.get("phase") != "presented":
        continue
    card = str(pin.get("card"))
    proved = pin.get("patchId", "")
    shown = presented.get(card, "")
    if proved and shown and proved != shown:
        print(f"{card} proved={proved} presented={shown}")
PY
}

@test "no presented round shows content the prove did not cover" {
  run mismatches "$RUNS" "$WITNESS"
  [ -z "$output" ] || {
    echo "A round was presented on content its prove never ran:"
    echo "$output"
    echo "The run pin records the patch-id the test leg ran; the witness records"
    echo "what was announced. They must be the same content."
    false
  }
}

@test "NEGATIVE PROOF: a pin and witness that disagree ARE caught" {
  # The exact 2026-09-06 pair, in a fixture. If this stops failing, the check
  # can no longer see the thing it was written for.
  r="$BATS_TEST_TMPDIR/runs"; mkdir -p "$r"
  cat > "$r/9999.json" <<'J'
{"card":9999,"phase":"presented","patchId":"1880f481e9cb4069f0faf0578e1f215503031a2d"}
J
  w="$BATS_TEST_TMPDIR/witness.jsonl"
  echo '{"event":"demo.presented","card_id":9999,"patch_id":"84154f966af71a707e91ed130044780ee74d3191"}' > "$w"
  run mismatches "$r" "$w"
  [ -n "$output" ]
  [[ "$output" == *"9999"* ]]
}

@test "NEGATIVE PROOF: a matching pair is NOT reported, or the check is noise" {
  r="$BATS_TEST_TMPDIR/runs2"; mkdir -p "$r"
  cat > "$r/9998.json" <<'J'
{"card":9998,"phase":"presented","patchId":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}
J
  w="$BATS_TEST_TMPDIR/witness2.jsonl"
  echo '{"event":"demo.presented","card_id":9998,"patch_id":"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"}' > "$w"
  run mismatches "$r" "$w"
  [ -z "$output" ]
}

@test "a round still RUNNING is not judged — only presented rounds are claims" {
  r="$BATS_TEST_TMPDIR/runs3"; mkdir -p "$r"
  cat > "$r/9997.json" <<'J'
{"card":9997,"phase":"running","patchId":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"}
J
  w="$BATS_TEST_TMPDIR/witness3.jsonl"
  echo '{"event":"demo.presented","card_id":9997,"patch_id":"cccccccccccccccccccccccccccccccccccccccc"}' > "$w"
  run mismatches "$r" "$w"
  [ -z "$output" ]
}

@test "a missing witness file does not fabricate a pass or a failure" {
  r="$BATS_TEST_TMPDIR/runs4"; mkdir -p "$r"
  cat > "$r/9996.json" <<'J'
{"card":9996,"phase":"presented","patchId":"dddddddddddddddddddddddddddddddddddddddd"}
J
  run mismatches "$r" "$BATS_TEST_TMPDIR/nope.jsonl"
  [ -z "$output" ]
}
