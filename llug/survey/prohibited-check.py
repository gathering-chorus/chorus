#!/usr/bin/env python3
"""#4119 — is this plant prohibited in Massachusetts?

Takes a name (as a plant-ID service would return it), resolves it through GBIF's
backbone, and walks the resulting key's PARENT CHAIN against the resolved MDAR list.

Why the chain: eleven MDAR rows are genus-level (`Cuscuta spp.`), so an identification
of a species has to match its genus row. And the list header bans "all cultivars,
varieties and hybrids", so a subspecies has to match its species row. Neither works by
comparing names as text — which is also how you miss the two names the state itself
misspelled, and Japanese knotweed, which is listed under a synonym GBIF will not place
at species rank.

  ./prohibited-check.py "Reynoutria japonica" "Acer rubrum" ...
"""
import json, sys, urllib.parse, urllib.request, os

HERE = os.path.dirname(os.path.abspath(__file__))
KEYS = json.load(open(os.path.join(HERE, "ma-prohibited-keys.json")))

def gbif(path):
    with urllib.request.urlopen("https://api.gbif.org/v1/" + path, timeout=25) as r:
        return json.load(r)

def chain(usage_key):
    """The key plus every ancestor key, as strings."""
    out = [str(usage_key)]
    d = gbif(f"species/{usage_key}")
    for f in ("speciesKey", "genusKey", "familyKey"):
        if d.get(f): out.append(str(d[f]))
    return out

def check(name):
    m = gbif("species/match?name=" + urllib.parse.quote(name))
    if not m.get("usageKey"):
        return {"name": name, "verdict": "UNKNOWN", "why": "GBIF could not match the name"}
    for k in chain(m["usageKey"]):
        if k in KEYS:
            hit = KEYS[k]
            how = "listed directly" if k == str(m["usageKey"]) else f"matched an ancestor ({hit['rank'].lower()} {hit['accepted']})"
            return {"name": name, "verdict": "PROHIBITED", "matchType": m.get("matchType"),
                    "resolved": m.get("canonicalName"), "listedAs": hit["listedAs"],
                    "common": hit["common"], "why": how}
    return {"name": name, "verdict": "not prohibited", "matchType": m.get("matchType"),
            "resolved": m.get("canonicalName"), "why": "no key in the chain is on the list"}

if __name__ == "__main__":
    for n in sys.argv[1:] or ["Reynoutria japonica"]:
        r = check(n)
        print(f"{r['verdict']:<15} {n:<28} {r.get('why','')}"
              + (f"  [listed as: {r['listedAs']}]" if r.get("listedAs") else ""))
