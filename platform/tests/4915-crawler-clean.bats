#!/usr/bin/env bats
# @test-type: integration — the crawler's newest pass in prod wrote every row
# @domain: proving
# @card: #4915 · owner: kade
# @needs: machine — reads prod after the land (AC check: after); a card run leaves it to borg-prove and the nightly (#4539)
# After #4889 retired werk-review, four test rows still named its deleted
# Cargo.lock in `exercises`. Every crawl restated those rows, the door refused
# them as unknown-target, and the watermark held all night (11-06). The fix
# (chorus-crawl drop_dead_edges, unit-tested in src/lib.rs dead_edges_4915)
# drops an edge to a row that is gone. This reads the crawler's own log.
# the crawler writes under the repo owner's home (two levels above the checkout),
# which is not $HOME when a role account runs this
# (review 11-06: "../" counts differ between canonical and a werk; the main checkout's
# .git sits at <home>/CascadeProjects/chorus/.git from both)
OWNER_HOME="$(cd "$BATS_TEST_DIRNAME/../.." && cd "$(git rev-parse --git-common-dir)/../../.." && pwd)"
LOG="${CRAWL_LOG_4915:-$OWNER_HOME/Library/Logs/Chorus/crawl-nightly.log}"

# the verdict over a crawl log: the newest pass's wrote/failed line, and any
# unknown-target refusal after it
verdict() {
  python3 - "$1" <<'PY'
import re, sys
t = open(sys.argv[1], errors="replace").read()
m = list(re.finditer(r"chorus-crawl: wrote=(\d+) failed=(\d+)", t))
if not m:
    print("UNMEASURED no crawl pass in the log"); sys.exit(2)
last = m[-1]
tail = t[last.start():]
end = tail.find("\nchorus-crawl: delta ", 1)
seg = tail if end < 0 else tail[:end]
before = t[max(0, t.rfind("chorus-crawl: delta ", 0, last.start())):last.start()]
bad = re.findall(r"unknown-target[^\n]{0,160}", before)
if int(last.group(2)) != 0 or bad:
    print(f"RED failed={last.group(2)} unknown-target={len(bad)} {bad[:1]}"); sys.exit(1)
print(f"GREEN wrote={last.group(1)} failed=0"); sys.exit(0)
PY
}

@test "the crawler's newest pass in prod failed no write and refused no edge" {
  run verdict "$LOG"
  echo "$output"
  [ "$status" -eq 0 ]
}

@test "negative proof: a pass with the 11-06 unknown-target refusals reads red" {
  f="$BATS_TEST_TMPDIR/crawl.log"
  cat > "$f" <<'L'
chorus-crawl: delta 7720f4bce..631bf4e87 · tracked=109 read=Scoped
chorus-crawl: FAILED update file platform/tests/4345-retirement-gate-generic-names.test.sh: PUT /code/files/x -> HTTP 422 { "error": "validation", "message": "athena-model: unknown-target: exercises → <https://jeffbridwell.com/chorus#code-file-file-platform-services-werk-review-cargo-lock-d9d7d84e> exists neither
chorus-crawl: wrote=300 failed=4 · elapsed=135s
chorus-crawl: watermark HELD — a write failed — the graph does not match this commit
L
  run verdict "$f"
  [ "$status" -eq 1 ]
  [[ "$output" == *"failed=4"* ]] || return 1
}

@test "negative proof: a clean pass after a red one reads green, so only the newest pass counts" {
  f="$BATS_TEST_TMPDIR/crawl.log"
  printf '%s\n' 'chorus-crawl: delta a..b · read=Scoped' 'unknown-target: old' 'chorus-crawl: wrote=300 failed=4' \
    'chorus-crawl: delta b..c · read=Scoped' 'chorus-crawl: wrote=12 failed=0' > "$f"
  run verdict "$f"
  [ "$status" -eq 0 ]
  printf '%s\n' 'nothing here' > "$f"
  run verdict "$f"
  [ "$status" -eq 2 ]
}
