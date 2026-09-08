#!/bin/bash
# @test-type: fitness
# no-teammate-default-guard: exempt — this check must contain the names it hunts.
#
# #3959 — a role default may never be a TEAMMATE'S NAME.
#
# Two sites defaulted to `silas` when no role was in the environment, so events
# produced by nobody-in-particular were written under a colleague who had not
# acted. Combined with ~26,000 unattributed events a day, that is part of why one
# role showed four times another's event count on 2026-08-21 and Wren's own work
# looked like silence.
#
# `system` is an honest actor for a script running outside a role session. A
# person is not. This guard does not care how the default is spelled — env
# fallback, unwrap_or, ?? — only that a human's name is never the answer.
set -u
SELF="$(cd "$(dirname "$0")" && pwd)/$(basename "$0")"
# The tree this file lives in, NOT $CHORUS_ROOT.
#
# #4113 (Wren) and #4111 (Kade) found this independently, in different suites, the
# same week: reading the env var made a werk's copy grade CANONICAL's files, so it
# reported the same hits no matter what the werk changed and a fix could never turn
# it green from where the fix was made. Same class as #3701's ratchet pin.
#
# SCAN_ROOT is the deliberate override — this suite's own self-tests use it to point
# the guard at a fixture tree, which is how its negative proofs run at all.
ROOT="${SCAN_ROOT:-$(cd "$(dirname "$0")/../.." && pwd)}"

# A guard whose search target moved must fail LOUDLY, never pass vacuously (#3734).
for d in platform directing; do
  [ -d "$ROOT/$d" ] || { echo "no-teammate-default: FAIL — search root $ROOT/$d does not exist" >&2; exit 1; }
done

# `:-wren` / `:-silas` / `:-kade` in a shell default, and the Rust/TS equivalents.
PATTERNS='(:-(wren|silas|kade)\}|unwrap_or_else\(\|_\| *"(wren|silas|kade)"|unwrap_or\("(wren|silas|kade)"|\?\? *.(wren|silas|kade).)'

hits=$(grep -rIn -E "$PATTERNS" \
  --include="*.sh" --include="*.rs" --include="*.ts" \
  "$ROOT/platform" "$ROOT/directing" 2>/dev/null \
  | grep -v node_modules | grep -v "/target/" | grep -vF "$SELF" \
  | while IFS= read -r hit; do
      f="${hit%%:*}"
      # a test fixture naming a role is data, not a production default
      skip=no
      if [ "${f#*test}" != "$f" ] || [ "${f#*spec}" != "$f" ]; then skip=yes; fi
<<<<<<< HEAD
      # #4111 — and neither is a COMMENT. A default is a line of code; a note
      # ABOUT one is the record of why it changed. This guard flagged the very
      # comment written to explain a fix it had just demanded, which would make
      # the price of satisfying it the deletion of its own rationale.
      # Shallow on purpose: only the first token of the line decides.
      code="${hit#*:}"; code="${code#*:}"
      if printf '%s' "$code" | grep -qE '^[[:space:]]*(#|//|\*|--|/\*)'; then skip=yes; fi
=======
      # #4113 — WHO AM I vs WHO DO I TELL. This guard exists so a script never ACTS
      # under a teammate name. A default RECIPIENT is different: the security lane
      # belongs to Silas, and routing its red to him is ownership, not impersonation.
      # Flagging those forced a correct routing default to be broken to satisfy the
      # guard, which then broke the nudge-routing test — a guard that cannot tell the
      # two apart makes the codebase worse.
      if printf '%s' "$hit" | grep -qiE '(_owner=|_OWNER:-|recipient|notify_target)'; then skip=yes; fi
>>>>>>> 9575914fa (wren: #4119 — repairing three suites my own #4113 land broke, and the guard that made me break a correct default)
      [ "$skip" = yes ] || echo "$hit"
    done | sort)

n=$(printf "%s" "$hits" | grep -c . || true)
if [ "$n" -gt 0 ]; then
  echo "no-teammate-default: FAIL — $n site(s) default a role to a teammate's name"
  printf "%s\n" "$hits" | head -20 | sed 's/^/  /'
  echo "  Use \"system\" for a script running outside a role session, or refuse." >&2
  exit 1
fi
<<<<<<< HEAD
# #4111 negative proofs. The comment exemption above is exactly the kind of
# widening that can quietly disarm a guard, so prove both directions every run.
st=$(mktemp -d); trap 'rm -rf "$st"' EXIT
mkdir -p "$st/platform" "$st/directing"
printf '%s\n' '# ROLE="${NIGHTLY_ROLE:-kade}" was the old default' > "$st/platform/prose.sh"
printf '%s\n' 'ROLE="${NIGHTLY_ROLE:-kade}"' > "$st/platform/live.sh"
printf '%s\n' '# the old default was :-silas}' 'OWNER="${SEC_OWNER:-silas}"' > "$st/platform/mixed.sh"

selftest_fail=0
probe() {
  local label="$1" want="$2"
  local got
  got=$(SCAN_ROOT="$st" bash "$SELF" 2>/dev/null | grep -c "site(s) default" || true)
  if [ "$got" = "$want" ]; then echo "  self-test PASS: $label"; else
    echo "  self-test FAIL: $label — expected $want failing-run(s), got $got"; selftest_fail=1; fi
}
echo "no-teammate-default: self-test"
if [ -z "${NO_TEAMMATE_SELFTEST:-}" ]; then
  export NO_TEAMMATE_SELFTEST=1
  rm -f "$st/platform/live.sh" "$st/platform/mixed.sh"
  probe "a comment quoting the old default is NOT a violation" 0
  printf '%s\n' 'ROLE="${NIGHTLY_ROLE:-kade}"' > "$st/platform/live.sh"
  probe "NEGATIVE PROOF: a real default still fails the guard" 1
  rm -f "$st/platform/live.sh"
  printf '%s\n' '# the old default was :-silas}' 'OWNER="${SEC_OWNER:-silas}"' > "$st/platform/mixed.sh"
  probe "NEGATIVE PROOF: a comment above a real default does not launder it" 1
  unset NO_TEAMMATE_SELFTEST
  [ "$selftest_fail" -eq 0 ] || { echo "no-teammate-default: FAIL — the guard cannot separate prose from a default"; exit 1; }
=======
# NEGATIVE PROOF (#3734) — the recipient exemption must not blind the guard to actors.
_np=$(mktemp -d); trap 'rm -rf "$_np"' EXIT
printf 'sec_owner="${X:-silas}"\n' > "$_np/recipient.sh"
printf 'ROLE="${CHORUS_ROLE:-silas}"\n' > "$_np/actor.sh"
_caught=$(grep -rIn -E "$PATTERNS" "$_np" 2>/dev/null | while IFS= read -r hit; do
  printf '%s' "$hit" | grep -qiE '(_owner=|_OWNER:-|recipient|notify_target)' || basename "${hit%%:*}"
done | sort -u)
if [ "$_caught" = "actor.sh" ]; then
  echo "no-teammate-default: negative proof OK — an ACTOR default is caught, a RECIPIENT default is not"
else
  echo "no-teammate-default: FAIL — the exemption does not separate actor from recipient (caught: '${_caught:-nothing}')" >&2
  exit 1
>>>>>>> 9575914fa (wren: #4119 — repairing three suites my own #4113 land broke, and the guard that made me break a correct default)
fi

echo "no-teammate-default: PASS — no role defaults to a teammate's name"
