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
      # #4111 — and neither is a COMMENT. A default is a line of code; a note
      # ABOUT one is the record of why it changed. This guard flagged the very
      # comment written to explain a fix it had just demanded, which would make
      # the price of satisfying it the deletion of its own rationale.
      # Shallow on purpose: only the first token of the line decides.
      code="${hit#*:}"; code="${code#*:}"
      if printf '%s' "$code" | grep -qE '^[[:space:]]*(#|//|\*|--|/\*)'; then skip=yes; fi
      [ "$skip" = yes ] || echo "$hit"
    done | sort)

n=$(printf "%s" "$hits" | grep -c . || true)
if [ "$n" -gt 0 ]; then
  echo "no-teammate-default: FAIL — $n site(s) default a role to a teammate's name"
  printf "%s\n" "$hits" | head -20 | sed 's/^/  /'
  echo "  Use \"system\" for a script running outside a role session, or refuse." >&2
  exit 1
fi
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

  # #4119 — the RECIPIENT exemption #4113 added here is GONE, and so is the proof
  # that went with it. Two reasons, and the second is the real one.
  #
  # It collided: my rule exempted anything matching `_OWNER:-`, which swallowed
  # #4111's own fixture `OWNER="${SEC_OWNER:-silas}"` — an ACTOR default it wrote
  # to prove a comment cannot launder a real one. Two exemptions, each correct
  # about its own case, and together a hole.
  #
  # And it was already dead. The only site it existed for was
  # `NIGHTLY_SECURITY_OWNER:-silas` in nightly-suites.sh, and #4111 replaced that
  # literal with `domain_owner security` — derived from the model, no teammate
  # name in the source at all. With the exemption removed the guard passes over
  # the whole tree, which is the measurement that says nothing needed it.
  #
  # The distinction it encoded (who I ACT as vs who I TELL) is still true and
  # still worth knowing; it just does not need an exemption in a guard right now.
  # If a real recipient default comes back, narrow it to that variable by name.
fi

echo "no-teammate-default: PASS — no role defaults to a teammate's name"
