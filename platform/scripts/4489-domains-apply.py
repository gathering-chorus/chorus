#!/usr/bin/env python3
"""#4489 — move every link off the five retired domains, through the door.

Dry run by default: lists each row it would change. `--apply` sends the
PATCHes (and the DELETEs of the alerts-monitors appointments, whose twins on
alerts and monitors already exist). `--count` prints the links still pointing at
the five and exits 1 when any remain, so a land can prove AC1.

Rulings (Jeff 2026-10-10, on the card): chorus-domain -> awareness;
athena-domain -> domains; convergence-domain -> integrations (no links);
alerts-monitors splits into monitors and alerts by Silas's measured unit list;
gates -> policies, except the guard and membrane events, which are security.

Needs CHORUS_IDENTITY_TOKEN for writes (platform/scripts/chorus-identity-token).
"""
import json
import os
import sys
import urllib.parse
import urllib.request

DOOR = os.environ.get("ATHENA_DOOR", "http://localhost:3360")
RETIRED = {"alerts-monitors", "athena-domain", "chorus-domain", "convergence-domain", "gates"}
BASE = "https://jeffbridwell.com/chorus#"

MONITORS = {"grafana", "blackbox-exporter", "heartbeat-probe", "heartbeat", "eventloop-probe",
            "deep-health", "clearing-probe", "chorus-health"}
ALERTS = {"infra-alert", "alertmanager", "alert-runner", "alert-notifier", "alert-delivery-test"}
SECURITY_EVENTS = {"eventtype-guard-classify-decided", "eventtype-guard-rule-decided",
                   "eventtype-guard-scrub-blocked", "eventtype-guard-sparql-warned",
                   "eventtype-membrane-violation"}


def home(old, row):
    """The domain a row's link to `old` moves to."""
    if old == "chorus-domain":
        return "awareness"
    if old == "athena-domain":
        return "domains"
    if old == "convergence-domain":
        return "integrations"
    if old == "gates":
        return "security" if row.get("name") in SECURITY_EVENTS else "policies"
    if old == "alerts-monitors":
        unit = row.get("launchdLabel") or row.get("name", "").split(".")[-1]
        unit = unit.split(".")[-1]
        if unit in MONITORS:
            return "monitors"
        if unit in ALERTS:
            return "alerts"
        raise SystemExit(f"4489: unit {row.get('name')} is in neither the monitors nor the alerts list")
    raise SystemExit(f"4489: no home for {old}")


# route, field — every place the baseline (07:41) found a link
ROUTES = [
    ("infrastructure/unitdomainmappings", "hasDomain"),
    ("events/types", "producedBy"),
    ("security/authboundaries", "betweenDomainB"),
    ("knowledge/documents", "hasDomain"),
    ("code/files", "hasDomain"),
    ("tests/tests", "covers"),
    ("domains/domains", "consumes"),
]
APPOINTMENTS = ("roles/appointments", "overAnchor")


def bare(v):
    return v[len(BASE):] if isinstance(v, str) and v.startswith(BASE) else v


def http(method, path, body=None):
    req = urllib.request.Request(f"{DOOR}{path}", method=method)
    if body is not None:
        req.data = json.dumps(body).encode()
        req.add_header("Content-Type", "application/json")
    tok = os.environ.get("CHORUS_IDENTITY_TOKEN")
    if tok and method != "GET":
        req.add_header("Authorization", "Bearer " + tok)
    try:
        with urllib.request.urlopen(req, timeout=60) as r:
            return r.status, json.loads(r.read() or b"{}")
    except urllib.error.HTTPError as e:
        return e.code, {"error": e.read().decode()[:300]}


def rows(route):
    st, body = http("GET", f"/v1/{route}?limit=20000")
    if st != 200:
        raise SystemExit(f"4489: the door does not serve /v1/{route} ({st}); nothing changed")
    return body.get("items") or body.get("data") or []


def hits(route, field):
    out = []
    for r in rows(route):
        v = r.get(field)
        vals = v if isinstance(v, list) else ([] if v in (None, "") else [v])
        old = [x for x in vals if bare(x) in RETIRED]
        if old:
            out.append((r, vals, old))
    return out


def main():
    mode = sys.argv[1] if len(sys.argv) > 1 else "--dry-run"
    left = 0
    for route, field in ROUTES:
        for r, vals, old in hits(route, field):
            left += len(old)
            new = []
            for x in vals:
                y = home(bare(x), r) if bare(x) in RETIRED else bare(x)
                if y not in new:
                    new.append(y)
            value = new if isinstance(r.get(field), list) else new[0]
            print(f"{route}/{r['name']}: {field} {[bare(o) for o in old]} -> {value}")
            if mode == "--apply":
                st, body = http("PATCH", f"/v1/{route}/{urllib.parse.quote(r['name'])}", {field: value})
                if st >= 300:
                    raise SystemExit(f"4489: PATCH {route}/{r['name']} refused {st} {body}")
    route, field = APPOINTMENTS
    for r, _vals, _old in hits(route, field):
        left += 1
        print(f"{route}/{r['name']}: delete (its twins over alerts and monitors stay)")
        if mode == "--apply":
            st, body = http("DELETE", f"/v1/{route}/{urllib.parse.quote(r['name'])}")
            if st >= 300:
                raise SystemExit(f"4489: DELETE {route}/{r['name']} refused {st} {body}")
    print(f"links to the five retired domains: {left}")
    if mode == "--count":
        sys.exit(1 if left else 0)


if __name__ == "__main__":
    main()
