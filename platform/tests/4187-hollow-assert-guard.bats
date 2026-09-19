#!/usr/bin/env bats
# @test-type: unit
# #4187 — THE HOLLOW-ASSERT GUARD. Two shapes, one guard, because we found the
# first today and the second is already in 91 of 223 suites and would otherwise
# be rediscovered in a month (Wren, 2026-09-18).
#
# SHAPE 1 — `grep -qv X` against `$output`.
#   `run` captures MULTI-LINE output. `grep -qv X` succeeds when ANY line lacks
#   X, so it is true even when X is present on another line. It reads as "assert
#   absent" and asserts nothing. Found today in three asserts I wrote into
#   4125-source-delete-refused.bats — a card that had already landed.
#   The obvious fix — `! echo "$output" | grep -q X` — is shape 3 and no better.
#   The form that holds is a simple command:
#     test -z "$(printf '%s' "$output" | grep -F "X" || true)"
#
# SHAPE 3 — `! cmd` that is not the last line of its @test block.
#   Under `set -e` bash IGNORES a failure whose status was inverted by `!`, so the
#   obvious fix for shape 1 is hollow in the same way. Found 2026-09-18 by mutating
#   the corrected asserts: the same wrong expectation went RED where the line
#   happened to be last and stayed GREEN where it did not.
#
# SHAPE 2 — `[[ ... ]]` that is not the last line of its @test block.
#   bash 3.2 (what macOS ships, what bats runs here) does not fail a test on a
#   failing `[[` unless it is the final command. A wrong expectation mid-block is
#   silently skipped. The correct form is `test`, or any simple command.
#
# This guard is itself a check that gates, so it ships with a fixture where both
# violations are present and the guard is watched failing on each (#3734).

TESTS_DIR="$(cd "$BATS_TEST_DIRNAME" && pwd)"

# One scanner, used by the real sweep AND by the negative proof, so the proof
# exercises the same code the sweep does rather than a re-implementation.
# Heredoc bodies are skipped in BOTH scanners. Without that the guard flags its
# own fixtures — which are deliberately wrong — and a guard that cannot tell a
# fixture from a finding is the same defect it exists to catch. Proven by test 4:
# the correct forms, written inside a heredoc, are not flagged.
# ONE scanner, in python because the awk quoting for this was its own bug source.
# Used by the real sweep AND by both negative proofs, so the proofs exercise the
# code the sweep runs rather than a re-implementation that can drift from it.
#
# Heredoc bodies are skipped. Without that the guard flags its own fixtures —
# which are deliberately wrong — and a guard that cannot tell a fixture from a
# finding is the very defect it exists to catch.
_scan() {   # $1 = shape (qv|bracket), $2 = file. prints "file:line: text" per hit
  python3 - "$1" "$2" <<'PYEOF'
import sys, re, io
shape, path = sys.argv[1], sys.argv[2]
lines = io.open(path, encoding='utf-8', errors='replace').read().splitlines()
# blank out heredoc bodies, keeping line numbers intact
live, tag = [], None
for ln in lines:
    if tag is None:
        m = re.search(r"<<-?'?([A-Za-z_][A-Za-z_0-9]*)'?", ln)
        if m:
            tag = m.group(1); live.append(''); continue
        live.append(ln)
    else:
        if ln.strip() == tag: tag = None
        live.append('')
# Single-quoted literals are not code. Without this the guard flags the very
# lines that BUILD its fixtures (printf '  [[ "1" = "2" ]]') — a guard that
# cannot tell a string from a statement is the defect it exists to catch.
live = [re.sub(r"'[^']*'", "''", ln) for ln in live]
hits = []
if shape == 'qv':
    for i, ln in enumerate(live, 1):
        if ln.lstrip().startswith('#'): continue
        if re.search(r'grep\s+-[a-zA-Z]*v[a-zA-Z]*\s', ln) and '-q' in ln:
            hits.append((i, ln.strip()))
elif shape == 'bang':
    block, in_test = [], False
    for i, ln in enumerate(live, 1):
        st = ln.strip()
        if st.startswith('@test'): in_test, block = True, []; continue
        if in_test and ln.startswith('}'):
            for j, (n, t) in enumerate(block):
                if j < len(block) - 1 and re.match(r'!\s', t): hits.append((n, t))
            in_test = False; continue
        if in_test and st and not st.startswith('#'): block.append((i, st))
else:
    block, in_test = [], False
    for i, ln in enumerate(live, 1):
        st = ln.strip()
        if st.startswith('@test'): in_test, block = True, []; continue
        if in_test and ln.startswith('}'):
            for j, (n, t) in enumerate(block):
                if j < len(block) - 1 and '[[' in t: hits.append((n, t))
            in_test = False; continue
        if in_test and st and not st.startswith('#'): block.append((i, st))
for n, t in hits: print(f"{path}:{n}: {t}")
PYEOF
}
_hollow_grep_qv()        { _scan qv "$1"; }
_hollow_double_bracket() { _scan bracket "$1"; }

