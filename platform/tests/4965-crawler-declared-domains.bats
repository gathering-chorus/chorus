#!/usr/bin/env bats
# @test-type: integration — runs the installed crawler's read-only validate over this tree and the store
# @domain: tests
# @card: #4965 · owner: kade
# @needs: machine — reads the store's graph, which a land's crawl writes; a card run leaves it to the nightly (#4539)
# AC5, second half (review 20:16: the check never ran the crawler): the crawler's validate
# reports no test file missing its declared domain — the `Test (declared domain)` line,
# measured, missing=0. Read-only: `--validate` writes nothing (#4568).

ROOT="$(cd "$BATS_TEST_DIRNAME/../.." && pwd)"
CRAWL_BIN="${CRAWL_BIN_4965:-$(command -v chorus-crawl)}"   # a role account's HOME holds no bin

# declared_gap <crawler> <tree> — empty when the declared-domain line is measured with missing=0;
# else the line, or why there is none
# #4915 — validate runs under a deadline (VALIDATE_DEADLINE_4965, 300 s): as jeffbridwell it ran
# past werk-test's 1200 s unit timeout every night from 11-20 (26 s as chorus-kade), so the suite
# died UNMEASURED saying nothing. Now a stall is red, naming the last lines and when they came.
validate_lines() { # validate_lines <crawler> <tree>: the run's output, or "STALLED ..." on the deadline
  python3 - "$1" "$2" "${VALIDATE_DEADLINE_4965:-300}" <<'PY'
import os, subprocess, sys, threading, time
crawler, tree, deadline = sys.argv[1], sys.argv[2], float(sys.argv[3])
env = dict(os.environ, CHORUS_ROOT=tree, CHORUS_ROLE="kade")
p = subprocess.Popen([crawler, "--validate"], cwd=tree, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
t0, seen = time.time(), []
def read():
    for line in p.stdout: seen.append((time.time() - t0, line.rstrip()))
r = threading.Thread(target=read, daemon=True); r.start(); r.join(deadline)
if r.is_alive():
    p.terminate()
    tail = " · ".join(f"+{t:.0f}s {l[:120]}" for t, l in seen[-3:]) or "no output at all"
    print(f"STALLED: {crawler} --validate still running after {deadline:.0f} s; last lines: {tail}")
else:
    p.wait(); print("\n".join(l for _, l in seen))
PY
}
declared_gap() {
  local out line
  out="$(validate_lines "$1" "$2")"
  case "$out" in STALLED:*) echo "$out"; return ;; esac
  line="$(printf '%s\n' "$out" | grep -m1 '^VALIDATE|tests|Test (declared domain)|')"
  [ -n "$line" ] || { echo "no 'Test (declared domain)' line from $1 --validate"; return; }
  [[ "$line" == *"|missing=0|"*"|measured" ]] || echo "$line"
}

@test "AC5: crawler validate reports no test file missing its declared domain" {
  [ -n "$CRAWL_BIN" ] && [ -x "$CRAWL_BIN" ] || { echo "no chorus-crawl on PATH"; return 1; }
  out="$(declared_gap "$CRAWL_BIN" "$ROOT")"
  [ -z "$out" ] || { echo "$out"; return 1; }
}

@test "AC5 NEGATIVE PROOF: a missing declared domain, an unmeasured line, or no line at all is named" {
  fake="$BATS_TEST_TMPDIR/crawl"
  for body in 'missing=3|stale=0|measured' 'missing=0|stale=0|UNMEASURED'; do
    printf '#!/bin/sh\necho "VALIDATE|tests|Test (declared domain)|tree=9|graph=6|%s"\n' "$body" > "$fake"; chmod +x "$fake"
    [ -n "$(declared_gap "$fake" "$BATS_TEST_TMPDIR")" ] || { echo "passed: $body"; return 1; }
  done
  printf '#!/bin/sh\necho "VALIDATE|code|CodeFile|tree=1|graph=1|missing=0|stale=0|measured"\n' > "$fake"
  [[ "$(declared_gap "$fake" "$BATS_TEST_TMPDIR")" == *"no 'Test (declared domain)' line"* ]] || return 1
  # a crawler that never finishes is red by name, never a timeout that says nothing
  printf '#!/bin/sh\necho "chorus-crawl: pass start"\nsleep 30\n' > "$fake"
  [[ "$(VALIDATE_DEADLINE_4965=2 declared_gap "$fake" "$BATS_TEST_TMPDIR")" == "STALLED:"*"pass start"* ]] || return 1
  # control: the measured zero passes
  printf '#!/bin/sh\necho "VALIDATE|tests|Test (declared domain)|tree=9|graph=9|missing=0|stale=0|measured"\n' > "$fake"
  [ -z "$(declared_gap "$fake" "$BATS_TEST_TMPDIR")" ]
}
