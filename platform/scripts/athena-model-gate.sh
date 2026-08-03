#!/usr/bin/env bash
# athena-model-gate — #3718. The back door, closed.
#
# THE PROBLEM THIS EXISTS FOR (measured 2026-07-30..08-02):
# The DAL governs INSTANCE writes and enforces plenty — ADR-040 IRI minting,
# SHACL required/enum/datatype/uniqueness, identity, a graph allowlist. None of
# it prevented this week's defects, for two reasons:
#
#   1. The model is not written through the DAL. Files are hand-edited, then
#      chorus-model-deploy.sh POSTs turtle straight to Fuseki's graph-store
#      endpoint. The DAL is never in the path.
#   2. assert_dal_writable marks urn:chorus:ontology DBA-path-only (#3356), so
#      SCHEMA cannot go through the DAL by design.
#
# Net: the DAL governs instances; NOTHING governs schema. Schema is where every
# failure of the last three days lived — 186 classes over 13 files and 6
# namespaces, 69 binding nothing, 11 properties with no domain/range, a shape
# and an ontology disagreeing about what a Domain is.
#
# A governed writer you can bypass with a text editor is theatre.
#
# THE DESIGN CHOICE THAT MATTERS: this gate does NOT ask "was this written by
# athena-model?" It asks "does this CONFORM?" Provenance is unenforceable — a
# hand-edit and a tool-write are byte-identical in git — and it is also the
# wrong question. Conformance is checkable, and a human editing a .ttl correctly
# should not be refused for using the wrong tool. Same shape as the clippy
# ratchet: it does not care who typed the code, only whether it passes.
#
# NOT ARMED IN pre-commit YET. It runs and reports. Arming waits on the
# definesVocabulary backfill (Silas) — the unclaimed-class refusal is correct
# but would refuse legitimate work until every served class is claimed.
#
# Usage:
#   athena-model-gate.sh                 # check .ttl files staged for commit
#   athena-model-gate.sh --all           # check every .ttl in the repo
#   athena-model-gate.sh <file>...       # check specific files
set -uo pipefail

CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"
BIN="${ATHENA_MODEL_BIN:-$(command -v athena-model 2>/dev/null || echo "$CHORUS_ROOT/platform/services/chorus-model/target/release/athena-model")}"

if [ ! -x "$BIN" ]; then
  echo "athena-model-gate: binary not found at $BIN" >&2
  echo "  build it: cargo build --release -p athena-model" >&2
  exit 127
fi

# Which files to check.
case "${1:-}" in
  --all) mapfile -t FILES < <(cd "$CHORUS_ROOT" && git ls-files '*.ttl') ;;
  "")    mapfile -t FILES < <(cd "$CHORUS_ROOT" && git diff --cached --name-only --diff-filter=ACM -- '*.ttl') ;;
  *)     FILES=("$@") ;;
esac

if [ "${#FILES[@]}" -eq 0 ]; then
  echo "athena-model-gate: no .ttl files to check"
  exit 0
fi

# The deploy set — parsed from the deploy script, never copied. A hand-kept list
# here would reproduce the authored-vs-deployed gap the gate exists to catch.
DEPLOY_SET="$(grep -oE '\$CHORUS_ROOT/[A-Za-z0-9/_.-]+\.ttl' \
  "$CHORUS_ROOT/platform/scripts/chorus-model-deploy.sh" 2>/dev/null \
  | sed 's|\$CHORUS_ROOT/||' | sort -u)"

fail=0
checked=0
unchecked=0
for f in "${FILES[@]}"; do
  rel="${f#"$CHORUS_ROOT"/}"
  [ -f "$CHORUS_ROOT/$rel" ] || continue

  # Every chorus: class this file declares. The gate checks the SCHEMA layer for
  # each — that is the half nothing governs today.
  classes="$(grep -oE '^chorus:[A-Za-z][A-Za-z0-9_-]* +a +owl:Class' "$CHORUS_ROOT/$rel" 2>/dev/null \
    | sed 's/^chorus://; s/ .*//' | sort -u)"

  # A file can declare classes in ANOTHER namespace (borg:, jb:, icd:, fw:).
  # Those are real declarations this gate cannot check — and saying nothing
  # about them would report OK over unchecked content, which is exactly the
  # "a check that cannot distinguish the two states it exists to separate"
  # disease we have now found five times this week. Name the blind spot.
  foreign="$(grep -oE '^[a-z][a-z0-9]*:[A-Za-z][A-Za-z0-9_-]* +a +owl:Class' "$CHORUS_ROOT/$rel" 2>/dev/null \
    | grep -v '^chorus:' | sed 's/:.*//' | sort -u | tr '\n' ' ')"
  if [ -n "$foreign" ]; then
    fcount="$(grep -cE '^[a-z][a-z0-9]*:[A-Za-z][A-Za-z0-9_-]* +a +owl:Class' "$CHORUS_ROOT/$rel" 2>/dev/null)"
    echo "UNCHECKED  $rel"
    echo "   declares $fcount class(es) in non-chorus namespace(s): ${foreign% }"
    echo "   This gate only checks the chorus: namespace. Reported so it is a KNOWN"
    echo "   blind spot, not a silent pass. (#1772 namespace convergence is the fix.)"
    unchecked=$((unchecked + 1))
  fi

  [ -z "$classes" ] && continue

  # Is this file in a deploy set at all? Reported ONCE per file rather than once
  # per class — the same refusal repeated 40 times is noise, not legibility.
  if ! printf '%s\n' "$DEPLOY_SET" | grep -qxF "$rel"; then
    echo "REFUSED  $rel"
    echo "   [manifest-membership] in neither MODEL_SET nor INSTANCE_SET — the classes"
    echo "                         declared here bind nothing. 69 of 186 classes (38%) are"
    echo "                         in this state today; APISurface has 25 LIVE instances"
    echo "                         from a file no manifest references."
    fail=1
  fi

  while IFS= read -r c; do
    [ -z "$c" ] && continue
    checked=$((checked + 1))
    # --definition: a class declaration is always the schema layer.
    if ! out="$(CHORUS_ROOT="$CHORUS_ROOT" "$BIN" check \
                 --class "$c" --file "$rel" --definition 2>&1)"; then
      echo "REFUSED  $rel :: $c"
      printf '%s\n' "$out" | sed 's/^/   /'
      fail=1
    fi
  done <<< "$classes"
done

echo
if [ "$fail" -eq 0 ]; then
  if [ "$checked" -eq 0 ]; then
    # NEVER report OK over zero checks. A green that checked nothing is the
    # thing this whole card exists to remove.
    echo "athena-model-gate: NOTHING CHECKED — 0 chorus: class declarations found"
    [ "$unchecked" -gt 0 ] && echo "  ($unchecked file(s) declare classes in other namespaces — see above)"
    echo "  This is NOT a pass. It means the gate had nothing it could judge."
    exit 0
  fi
  echo "athena-model-gate: OK — $checked class declaration(s) conform"
  [ "$unchecked" -gt 0 ] && echo "  WITH $unchecked file(s) partially unchecked — see UNCHECKED above"
  exit 0
fi
echo "athena-model-gate: REFUSED — see above."
echo "  This gate checks CONFORMANCE, not authorship. Fix the declaration, or"
echo "  add the file to MODEL_SET/INSTANCE_SET if it is meant to deploy."
exit 1
