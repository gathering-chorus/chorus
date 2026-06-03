#!/usr/bin/env bash
# daemon-env-3197.test.sh — #3197 single-source daemon env.
# (Named *.test.sh so the TDD gate recognizes it; the repo's test-*.sh
# convention is invisible to the gate — that gap is a separate follow-on.)
#
# The verb-spawning daemons (chorus-mcp, chorus-api) exec chorus verbs that
# need the same env a role shell has. The failure class: each daemon hand-copied
# a SUBSET of the env into its plist, drifting from the one source — PATH gap
# 2026-06-02, WERK_*_BIN gap 2026-06-03, both found only as a prod deploy ENOENT.
#
# Fix: the wrappers source chorus-env-setup.sh (the single source), and
# env-setup exports all three WERK_<ROLE>_BIN GLOBALLY (ungated from
# CHORUS_ROLE) — so a role-less daemon that sources it still has every role's
# slot for per-request lookup. The slot formula lives ONLY in env-setup;
# chorus-bin-install READS the exported var, it does not re-derive it (one
# source, no competing derivation).
set -uo pipefail

DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd -P)"
fail=0
ok()  { echo "ok - $1"; }
bad() { echo "not ok - $1"; fail=1; }

# --- T1/T2: wrappers single-source the env ----------------------------------
for wrapper in chorus-mcp-wrapper.sh chorus-api-wrapper.sh; do
  if grep -q 'chorus-env-setup.sh' "$DIR/$wrapper" 2>/dev/null; then
    ok "$wrapper sources chorus-env-setup.sh"
  else
    bad "$wrapper does NOT source chorus-env-setup.sh (daemon env drifts from the one source)"
  fi
done

# --- T3: env-setup exports the werk slots GLOBALLY, even role-less -----------
# The daemon (chorus-mcp / chorus-api) boots with NO CHORUS_ROLE. After sourcing
# env-setup it must still hold all three WERK_<ROLE>_BIN so a per-request role
# resolves its slot. Pre-fix: role-gated → a role-less source exported none →
# chorus-bin-install --target werk exit-7'd on every deploy.
ENV_SETUP="$DIR/chorus-env-setup.sh"
TMPB="$(mktemp -d)"
slots_out="$(
  env -u CHORUS_ROLE -u WERK_KADE_BIN -u WERK_WREN_BIN -u WERK_SILAS_BIN \
    CHORUS_WERK_BASE="$TMPB/werkbase" \
    bash -c "source '$ENV_SETUP' >/dev/null 2>&1; printf 'K=%s W=%s S=%s' \"\$WERK_KADE_BIN\" \"\$WERK_WREN_BIN\" \"\$WERK_SILAS_BIN\""
)"
if [ "$slots_out" = "K=$TMPB/werkbase/kade-bin W=$TMPB/werkbase/wren-bin S=$TMPB/werkbase/silas-bin" ]; then
  ok "env-setup exports all three WERK_<ROLE>_BIN globally with no CHORUS_ROLE (role-less daemon resolves any role's slot)"
else
  bad "env-setup did NOT globally export the werk slots role-less: $slots_out"
fi
rm -rf "$TMPB"

# --- T4: chorus-bin-install consumes the exported slot (no re-derivation) -----
# With WERK_<ROLE>_BIN present (as env-setup now guarantees), the install lands
# in the slot. This is the consumer side of the single source.
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
SRC="$TMP/dummy-bin"; cp /bin/echo "$SRC"; chmod +x "$SRC"
out="$(
  env CHORUS_ROLE=kade WERK_KADE_BIN="$TMP/kade-bin" \
    CHORUS_BIN_SPINE_LOG="$TMP/spine.log" \
    bash "$DIR/chorus-bin-install" --target werk "$SRC" dummy-bin 2>&1
)"
rc=$?
if [ -x "$TMP/kade-bin/dummy-bin" ]; then
  ok "chorus-bin-install --target werk installed into WERK_KADE_BIN"
else
  bad "chorus-bin-install --target werk failed (exit $rc): $out"
fi

if [ "$fail" -eq 0 ]; then echo "PASS: daemon env single-sourced; werk slot global + consumed"; else echo "FAIL: daemon env still drifts"; fi
exit "$fail"
