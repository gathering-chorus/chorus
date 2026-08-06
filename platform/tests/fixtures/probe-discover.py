#!/usr/bin/env python3
"""Call the guard's OWN discover() against a stub issuer (#3771).

    probe-discover.py <guard.py> <issuer-url> [user-agent-override]

Exits 0 if discovery succeeded, 1 if it was refused.

Executes the guard's real source with __name__ set to something other than
"__main__", so every function is the genuine one and the server never starts. A
reimplementation of fetch() here would defeat the point — it would agree with a
broken original and prove nothing.

The caller must supply the environment the guard needs to reach its own
module-level configuration (SHARE_ALLOW, SHARE_PRINCIPALS_FILE, SHARE_STATE_FILE,
SHARE_OIDC_OFFLINE), because the guard fails closed on a missing policy and would
otherwise exit before defining anything.
"""
import sys

guard_path, issuer = sys.argv[1], sys.argv[2]
ns = {"__name__": "probe"}
exec(compile(open(guard_path).read(), "guard", "exec"), ns)

ns["ISSUER"] = issuer.rstrip("/")
if len(sys.argv) > 3:
    ns["USER_AGENT"] = sys.argv[3]

try:
    ns["discover"]()
except Exception:
    sys.exit(1)
sys.exit(0)
