#!/usr/bin/env python3
"""
Full canonical photo rebuild — all 4 eras + supplementary-only.
Queries Fuseki, merges per era-scoped spec, writes N-Triples.

Card: #1644 | Emits progress to stderr per era.
"""

import json
import sys
import urllib.request
import urllib.parse
import time

FUSEKI = "http://localhost:3030/pods/query"
OUTPUT = "/tmp/canonical-photos-rebuild.nt"

# Import merge functions
sys.path.insert(0, "scripts/nifi")
with open("scripts/nifi/photos-merge-cli.py") as f:
    code = f.read().split("if __name__")[0]
exec(code)


def sparql(query):
    """Execute SPARQL query against Fuseki, return bindings."""
    data = urllib.parse.urlencode({"query": query, "output": "json"}).encode()
    req = urllib.request.Request(FUSEKI, data=data, method="POST")
    req.add_header("Accept", "application/sparql-results+json")
    resp = urllib.request.urlopen(req, timeout=300)
    return json.loads(resp.read())["results"]["bindings"]


def era_query(era_name, date_min, date_max, golden):
    """Build cross-graph join SPARQL for one era."""
    if golden == "apple":
        golden_graph = "urn:gathering:photos/source/apple"
        supp_blocks = """
  OPTIONAL {
    GRAPH <urn:gathering:photos/source/iphone> {
      ?iPhoto a jb:SourcePhoto ; jb:photoFilename ?filename ; jb:dateTaken ?i_date .
      OPTIONAL { ?iPhoto jb:latitude ?i_lat } OPTIONAL { ?iPhoto jb:longitude ?i_lon }
      OPTIONAL { ?iPhoto jb:imageWidth ?i_width } OPTIONAL { ?iPhoto jb:imageHeight ?i_height }
      OPTIONAL { ?iPhoto jb:fileSize ?i_fileSize } OPTIONAL { ?iPhoto jb:mediaSubtype ?i_mediaSubtype }
    }
  }
  OPTIONAL {
    GRAPH <urn:gathering:photos/source/takeout> {
      ?tPhoto a jb:SourcePhoto ; jb:photoFilename ?filename ; jb:dateTaken ?t_date .
      OPTIONAL { ?tPhoto jb:latitude ?t_lat } OPTIONAL { ?tPhoto jb:longitude ?t_lon }
    }
  }"""
        supp_vars = "?i_date ?i_lat ?i_lon ?i_width ?i_height ?i_fileSize ?i_mediaSubtype ?t_date ?t_lat ?t_lon"
    else:  # modern: iphone golden
        golden_graph = "urn:gathering:photos/source/iphone"
        supp_blocks = """
  OPTIONAL {
    GRAPH <urn:gathering:photos/source/apple> {
      ?aPhoto a jb:SourcePhoto ; jb:photoFilename ?filename ; jb:dateTaken ?a_date .
      OPTIONAL { ?aPhoto jb:latitude ?a_lat } OPTIONAL { ?aPhoto jb:longitude ?a_lon }
      OPTIONAL { ?aPhoto jb:imageWidth ?a_width } OPTIONAL { ?aPhoto jb:imageHeight ?a_height }
      OPTIONAL { ?aPhoto jb:fileSize ?a_fileSize } OPTIONAL { ?aPhoto jb:mediaSubtype ?a_mediaSubtype }
    }
  }
  OPTIONAL {
    GRAPH <urn:gathering:photos/source/takeout> {
      ?tPhoto a jb:SourcePhoto ; jb:photoFilename ?filename ; jb:dateTaken ?t_date .
      OPTIONAL { ?tPhoto jb:latitude ?t_lat } OPTIONAL { ?tPhoto jb:longitude ?t_lon }
    }
  }"""
        supp_vars = "?a_date ?a_lat ?a_lon ?a_width ?a_height ?a_fileSize ?a_mediaSubtype ?t_date ?t_lat ?t_lon"

    return f"""PREFIX jb: <https://jeffbridwell.com/ontology#>
PREFIX xsd: <http://www.w3.org/2001/XMLSchema#>
SELECT DISTINCT ?filename ?g_date ?g_lat ?g_lon ?g_width ?g_height ?g_fileSize ?g_mediaSubtype {supp_vars}
WHERE {{
  GRAPH <{golden_graph}> {{
    ?gPhoto a jb:SourcePhoto ; jb:photoFilename ?filename ; jb:dateTaken ?g_date .
    OPTIONAL {{ ?gPhoto jb:latitude ?g_lat }} OPTIONAL {{ ?gPhoto jb:longitude ?g_lon }}
    OPTIONAL {{ ?gPhoto jb:imageWidth ?g_width }} OPTIONAL {{ ?gPhoto jb:imageHeight ?g_height }}
    OPTIONAL {{ ?gPhoto jb:fileSize ?g_fileSize }} OPTIONAL {{ ?gPhoto jb:mediaSubtype ?g_mediaSubtype }}
    FILTER(?g_date >= "{date_min}T00:00:00Z"^^xsd:dateTime && ?g_date < "{date_max}T00:00:00Z"^^xsd:dateTime)
  }}
  {supp_blocks}
}}"""


