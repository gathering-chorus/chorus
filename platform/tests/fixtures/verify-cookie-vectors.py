#!/usr/bin/env python3
"""Run the shared cookie vectors through the guard's REAL verifier (#3790).

    verify-cookie-vectors.py <guard.py> <vectors.json> [--wrong-key]

Exit 0 when every vector matches its expected verdict, 1 otherwise.

One artifact, two consumers: this asserts the Python side, Wren's TypeScript
verifier asserts the same file. DEC-2209 clause 2 asks for a single shared
verifier; until #3685 delivers that crate there are two implementations of one
contract, and a drift between them would show up as a login that works on one
surface and not the other — found by a person, on a Tuesday.

--wrong-key is the negative proof: verify the VALID vector under a key that isn't
the fixture's, and require refusal. Without it, a verifier that accepted anything
would pass the main run and the whole fixture would prove nothing.
"""
import json
import sys

guard, vectors = sys.argv[1], sys.argv[2]
wrong_key = "--wrong-key" in sys.argv

# Execute only the definitions — everything before module-level configuration,
# which would read policy files and exit.
src = open(guard).read().split("ALLOW, ALLOW_SOURCE = load_allow()")[0]
ns = {"__name__": "vectors"}
exec(compile(src, "guard", "exec"), ns)

fx = json.load(open(vectors))
now = fx["now"]

if wrong_key:
    valid = next(v for v in fx["vectors"] if v["name"] == "valid")
    got = ns["verify_session"](valid["cookie"], b"a-different-key-entirely-000000000", now=now)
    if got is not None:
        print(f"the valid vector verified under the WRONG key (got {got!r}) — "
              "the signature check is not doing anything")
        sys.exit(1)
    sys.exit(0)

key = ns["b64u_decode"](fx["key_b64u"])
bad = []
for v in fx["vectors"]:
    got = ns["verify_session"](v["cookie"], key, now=now)
    if v["expect"] == "accept":
        if got != v.get("webid"):
            bad.append(f"{v['name']}: expected the webid, got {got!r} — {v['why']}")
    elif got is not None:
        bad.append(f"{v['name']}: expected refusal, got {got!r} — {v['why']}")

if bad:
    for b in bad:
        print(b)
    sys.exit(1)
sys.exit(0)
