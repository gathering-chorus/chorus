#!/usr/bin/env python3
"""
Build the photos-canonical-rebuild NiFi flow (v2 — SPARQL-join architecture).
Runs on Library, connects to Bedroom NiFi via REST API.

Card: #1644 — Rebuild canonical photo graph
Spec: architect/docs/merge-specification-photos.html
Architecture decision: SPARQL cross-graph join per era (chat with Silas 2026-03-24)

Flow: source-extraction (per-era SPARQL) → field-merge (Jolt) → validation → output + dead-letter
"""

import json
import sys
import urllib.request
import urllib.parse
import ssl

NIFI_URL = "https://192.168.86.242:8443/nifi-api"
NIFI_USER = "admin"
NIFI_PASS = "nifi-gathering-2026"
FUSEKI_URL = "http://192.168.86.36:3030/pods/query"

ctx = ssl.create_default_context()
ctx.check_hostname = False
ctx.verify_mode = ssl.CERT_NONE

def api(method, path, body=None):
    url = f"{NIFI_URL}/{path}"
    data = json.dumps(body).encode() if body else None
    headers = {"Authorization": f"Bearer {TOKEN}", "Content-Type": "application/json"}
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        resp = urllib.request.urlopen(req, context=ctx)
        return json.loads(resp.read()) if resp.status != 204 else {}
    except urllib.error.HTTPError as e:
        print(f"  API ERROR {e.code}: {method} {path}: {e.read().decode()[:200]}")
        return None

def get_token():
    url = f"{NIFI_URL}/access/token"
    data = urllib.parse.urlencode({"username": NIFI_USER, "password": NIFI_PASS}).encode()
    req = urllib.request.Request(url, data=data, method="POST")
    resp = urllib.request.urlopen(req, context=ctx)
    return resp.read().decode()

def create_pg(parent_id, name, x=0, y=0):
    result = api("POST", f"process-groups/{parent_id}/process-groups", {
        "revision": {"version": 0},
        "component": {"name": name, "position": {"x": x, "y": y}}
    })
    if result:
        print(f"  PG: {name} → {result['id']}")
        return result["id"]
    return None

def create_processor(pg_id, proc_type, name, x=0, y=0, props=None, scheduling=None):
    body = {
        "revision": {"version": 0},
        "component": {
            "type": proc_type,
            "name": name,
            "position": {"x": x, "y": y}
        }
    }
    result = api("POST", f"process-groups/{pg_id}/processors", body)
    if not result:
        return None
    proc_id = result["id"]
    version = result["revision"]["version"]
    if props or scheduling:
        update = {"revision": {"version": version}, "component": {"id": proc_id, "config": {}}}
        if props:
            update["component"]["config"]["properties"] = props
        if scheduling:
            update["component"]["config"].update(scheduling)
        result = api("PUT", f"processors/{proc_id}", update)
        if result:
            version = result["revision"]["version"]
    print(f"  Proc: {name} → {proc_id}")
    return proc_id

def create_port(pg_id, name, port_type, x=0, y=0):
    result = api("POST", f"process-groups/{pg_id}/{port_type}", {
        "revision": {"version": 0},
        "component": {"name": name, "position": {"x": x, "y": y}}
    })
    if result:
        label = "In" if "input" in port_type else "Out"
        print(f"  {label}: {name} → {result['id']}")
        return result["id"]
    return None

def connect(pg_id, src_id, dst_id, rels, src_type="PROCESSOR", dst_type="PROCESSOR", src_gid=None, dst_gid=None):
    result = api("POST", f"process-groups/{pg_id}/connections", {
        "revision": {"version": 0},
        "component": {
            "source": {"id": src_id, "type": src_type, "groupId": src_gid or pg_id},
            "destination": {"id": dst_id, "type": dst_type, "groupId": dst_gid or pg_id},
            "selectedRelationships": rels
        }
    })
    return result["id"] if result else None

def auto_terminate(proc_id, rels):
    proc = api("GET", f"processors/{proc_id}")
    if not proc:
        return
    api("PUT", f"processors/{proc_id}", {
        "revision": {"version": proc["revision"]["version"]},
        "component": {"id": proc_id, "config": {"autoTerminatedRelationships": rels}}
    })

