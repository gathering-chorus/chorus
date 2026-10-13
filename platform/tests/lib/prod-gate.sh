#!/usr/bin/env bash
# #4022 — the one gate for suites that write PRODUCTION on purpose (4279, 4022 live).
# Source it; prod_gate_reason <root> <runner> <api> prints why the suite must not
# run here, or nothing. WERK_TEST_NIGHTLY alone is not the nightly: werk-test sets
# it for EVERY suite it spawns (main.rs apply_suite_world), card runs included, so
# on 2026-10-13 a card pipeline wrote production. A tree under chorus-werk/ is a
# card's werk and never writes production unless a person says so.
prod_gate_reason() { # $1 root $2 runner $3 api
  if [ "${QUARTET_PROD_CONFIRM:-}" != "yes" ]; then
    case "$1" in
      */chorus-werk/*) echo "a card's werk ($1) never writes production — set QUARTET_PROD_CONFIRM=yes by hand (#4022)"; return ;;
    esac
    [ "${WERK_TEST_NIGHTLY:-}" = "1" ] || { echo "not the nightly and no QUARTET_PROD_CONFIRM=yes — this suite writes PRODUCTION on purpose (#4279)"; return; }
  fi
  [ -x "$2" ] || { echo "runner missing: $2"; return; }
  curl -sf --max-time 5 "$3/health" >/dev/null 2>&1 || echo "$3 not answering"
}
