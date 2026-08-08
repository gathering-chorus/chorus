#!/usr/bin/env python3
"""Drive the guard's real safe_return() over the return-URL vectors (#3791).

    probe-safe-return.py <guard.py>

Exit 0 when every vector behaves, 1 with the failures printed.

The guard's OWN function is executed, not a copy: a validator reimplemented in
its test agrees with itself while both are wrong, and this one guards an open
redirect — the failure mode is our sign-in page being used as a credible hop to
somebody else's site, with our domain in the link the victim clicked.
"""
import sys

guard = sys.argv[1]
ns = {"__name__": "probe"}
exec(compile(open(guard).read().split("ALLOW, ALLOW_SOURCE = load_allow()")[0], "guard", "exec"), ns)
safe_return = ns["safe_return"]
# #3796 — a refusal now falls back to the guard's LANDING page, not "/". Comparing
# against a hardcoded "/" here would read every refusal as an ACCEPT: the probe
# would go green while the validator refused nothing. Read the constant.
LANDING = ns["LANDING"]

ACCEPT, REFUSE = "accept", "refuse"

VECTORS = [
    # --- the point of the card: cross-host return has to work ---
    ("https://clearing.lightlifeurbangardens.com/room", ACCEPT,
     "the Clearing bounce — the case that sent people to the guard root instead"),
    ("https://lightlifeurbangardens.com/chorus", ACCEPT,
     "the app leg (#3778): the apex itself, not a subdomain"),
    ("https://chorus.lightlifeurbangardens.com/athena/model.html", ACCEPT,
     "our own host, absolute"),
    ("/athena/model.html", ACCEPT,
     "host-relative still works, unregressed"),

    # --- the two host-spoofing shapes, which are the whole reason to parse ---
    ("https://evil-lightlifeurbangardens.com/x", REFUSE,
     "ends with our name without a dot boundary — what a naive endswith() admits"),
    ("https://lightlifeurbangardens.com.evil.tld/x", REFUSE,
     "our name as a prefix of theirs — what a naive 'contains' admits"),
    ("https://notlightlifeurbangardens.com/x", REFUSE,
     "same class, different spelling"),

    # --- scheme ---
    ("http://clearing.lightlifeurbangardens.com/x", REFUSE,
     "http downgrades the session in transit"),
    ("javascript:alert(1)", REFUSE, "script scheme"),
    ("data:text/html,<script>", REFUSE, "data scheme"),

    # --- shapes a path check waves through ---
    ("//evil.tld/x", REFUSE,
     "protocol-relative: a browser reads this as another HOST, a startswith('/') check reads it as a path"),
    ("", REFUSE, "absent"),
    ("   ", REFUSE, "whitespace"),
]

bad = []
for raw, want, why in VECTORS:
    got = safe_return(raw)
    accepted = got != LANDING
    if (want == ACCEPT) != accepted:
        bad.append(f"{raw!r}: wanted {want}, got {got!r} — {why}")

if bad:
    for b in bad:
        print(b)
    sys.exit(1)
sys.exit(0)