def delete_pg_recursive(pg_id):
    """Delete a process group and all contents."""
    pg = api("GET", f"process-groups/{pg_id}")
    if not pg:
        return
    children = api("GET", f"process-groups/{pg_id}/process-groups")
    if children:
        for child in children.get("processGroups", []):
            delete_pg_recursive(child["id"])
    for coll_key, endpoint in [("connections", "connections"), ("processors", "processors"),
                                ("inputPorts", "input-ports"), ("outputPorts", "output-ports")]:
        items = api("GET", f"process-groups/{pg_id}/{endpoint}")
        if items:
            for item in items.get(coll_key, []):
                api("DELETE", f"{endpoint}/{item['id']}?version={item['revision']['version']}")
    pg = api("GET", f"process-groups/{pg_id}")
    if pg:
        api("DELETE", f"process-groups/{pg_id}?version={pg['revision']['version']}")


# ── SPARQL Queries (per-era cross-graph join) ──────────────────

def era_query(era_name, date_min, date_max, golden_source):
    """Generate a cross-graph SPARQL SELECT for one era.

    Golden source drives the query; supplementary sources join by filename (OPTIONAL).
    Date filtering on the golden source ensures era scoping.
    """
    # All three source URIs
    sources = {
        "apple": "urn:gathering:photos/source/apple",
        "takeout": "urn:gathering:photos/source/takeout",
        "iphone": "urn:gathering:photos/source/iphone"
    }
    golden_uri = sources[golden_source]
    supp_sources = {k: v for k, v in sources.items() if k != golden_source}

    # Fields per source (based on what's actually in Fuseki)
    rich_fields = "jb:photoFilename jb:dateTaken jb:latitude jb:longitude jb:imageWidth jb:imageHeight jb:fileSize jb:mediaSubtype"
    sparse_fields = "jb:photoFilename jb:dateTaken jb:latitude jb:longitude"

    # Build the query
    q = f"""PREFIX jb: <https://jeffbridwell.com/ontology#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>

SELECT ?filename
  ?g_date ?g_lat ?g_lon ?g_width ?g_height ?g_fileSize ?g_mediaSubtype"""

    # Add supplementary source variables
    for prefix in supp_sources:
        p = prefix[0]  # a, t, or i
        q += f"\n  ?{p}_date ?{p}_lat ?{p}_lon"
        if prefix != "takeout":  # Takeout has ceiling on dimensions/fileSize
            q += f" ?{p}_width ?{p}_height ?{p}_fileSize ?{p}_mediaSubtype"

    q += f"""
WHERE {{
  # Golden source: {golden_source} (era: {era_name})
  GRAPH <{golden_uri}> {{
    ?gPhoto a jb:SourcePhoto ;
            jb:photoFilename ?filename ;
            jb:dateTaken ?g_date .
    OPTIONAL {{ ?gPhoto jb:latitude ?g_lat }}
    OPTIONAL {{ ?gPhoto jb:longitude ?g_lon }}
    OPTIONAL {{ ?gPhoto jb:imageWidth ?g_width }}
    OPTIONAL {{ ?gPhoto jb:imageHeight ?g_height }}
    OPTIONAL {{ ?gPhoto jb:fileSize ?g_fileSize }}
    OPTIONAL {{ ?gPhoto jb:mediaSubtype ?g_mediaSubtype }}
    FILTER(?g_date >= "{date_min}"^^xsd:dateTime && ?g_date < "{date_max}"^^xsd:dateTime)
  }}"""

    # Add supplementary source joins (OPTIONAL, by filename)
    for prefix, uri in supp_sources.items():
        p = prefix[0]
        q += f"""
  OPTIONAL {{
    GRAPH <{uri}> {{
      ?{p}Photo a jb:SourcePhoto ;
              jb:photoFilename ?filename ;
              jb:dateTaken ?{p}_date .
      OPTIONAL {{ ?{p}Photo jb:latitude ?{p}_lat }}
      OPTIONAL {{ ?{p}Photo jb:longitude ?{p}_lon }}"""
        if prefix != "takeout":
            q += f"""
      OPTIONAL {{ ?{p}Photo jb:imageWidth ?{p}_width }}
      OPTIONAL {{ ?{p}Photo jb:imageHeight ?{p}_height }}
      OPTIONAL {{ ?{p}Photo jb:fileSize ?{p}_fileSize }}
      OPTIONAL {{ ?{p}Photo jb:mediaSubtype ?{p}_mediaSubtype }}"""
        q += """
    }
  }"""

    q += "\n}"
    return q


