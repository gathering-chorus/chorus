#!/usr/bin/env python3
"""
#3433 — Properties domain foundation: SHACL conformance test for chorus:PropertyShape.

The shape has teeth or it doesn't. This validates real Property instances against
the live shape in chorus.ttl using the jena `shacl` CLI (not a parse-check):
  - a well-formed Property (key + value + scope) CONFORMS,
  - a Property missing any required field (value, scope, key) is REJECTED.
If this is green, the foundation is real: the model rejects malformed config at the
shape boundary, which is what #3436 (validate-on-write) builds on.

Run: python3 3433-property-shape.py
"""
import os, subprocess, tempfile, sys

HERE = os.path.dirname(__file__)
SHAPES = os.path.abspath(os.path.join(HERE, "..", "..", "roles/silas/ontology/chorus.ttl"))
PREFIX = "@prefix chorus: <https://jeffbridwell.com/chorus#> .\n"

VALID = PREFIX + '''chorus:p_valid a chorus:Property ;
    chorus:propertyKey "alert.threshold" ;
    chorus:propertyValue "0.9" ;
    chorus:propertyScope "global" .
'''
NO_VALUE = PREFIX + '''chorus:p_noval a chorus:Property ;
    chorus:propertyKey "alert.threshold" ;
    chorus:propertyScope "global" .
'''
NO_SCOPE = PREFIX + '''chorus:p_noscope a chorus:Property ;
    chorus:propertyKey "alert.threshold" ;
    chorus:propertyValue "0.9" .
'''
NO_KEY = PREFIX + '''chorus:p_nokey a chorus:Property ;
    chorus:propertyValue "0.9" ;
    chorus:propertyScope "global" .
'''

fails = []
def check(name, cond):
    print(("PASS" if cond else "FAIL"), name)
    if not cond:
        fails.append(name)

def conforms(data_ttl):
    """Run jena shacl against chorus.ttl; True iff the data conforms (no violations)."""
    with tempfile.NamedTemporaryFile("w", suffix=".ttl", delete=False) as fh:
        fh.write(data_ttl)
        path = fh.name
    try:
        out = subprocess.run(["shacl", "validate", "--shapes", SHAPES, "--data", path],
                             capture_output=True, text=True, timeout=120)
        report = out.stdout + out.stderr
        # jena emits a SHACL report in Turtle: conforms => "sh:conforms  true" and no
        # sh:result nodes; a violation => sh:conforms false + sh:result/sh:Violation.
        if "sh:conforms" in report:
            line = [l for l in report.splitlines() if "sh:conforms" in l][0]
            return "true" in line
        # fallback: no violation node => conforms
        return "sh:Violation" not in report and "sh:result " not in report
    finally:
        os.unlink(path)

check("well-formed Property (key+value+scope) conforms to PropertyShape", conforms(VALID))
check("Property missing propertyValue is REJECTED", not conforms(NO_VALUE))
check("Property missing scope is REJECTED", not conforms(NO_SCOPE))
check("Property missing propertyKey is REJECTED", not conforms(NO_KEY))

print(f"\n{'ALL GREEN' if not fails else 'FAILURES: ' + ', '.join(fails)}")
sys.exit(1 if fails else 0)
