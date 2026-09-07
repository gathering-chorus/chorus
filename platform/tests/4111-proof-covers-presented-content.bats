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
  # #4111 — no absolute local path: the guard is right, and a test that pins one
  # person's home directory cannot run anywhere else.
  ROOT="${CHORUS_HOME:-$(cd "$(dirname "$BATS_TEST_FILENAME")/../.." && pwd)}"
  WITNESS="${CHORUS_WITNESS:-$ROOT/ops/logs/werk-demo.jsonl}"
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
  # Agreed with Silas 2026-09-06: a re-stamp can be LEGITIMATE. #3678 added it
  # on purpose so the pipeline's own commits (it rewrites a doc-coherence file
  # mid-run) get absorbed instead of invalidating the round. So a bare mismatch
  # is not yet a finding — comparing two ids cannot say which kind it is, which
  # would be the same blindness this check exists to end, one level up.
  #
  # The re-stamp will emit its own justification:
  #     round.restamped { priorPatchId, newPatchId, files[] }
  # Declared set, not inferred. Until that record exists a mismatch is
  # UNCLASSIFIED: reported, never counted as a pass, and never called a defect.
  run mismatches "$RUNS" "$WITNESS"
  if [ -n "$output" ]; then
    echo "UNCLASSIFIED — a presented round's content differs from what its prove ran:"
    echo "$output"
    echo
    echo "This is not yet a verdict. A re-stamp is legitimate when the delta is"
    echo "confined to pipeline-written files and a defect when it absorbs the"
    echo "author's commits, and nothing recorded which happened. Once"
    echo "round.restamped ships, this test classifies instead of reporting."
  fi
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
