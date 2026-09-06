#!/bin/bash
# @test-type: fitness
# retired-name-guard: exempt — this check must contain the retired names to search for them.
# #3561 — the retired-name guard. FAILS on any live reference to a pre-rename
# athena name. Written BEFORE the rename so it is proven red first (#3734).
#
# Jeff, 2026-08-21, after a deploy error handed him 'chorus-model-deploy':
# "i thought we were doing athena-model, athena-make (aka owl-api) and
# athena-deploy". A name nobody can rely on costs a lookup every time.
set -u
SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
# #4111 — the tree this script SHIPS in, not the tree an env var points at.
# CHORUS_ROOT is canonical inside a role session, so run from a werk this guard
# scanned canonical: a violation on main failed your branch, and fixing it in
# your branch could never turn it green. Same class as #3701's ratchet pin.
# Override deliberately with SCAN_ROOT if you really mean another tree.
ROOT="${SCAN_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"
# Both spellings: `owl-api` is the binary/service name, `owl_api` is the CRATE
# path in Rust `use` lines. The hyphen-only pattern reported PASS on 2026-08-21
# while twelve test files still said `owl_api::` and would not compile — one
# spelling of a name is not the name.
RETIRED="owl-api|owl_api|chorus-model-deploy"
# A guard whose search target has moved must fail LOUDLY, never pass vacuously
# (#3734). Without this, a wrong ROOT greps nothing and reports PASS.
for d in platform directing; do
  [ -d "$ROOT/$d" ] || { echo "retired-name guard: FAIL — search root $ROOT/$d does not exist; guard cannot see the tree it grades" >&2; exit 1; }
done
# Historical surfaces keep their old names on purpose: ADRs, decisions, journals
# and the retirement ledger are the RECORD of the rename, not drift. This file
# is excluded by its own path — it must contain the retired names to search for
# them, and excluding by path (not by name) keeps the exclusion honest if it moves.
# #4111 — a COMMENT is not a live reference.
#
# The block above already says historical surfaces keep the old names on
# purpose: an ADR or a journal is the RECORD of a rename, not drift. A comment
# inside a .sh is the same record one level down — "renamed owl-api ->
# athena-make (#3561)" is the rename being documented, and flagging it asks
# people to delete the only note explaining why the name changed.
#
# All six files this guard failed on today were exactly that: prose. It could
# not tell a caller from a footnote, which are the two states it exists to
# separate. So a hit now counts only on a line that is not a comment.
#
# The stripper is deliberately shallow — leading //, #, *, --, /* — because it
# only has to be right about the first token of a line. A retired name inside a
# string on a code line still counts, which is the conservative direction.
is_live_reference() {
  grep -nE "$RETIRED" "$1" 2>/dev/null \
    | sed 's/^[0-9]*://' \
    | grep -qvE '^[[:space:]]*(//|#|\*|--|/\*)'
}

hits=$(grep -rIl -E "$RETIRED" \
  --include="*.rs" --include="*.ts" --include="*.sh" --include="*.toml" --include="*.yml" \
  "$ROOT/platform" "$ROOT/directing" 2>/dev/null \
  | grep -v node_modules | grep -v "/target/" | grep -vF "$SELF" \
  | while read -r f; do
      # A guard has to contain the names it searches for. The exemption is a
      # declared marker, allowed ONLY under platform/tests, so it cannot be used
      # to quiet a real caller.
      exempt=no
      if [ "${f#"$ROOT/platform/tests/"}" != "$f" ] && grep -q "retired-name-guard: exempt" "$f"; then
        exempt=yes
      fi
      [ "$exempt" = yes ] && continue
      is_live_reference "$f" && echo "$f"
    done | sort)
n=$(printf "%s" "$hits" | grep -c . || true)
if [ "$n" -gt 0 ]; then
  echo "retired-name guard: FAIL — $n file(s) still reference a retired athena name"
  printf "%s\n" "$hits" | head -20 | sed 's/^/  /'
  exit 1
fi

# CONTENT and FILENAMES are two states. Grepping content reported PASS on
# 2026-08-21 while `owl-api-launch.sh` and `owl-api-regen.sh` sat in the
# renamed crate — a check that cannot see the name on the file cannot separate
# "renamed" from "renamed inside only". Scoped to the verb surfaces: page routes
# under api/views and api/public keep their names until a route rename is done
# deliberately, which is separate work, not drift.
fnames=$(find "$ROOT/platform/scripts" "$ROOT/platform/tests" "$ROOT/platform/services" \
  -type f \( -name "*owl-api*" -o -name "*chorus-model*" -o -name "*chorus_model*" \) \
  -not -path "*/target/*" -not -path "*/node_modules/*" 2>/dev/null \
  | grep -vF "$SELF" | grep -v "chorus_model_retired.rs" | sort)
fn=$(printf "%s" "$fnames" | grep -c . || true)
if [ "$fn" -gt 0 ]; then
  echo "retired-name guard: FAIL — $fn file(s) still CARRY a retired athena name"
  printf "%s\n" "$fnames" | head -20 | sed 's/^/  /'
  exit 1
fi

# #4111 negative proofs. A guard that just went green has to be shown capable of
# going red, and shown to go red for the RIGHT reason — the comment exemption is
# exactly the kind of widening that quietly disarms a check.
st=$(mktemp -d); trap 'rm -rf "$st"' EXIT
printf '%s\n' '// the generator was renamed owl-api -> athena-make (#3561)' > "$st/prose.ts"
printf '%s\n' 'const url = "http://localhost:3360/owl-api/domains";' > "$st/live.ts"
printf '%s\n' '# owl-api used to serve this' 'run_athena_make --serve' > "$st/mixed.sh"
printf '%s\n' '# owl-api used to serve this' 'curl "$OWL_API/x"  # owl_api legacy' > "$st/sneaky.sh"

selftest_fail=0
check_selftest() {
  local label="$1" file="$2" want="$3"
  if is_live_reference "$file"; then got=live; else got=prose; fi
  if [ "$got" = "$want" ]; then
    echo "  self-test PASS: $label"
  else
    echo "  self-test FAIL: $label — expected $want, got $got"
    selftest_fail=1
  fi
}
echo "retired-name guard: self-test"
check_selftest "a comment recording the rename is NOT a live reference" "$st/prose.ts" prose
check_selftest "NEGATIVE PROOF: a real call site still fails the guard" "$st/live.ts" live
check_selftest "a file whose only hit is a comment passes even with real code beside it" "$st/mixed.sh" prose
check_selftest "NEGATIVE PROOF: a trailing comment does not launder the code line" "$st/sneaky.sh" live
[ "$selftest_fail" -eq 0 ] || { echo "retired-name guard: FAIL — the guard cannot separate prose from a call site"; exit 1; }

echo "retired-name guard: PASS — no live references to, and no files named for, $RETIRED"
