#!/usr/bin/env bash
# GREEN = flag ON for the RUNNING clearing AND the public Clearing surface
# refuses an unauthenticated protected-API request NAMING the refusal (401/403).
#
# #3999 repoint: the old probe hit chorus.llug.com (the share-guard), where
# /api/stream is simply NOT SHARED — its 404 "path not shared" is the CORRECT
# state-naming refusal for that surface (Wren's 08-09 rule, documented in the
# guard). The protected route LIVES on the Clearing surface; that is where DPoP
# enforcement is or isn't. Redirect chains are followed: the auth wall's
# 30x→401 terminus is the named refusal.
# Distinct reds: 401/403=green · 404=probe-target-missing (route moved — fix
# the probe, not the gate) · anything else=enforcement gap.
launchctl print gui/501/com.chorus.clearing 2>/dev/null | grep -q "CHORUS_CLEARING_REQUIRE_DPOP => 1" || { echo "flag not set on running unit"; exit 1; }
code=$(curl -sL -o /dev/null -w '%{http_code}' --max-time 10 "https://lightlifeurbangardens.com/clearing/api/stream")
case "$code" in
  401|403) echo "public clearing route unauthenticated -> HTTP $code (named refusal)"; exit 0 ;;
  404)     echo "probe-target-missing: /clearing/api/stream -> 404 — the ROUTE moved; repoint this probe, do not read as posture"; exit 1 ;;
  *)       echo "public clearing route unauthenticated -> HTTP $code (want 401/403)"; exit 1 ;;
esac