# THE RATCHET. 164 violations exist today (4 inverted-grep, 160 double-bracket).
# A guard that fires 164 times on day one gets switched off, so this one holds a
# baseline and goes red only on a NEW one (Wren, 2026-09-18). The 4 are fixed on
# this card; the 160 are their own card.
#
# The baseline is a committed FILE, not a constant in here, so lowering it is a
# diff someone reviews rather than an edit inside a test nobody reads.
BASELINE_FILE="$TESTS_DIR/.hollow-assert-baseline"

_count() {   # $1 = shape
  local n=0 f
  for f in "$TESTS_DIR"/*.bats; do
    n=$(( n + $(_scan "$1" "$f" | grep -c . || true) ))
  done
  echo "$n"
}

_baseline() {   # $1 = shape; a missing file or key is 0, never a free pass
  grep -E "^$1=" "$BASELINE_FILE" 2>/dev/null | head -1 | cut -d= -f2 | tr -dc '0-9'
}

@test "#4187 no NEW inverted-grep assert (ratchet)" {
  now=$(_count qv); base=$(_baseline qv); base=${base:-0}
  echo "inverted-grep: now $now, baseline $base"
  if [ "$now" -gt "$base" ]; then
    echo "a new one landed; an inverted quiet grep asserts nothing on multi-line output"
    for f in "$TESTS_DIR"/*.bats; do _scan qv "$f"; done
    return 1
  fi
}

@test "#4187 no NEW mid-block double-bracket (ratchet)" {
  now=$(_count bracket); base=$(_baseline bracket); base=${base:-0}
  echo "mid-block double-bracket: now $now, baseline $base"
  if [ "$now" -gt "$base" ]; then
    echo "a new one landed; bash 3.2 swallows a failing test unless it is the block last command"
    for f in "$TESTS_DIR"/*.bats; do _scan bracket "$f"; done
    return 1
  fi
}

@test "#4187 no NEW mid-block inverted command (ratchet)" {
  now=$(_count bang); base=$(_baseline bang); base=${base:-0}
  echo "mid-block inverted command: now $now, baseline $base"
  if [ "$now" -gt "$base" ]; then
    echo "a new one landed; set -e ignores a failure inverted by ! unless it is the block last command"
    echo "use: test -z \"\$(printf '%s' \"\$output\" | grep -F \"X\" || true)\""
    for f in "$TESTS_DIR"/*.bats; do _scan bang "$f"; done
    return 1
  fi
}

@test "#4187 NEGATIVE PROOF: the ratchet still goes RED when the count rises" {
  fixture="$BATS_TEST_TMPDIR/rise.bats"
  printf '%s\n' '#!/usr/bin/env bats' > "$fixture"
  printf '%s\n' '@test "new violation" {' >> "$fixture"
  printf '%s\n' '  run echo "alpha"' >> "$fixture"
  printf '%s\n' '  echo "$output" | grep -qv "alpha"' >> "$fixture"
  printf '%s\n' '}' >> "$fixture"
  n=$(_scan qv "$fixture" | grep -c . || true)
  test "$n" -gt 0
}

@test "#4187 NEGATIVE PROOF: the guard catches both shapes in a fixture" {
  # printf, not a heredoc: a heredoc inside a bats test inside a function that
  # itself uses a heredoc is exactly how the first draft of this proof silently
  # scanned nothing and passed the qv half while the bracket half saw an empty file.
  fixture="$BATS_TEST_TMPDIR/hollow.bats"
  printf '%s\n' '#!/usr/bin/env bats' > "$fixture"
  printf '%s\n' '@test "shape 1" {' >> "$fixture"
  printf '%s\n' '  run echo "alpha"' >> "$fixture"
  printf '%s\n' '  echo "$output" | grep -qv "alpha"' >> "$fixture"
  printf '%s\n' '}' >> "$fixture"
  printf '%s\n' '@test "shape 2" {' >> "$fixture"
  printf '%s\n' '  [[ "1" = "2" ]]' >> "$fixture"
  printf '%s\n' '  true' >> "$fixture"
  printf '%s\n' '}' >> "$fixture"

  hits1="$(_hollow_grep_qv "$fixture")"
  hits2="$(_hollow_double_bracket "$fixture")"
  echo "qv hits: $hits1"
  echo "bracket hits: $hits2"
  test -n "$hits1"
  test -n "$hits2"
}

@test "#4187 NEGATIVE PROOF: the guard does NOT flag the correct forms" {
  fixture="$BATS_TEST_TMPDIR/sound.bats"
  printf '%s\n' '#!/usr/bin/env bats' > "$fixture"
  printf '%s\n' '@test "sound" {' >> "$fixture"
  printf '%s\n' '  run echo "alpha"' >> "$fixture"
  printf '%s\n' '  test -z "$(printf %s "$output" | grep -F "beta" || true)"' >> "$fixture"
  printf '%s\n' '  test "$status" -eq 0' >> "$fixture"
  printf '%s\n' '}' >> "$fixture"
  printf '%s\n' '@test "a double bracket as the last line is sound" {' >> "$fixture"
  printf '%s\n' '  run echo "alpha"' >> "$fixture"
  printf '%s\n' '  [[ "$output" = "alpha" ]]' >> "$fixture"
  printf '%s\n' '}' >> "$fixture"

  test -z "$(_hollow_grep_qv "$fixture")"
  test -z "$(_hollow_double_bracket "$fixture")"
}
