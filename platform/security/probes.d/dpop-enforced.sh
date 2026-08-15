#!/usr/bin/env bash
# GREEN = flag ON for the RUNNING clearing AND the PUBLIC surface refuses an
# unauthenticated protected-API request, NAMING the refusal (401/403).
#
# Local curls are useless here: gate() trusts isLocal(req) by design, so a
# localhost 200 proves nothing about enforcement. The public hostname is the
# surface Jeff's phone uses — that is where enforcement is or isn't.
#
# KNOWN RED (2026-08-15): the public surface answers 404, not 401/403 — access
# IS refused, but the refusal cannot be told from not-found. That is the
# "gate refusal must name its state" defect (Jeff's 08-08 ruling, the quintet's
# 6th property). This probe stays red until the gate names its state.
launchctl print gui/501/com.chorus.clearing 2>/dev/null | grep -q "CHORUS_CLEARING_REQUIRE_DPOP => 1" || { echo "flag not set on running unit"; exit 1; }
code=$(curl -s -o /dev/null -w '%{http_code}' --max-time 10 "https://chorus.lightlifeurbangardens.com/api/stream")
echo "public protected route unauthenticated -> HTTP $code (want 401/403; 404 = refusal that cannot name its state)"
case "$code" in 401|403) exit 0 ;; *) exit 1 ;; esac
