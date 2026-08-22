#!/bin/bash
# @test-type: fitness
# retired-name-guard: exempt — this check must contain the retired names to search for them.
# #3561 AC14 — an error or usage message raised by an athena verb must name
# ONLY athena-* surfaces.
#
# This is the #3885 case, and it is the reason the whole rename card exists.
# Jeff ran a deploy, it failed, and the failure handed him 'chorus-model-deploy'
# — a name the team had already retired. The binary had been renamed; the string
# inside it had not. A rename that stops at the filename is not a rename.
#
# The content guard (retired-athena-names-3561.test.sh) greps SOURCE. This one
# RUNS the verbs and grades what a human actually reads. Those are two different
# states and one check cannot separate them: source can be clean while a built
# binary on PATH still prints the old text, and a stale binary is exactly what
# reached Jeff.
set -u
ROOT="${CHORUS_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
RETIRED="chorus-model|owl-api|chorus-model-deploy"
fails=0
checked=0

# Each verb invoked so it RAISES — no args, which every athena verb answers with
# its usage block. A verb that exits 0 silently is reported, never skipped: a
# check that quietly passes over a missing verb cannot tell "clean" from "absent".
grade() {  # <label> <binary> <fatal:yes|no>
  local label="$1" bin="$2" fatal="$3" out
  checked=$((checked + 1))
  out="$("$bin" 2>&1 | head -40 || true)"
  printf '%s' "$out" | grep -qE "$RETIRED" || { echo "  ok   $label ($bin)"; return; }
  if [ "$fatal" = yes ]; then
    echo "athena-verb-names: FAIL — $label prints a retired name to the user:" >&2
    fails=$((fails + 1))
  else
    echo "athena-verb-names: STALE DEPLOY — $label prints a retired name; the" >&2
    echo "  built artifact is clean, so this clears when the binary is deployed:" >&2
    stale=$((stale + 1))
  fi
  printf '%s\n' "$out" | grep -nE "$RETIRED" | head -5 | sed 's/^/    /' >&2
}

# BUILT and DEPLOYED are two states, and one lookup cannot separate them. #3885
# reached Jeff because the source was renamed and the binary on his PATH was not.
# So grade both: the built artifact is FATAL (it is what this card ships), the
# deployed copy is reported as STALE DEPLOY (it clears at the live swap).
stale=0
for verb in athena-model athena-make athena-deploy; do
  # ATHENA_BIN_DIR exists so this check can be pointed at a deliberately-bad
  # binary and shown to FAIL (#3734). Unset in every real run.
  if [ -n "${ATHENA_BIN_DIR:-}" ] && [ -x "$ATHENA_BIN_DIR/$verb" ]; then
    grade "$verb (probe)" "$ATHENA_BIN_DIR/$verb" yes
    continue
  fi
  built="$ROOT/platform/services/$verb/target/release/$verb"
  deployed="$HOME/.chorus/bin/$verb"
  found=no
  [ -x "$built" ] && { grade "$verb (built)" "$built" yes; found=yes; }
  [ -x "$deployed" ] && { grade "$verb (deployed)" "$deployed" no; found=yes; }
  if [ "$found" = no ]; then
    path_bin="$(command -v "$verb" 2>/dev/null || true)"
    if [ -n "$path_bin" ]; then
      grade "$verb (PATH)" "$path_bin" yes
    else
      echo "athena-verb-names: FAIL — $verb not found; cannot grade what it prints" >&2
      fails=$((fails + 1))
    fi
  fi
done

# A guard that graded zero verbs must not report PASS (#3734).
if [ "$checked" -eq 0 ]; then
  echo "athena-verb-names: FAIL — graded 0 verbs; the check saw nothing" >&2
  exit 1
fi

[ "$fails" -eq 0 ] || exit 1
echo "athena-verb-names: PASS — $checked binary/binaries graded, built artifacts name only athena-*${stale:+ ($stale stale deploy(s) pending the live swap)}"