# Era definitions from the merge spec
ERAS = [
    ("pre-digital",    "0001-01-01T00:00:00Z", "2006-01-01T00:00:00Z", "apple"),
    ("camera-era",     "2006-01-01T00:00:00Z", "2013-01-01T00:00:00Z", "apple"),
    ("iphone-primary", "2013-01-01T00:00:00Z", "2020-01-01T00:00:00Z", "apple"),
    ("modern",         "2020-01-01T00:00:00Z", "2099-01-01T00:00:00Z", "iphone"),
]

# Supplementary-only query: records in supplementary sources with no golden source match
SUPP_ONLY_QUERY = """PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT ?filename ?source ?date ?lat ?lon ?width ?height ?fileSize ?mediaSubtype
WHERE {
  {
    GRAPH <urn:gathering:photos/source/takeout> {
      ?s a jb:SourcePhoto ;
         jb:photoFilename ?filename ;
         jb:dateTaken ?date .
      OPTIONAL { ?s jb:latitude ?lat }
      OPTIONAL { ?s jb:longitude ?lon }
      BIND("takeout" AS ?source)
    }
    FILTER NOT EXISTS {
      GRAPH <urn:gathering:photos/source/apple> { ?a jb:photoFilename ?filename }
    }
    FILTER NOT EXISTS {
      GRAPH <urn:gathering:photos/source/iphone> { ?i jb:photoFilename ?filename }
    }
  } UNION {
    GRAPH <urn:gathering:photos/source/iphone> {
      ?s a jb:SourcePhoto ;
         jb:photoFilename ?filename ;
         jb:dateTaken ?date .
      OPTIONAL { ?s jb:latitude ?lat }
      OPTIONAL { ?s jb:longitude ?lon }
      OPTIONAL { ?s jb:imageWidth ?width }
      OPTIONAL { ?s jb:imageHeight ?height }
      OPTIONAL { ?s jb:fileSize ?fileSize }
      OPTIONAL { ?s jb:mediaSubtype ?mediaSubtype }
      BIND("iphone" AS ?source)
    }
    FILTER NOT EXISTS {
      GRAPH <urn:gathering:photos/source/apple> { ?a jb:photoFilename ?filename }
    }
  }
}"""


# ── Main ────────────────────────────────────────────────────

