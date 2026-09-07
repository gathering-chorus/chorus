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
# #4113 — the tree this file lives in, NOT $CHORUS_ROOT. Reading the env var made a
# werk's copy of this suite grade CANONICAL's files: it reported the same six hits no
# matter what the werk changed, so a fix could never turn it green from where the fix
# was made. Same defect this card found in two other suites.
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
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
# #4113 — match CODE, not the record of the rename.
#
# Every one of the six hits this reported on 2026-09-07 was a COMMENT explaining that
# owl-api had been renamed to athena-make: "# the generator was RENAMED owl-api ->
# athena-make (#3561)". The guard fired on the history of its own fix and could not
# be made green without deleting the explanation of why the name changed — so it
# could not tell a live reference from a note about a dead one. Comment lines are
# excluded; a reference in code still fails, and the self-test at the foot proves it.
#
# `grep -l` cannot do this (it matches per FILE), so the hit list is built per LINE
# and the comment prefixes for the four languages searched are stripped first.
strip_comments() { grep -vE '^[[:space:]]*(//|#|\*|/\*)' "$1" 2>/dev/null || true; }

hits=$(grep -rIl -E "$RETIRED" \
  --include="*.rs" --include="*.ts" --include="*.sh" --include="*.toml" --include="*.yml" \
  "$ROOT/platform" "$ROOT/directing" 2>/dev/null \
  | grep -v node_modules | grep -v "/target/" | grep -vF "$SELF" \
  | while read -r f; do
      # keep the file only if a NON-comment line carries a retired name
      strip_comments "$f" | grep -qE "$RETIRED" && echo "$f"
    done \
  | while read -r f; do
      # A guard has to contain the names it searches for. The exemption is a
      # declared marker, allowed ONLY under platform/tests, so it cannot be used
      # to quiet a real caller.
      exempt=no
      if [ "${f#"$ROOT/platform/tests/"}" != "$f" ] && grep -q "retired-name-guard: exempt" "$f"; then
        exempt=yes
      fi
      [ "$exempt" = yes ] || echo "$f"
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

# NEGATIVE PROOF (#3734) — the comment exclusion must not blind the guard.
# A fixture with the retired name in CODE must still be caught; the same name in a
# comment must not be. Without both halves, "ignore comments" could quietly mean
# "ignore everything".
_neg=$(mktemp -d); trap 'rm -rf "$_neg"' EXIT
printf '// the generator was renamed owl-api -> athena-make\nconst x = 1;\n' > "$_neg/comment_only.ts"
printf '// a note\nimport { thing } from "owl-api";\n' > "$_neg/live_reference.ts"
_caught=$(for f in "$_neg"/*.ts; do strip_comments "$f" | grep -qE "$RETIRED" && basename "$f"; done)
if [ "$_caught" = "live_reference.ts" ]; then
  echo "retired-name guard: negative proof OK — a comment is ignored, a live reference is caught"
else
  echo "retired-name guard: FAIL — the negative proof did not separate the two states (caught: '${_caught:-nothing}')" >&2
  exit 1
fi

echo "retired-name guard: PASS — no live references to, and no files named for, $RETIRED"