ERAS = [
    ("pre-digital",    "0001-01-01", "2006-01-01", "apple"),
    ("camera-era",     "2006-01-01", "2013-01-01", "apple"),
    ("iphone-primary", "2013-01-01", "2020-01-01", "apple"),
    ("modern",         "2020-01-01", "2099-01-01", "iphone"),
]

SUPP_QUERY = """PREFIX jb: <https://jeffbridwell.com/ontology#>
SELECT DISTINCT ?filename ?g_date ?g_lat ?g_lon
WHERE {
  {
    GRAPH <urn:gathering:photos/source/takeout> {
      ?s a jb:SourcePhoto ; jb:photoFilename ?filename ; jb:dateTaken ?g_date .
      OPTIONAL { ?s jb:latitude ?g_lat } OPTIONAL { ?s jb:longitude ?g_lon }
    }
    FILTER NOT EXISTS {
      GRAPH <urn:gathering:photos/source/apple> { ?a jb:photoFilename ?filename }
    }
    FILTER NOT EXISTS {
      GRAPH <urn:gathering:photos/source/iphone> { ?i jb:photoFilename ?filename }
    }
  }
}"""


if __name__ == "__main__":
    print("=== Canonical Photo Rebuild ===", file=sys.stderr)
    total_records = 0
    total_triples = 0

    with open(OUTPUT, "w") as out:
        for era_name, date_min, date_max, golden in ERAS:
            t0 = time.time()
            print(f"[{era_name}] Querying Fuseki ({golden} golden)...", file=sys.stderr, flush=True)

            query = era_query(era_name, date_min, date_max, golden)
            bindings = sparql(query)

            elapsed_q = time.time() - t0
            print(f"[{era_name}] {len(bindings)} rows in {elapsed_q:.1f}s, deduping + merging...", file=sys.stderr, flush=True)

            seen = set()
            era_records = 0
            era_triples = 0

            for record in bindings:
                fn = record.get("filename", {}).get("value", "")
                dt = record.get("g_date", {}).get("value", "")[:10]
                dedup_key = f"{fn}|{dt}"
                if dedup_key in seen:
                    continue
                seen.add(dedup_key)

                triples = merge_record(record)
                for t in triples:
                    if not t.startswith("#"):
                        out.write(t + "\n")
                        era_triples += 1
                era_records += 1

            elapsed_m = time.time() - t0
            total_records += era_records
            total_triples += era_triples
            print(f"[{era_name}] {era_records} records → {era_triples} triples in {elapsed_m:.1f}s (running total: {total_records} records, {total_triples} triples)", file=sys.stderr, flush=True)

        # Supplementary-only records
        print(f"[supplementary] Querying...", file=sys.stderr, flush=True)
        t0 = time.time()
        bindings = sparql(SUPP_QUERY)
        print(f"[supplementary] {len(bindings)} rows in {time.time()-t0:.1f}s, merging...", file=sys.stderr, flush=True)

        seen_supp = set()
        supp_records = 0
        supp_triples = 0
        for record in bindings:
            fn = record.get("filename", {}).get("value", "")
            dt = record.get("g_date", {}).get("value", "")[:10]
            dedup_key = f"{fn}|{dt}"
            if dedup_key in seen_supp:
                continue
            seen_supp.add(dedup_key)
            triples = merge_record(record)
            for t in triples:
                if not t.startswith("#"):
                    out.write(t + "\n")
                    supp_triples += 1
            supp_records += 1

        total_records += supp_records
        total_triples += supp_triples
        print(f"[supplementary] {supp_records} records → {supp_triples} triples", file=sys.stderr, flush=True)

    print(f"\n=== COMPLETE ===", file=sys.stderr)
    print(f"Total: {total_records} canonical records, {total_triples} triples", file=sys.stderr)
    print(f"Output: {OUTPUT}", file=sys.stderr)