if __name__ == "__main__":
    print("=== Photos Canonical Rebuild — NiFi Flow Builder v2 ===")
    print("Architecture: SPARQL cross-graph join per era\n")

    TOKEN = get_token()
    print(f"Authenticated. Token: {TOKEN[:20]}...")

    root = api("GET", "process-groups/root")
    ROOT_PG = root["id"]

    # Clean up existing
    children = api("GET", f"process-groups/{ROOT_PG}/process-groups")
    for pg in children.get("processGroups", []):
        if pg["component"]["name"] == "photos-canonical-rebuild":
            print(f"\nCleaning up: {pg['id']}...")
            delete_pg_recursive(pg["id"])

    # ── Create top-level PG ──
    print("\n[1/5] Creating photos-canonical-rebuild...")
    MAIN_PG = create_pg(ROOT_PG, "photos-canonical-rebuild", 100, 100)

    # ── Stage 1: Source Extraction (per-era SPARQL) ──
    print("\n[2/5] Building source-extraction (per-era SPARQL queries)...")
    SRC_PG = create_pg(MAIN_PG, "source-extraction", 0, 0)

    # Trigger processor
    trigger = create_processor(SRC_PG, "org.apache.nifi.processors.standard.GenerateFlowFile",
        "Pipeline Trigger", 300, 0, props={"Text": "trigger", "Batch Size": "1"},
        scheduling={"schedulingStrategy": "TIMER_DRIVEN", "schedulingPeriod": "1 day"})

    # Per-era SPARQL extraction processors
    era_procs = {}
    src_out = create_port(SRC_PG, "era-records", "output-ports", 300, 800)

    for i, (era_name, date_min, date_max, golden) in enumerate(ERAS):
        sparql = era_query(era_name, date_min, date_max, golden)
        encoded = urllib.parse.quote(sparql)
        url = f"{FUSEKI_URL}?query={encoded}&output=json"

        proc = create_processor(SRC_PG, "org.apache.nifi.processors.standard.InvokeHTTP",
            f"Era: {era_name} ({golden} golden)", 100 + i * 250, 200, props={
                "HTTP Method": "GET",
                "HTTP URL": url
            })
        era_procs[era_name] = proc

        # Trigger → this processor
        connect(SRC_PG, trigger, proc, ["success"])
        # Response → output port
        connect(SRC_PG, proc, src_out, ["Response"], dst_type="OUTPUT_PORT")
        auto_terminate(proc, ["Failure", "No Retry", "Original"])

    # Supplementary-only processor
    supp_url = f"{FUSEKI_URL}?query={urllib.parse.quote(SUPP_ONLY_QUERY)}&output=json"
    supp_proc = create_processor(SRC_PG, "org.apache.nifi.processors.standard.InvokeHTTP",
        "Unmatched Supplementary Records", 1100, 200, props={
            "HTTP Method": "GET",
            "HTTP URL": supp_url
        })
    connect(SRC_PG, trigger, supp_proc, ["success"])
    connect(SRC_PG, supp_proc, src_out, ["Response"], dst_type="OUTPUT_PORT")
    auto_terminate(supp_proc, ["Failure", "No Retry", "Original"])

    # ── Stage 2: Field Merge (Jolt per era) ──
    print("\n[3/5] Building field-merge (Jolt transforms)...")
    MERGE_PG = create_pg(MAIN_PG, "field-merge", 0, 400)
    merge_in = create_port(MERGE_PG, "era-records-in", "input-ports", 300, 0)
    merge_out = create_port(MERGE_PG, "merged-records", "output-ports", 300, 1000)

    # Split SPARQL results into individual records
    split = create_processor(MERGE_PG, "org.apache.nifi.processors.standard.SplitJson",
        "Split SPARQL Results", 300, 150, props={
            "JsonPath Expression": "$.results.bindings[*]"
        })
    connect(MERGE_PG, merge_in, split, [""], src_type="INPUT_PORT")

    # Extract era from the golden date for routing
    extract = create_processor(MERGE_PG, "org.apache.nifi.processors.standard.EvaluateJsonPath",
        "Extract Era + Filename", 300, 300, props={
            "Destination": "flowfile-attribute",
            "record.filename": "$.filename.value",
            "golden.date": "$.g_date.value"
        })
    connect(MERGE_PG, split, extract, ["split"])
    auto_terminate(split, ["failure", "original"])

    # Era router
    route_era = create_processor(MERGE_PG, "org.apache.nifi.processors.standard.RouteOnAttribute",
        "Route by Era", 300, 450, props={
            "pre-digital": "${golden.date:isEmpty():not():and(${golden.date:compareTo('2006-01-01'):lt(0)})}",
            "camera-era": "${golden.date:isEmpty():not():and(${golden.date:compareTo('2006-01-01'):ge(0)}):and(${golden.date:compareTo('2013-01-01'):lt(0)})}",
            "iphone-primary": "${golden.date:isEmpty():not():and(${golden.date:compareTo('2013-01-01'):ge(0)}):and(${golden.date:compareTo('2020-01-01'):lt(0)})}",
            "modern": "${golden.date:isEmpty():not():and(${golden.date:compareTo('2020-01-01'):ge(0)})}"
        })
    connect(MERGE_PG, extract, route_era, ["matched"])
    auto_terminate(extract, ["unmatched"])

    # Jolt transforms per era — placeholder specs, will be refined with real data
    jolt_procs = {}
    for i, (era_name, _, _, golden) in enumerate(ERAS):
        jolt = create_processor(MERGE_PG, "org.apache.nifi.processors.jolt.JoltTransformJSON",
            f"Merge: {era_name}", 100 + i * 250, 650, props={
                "Jolt Specification": json.dumps([{"operation": "shift", "spec": {"@": ""}}])
            })
        jolt_procs[era_name] = jolt
        connect(MERGE_PG, route_era, jolt, [era_name])
        connect(MERGE_PG, jolt, merge_out, ["success"], dst_type="OUTPUT_PORT")
        auto_terminate(jolt, ["failure"])

    # Unmatched → dead letter
    auto_terminate(route_era, ["unmatched"])

    # ── Stage 3: Validation ──
    print("\n[4/5] Building validation...")
    VAL_PG = create_pg(MAIN_PG, "validation", 0, 800)
    val_in = create_port(VAL_PG, "merged-in", "input-ports", 300, 0)
    val_out = create_port(VAL_PG, "validated", "output-ports", 300, 500)
    val_dead = create_port(VAL_PG, "rejected", "output-ports", 600, 500)

    # Provenance attributes
    prov = create_processor(VAL_PG, "org.apache.nifi.processors.attributes.UpdateAttribute",
        "Attach Provenance", 300, 200, props={
            "merge.timestamp": "${now():format('yyyy-MM-dd\\'T\\'HH:mm:ss\\'Z\\'')}",
        })
    connect(VAL_PG, val_in, prov, [""], src_type="INPUT_PORT")
    connect(VAL_PG, prov, val_out, ["success"], dst_type="OUTPUT_PORT")

    # ── Stage 4: Output ──
    print("\n[5/5] Building output + dead-letter...")
    OUT_PG = create_pg(MAIN_PG, "output", 0, 1200)
    out_in = create_port(OUT_PG, "validated-in", "input-ports", 300, 0)

    convert = create_processor(OUT_PG, "org.apache.nifi.processors.standard.ReplaceText",
        "Format as N-Triples", 300, 200, props={
            "Search Value": ".*",
            "Replacement Value": "# Placeholder — script processor will generate TTL from merged JSON",
            "Evaluation Mode": "Entire text"
        })
    fuseki_write = create_processor(OUT_PG, "org.apache.nifi.processors.standard.InvokeHTTP",
        "Write to Fuseki", 300, 400, props={
            "HTTP Method": "PUT",
            "HTTP URL": "http://192.168.86.36:3030/pods/data?graph=urn:jb:photos/canonical/",
            "Request Content-Type": "application/n-triples"
        })
    connect(OUT_PG, out_in, convert, [""], src_type="INPUT_PORT")
    connect(OUT_PG, convert, fuseki_write, ["success"])
    auto_terminate(convert, ["failure"])
    auto_terminate(fuseki_write, ["Failure", "No Retry", "Original", "Response"])

    # Dead letter
    DEAD_PG = create_pg(MAIN_PG, "dead-letter", 400, 800)
    dead_in = create_port(DEAD_PG, "rejected-in", "input-ports", 300, 0)
    dead_write = create_processor(DEAD_PG, "org.apache.nifi.processors.standard.PutFile",
        "Write Dead Letter", 300, 200, props={
            "Directory": "/Users/jeffbridwell/data/nifi/dead-letter/photos",
            "Conflict Resolution Strategy": "replace"
        })
    connect(DEAD_PG, dead_in, dead_write, [""], src_type="INPUT_PORT")
    auto_terminate(dead_write, ["failure", "success"])

    # ── Inter-group connections ──
    print("\n[wiring inter-group]")
    connect(MAIN_PG, src_out, merge_in, [""], "OUTPUT_PORT", "INPUT_PORT", SRC_PG, MERGE_PG)
    connect(MAIN_PG, merge_out, val_in, [""], "OUTPUT_PORT", "INPUT_PORT", MERGE_PG, VAL_PG)
    connect(MAIN_PG, val_out, out_in, [""], "OUTPUT_PORT", "INPUT_PORT", VAL_PG, OUT_PG)
    connect(MAIN_PG, val_dead, dead_in, [""], "OUTPUT_PORT", "INPUT_PORT", VAL_PG, DEAD_PG)

    print(f"\n=== Flow v2 complete ===")
    print(f"Main PG: {MAIN_PG}")
    print(f"Stages: source-extraction → field-merge → validation → output + dead-letter")
    print(f"5 SPARQL queries (4 eras + 1 supplementary-only)")
    print(f"4 Jolt merge processors (one per era)")
    print(f"\nNext: refine Jolt specs per era, add N-Triples conversion script, test with batch of 100")
