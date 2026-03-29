#!/usr/bin/env python3
"""
webMethods Package Schema Extractor

Reads node.ndf files from webMethods packages, extracts doc type schemas,
resolves references, and produces an HTML inventory.

Card: #1641 — Demo for Deb Majumdar, Allu Babu Nukala, Kathy Kysar
"""

import xml.etree.ElementTree as ET
import os
import sys
import json
from collections import defaultdict
from pathlib import Path


def parse_node_ndf(filepath):
    """Parse a node.ndf file and extract the doc type schema."""
    try:
        tree = ET.parse(filepath)
        root = tree.getroot()
    except ET.ParseError:
        return None

    # Find the root record
    record = root.find("record")
    if record is None:
        return None

    node_type = _val(record, "node_type")
    ns_name = _val(record, "node_nsName")
    pkg = _val(record, "node_pkg")
    field_type = _val(record, "field_type")

    # Extract fields recursively
    fields = _extract_fields(record)

    return {
        "nsName": ns_name,
        "package": pkg,
        "nodeType": node_type,
        "fieldType": field_type,
        "fields": fields,
        "filePath": filepath,
    }


def _val(element, name):
    """Get a value element's text by name."""
    for v in element.findall("value"):
        if v.get("name") == name:
            return v.text
    return None


def _extract_fields(record):
    """Recursively extract fields from a record's rec_fields array."""
    fields = []
    for arr in record.findall("array"):
        if arr.get("name") == "rec_fields":
            for field_rec in arr.findall("record"):
                field = {
                    "name": _val(field_rec, "field_name"),
                    "type": _val(field_rec, "field_type"),
                    "dim": _val(field_rec, "field_dim"),
                    "optional": _val(field_rec, "field_opt") == "true",
                    "nillable": _val(field_rec, "nillable") == "true",
                    "ref": _val(field_rec, "rec_ref"),
                    "children": _extract_fields(field_rec),
                }
                fields.append(field)
    return fields


def parse_service_ndf(filepath):
    """Parse a flow service's node.ndf for service metadata (type, signature, etc.)."""
    try:
        tree = ET.parse(filepath)
        root = tree.getroot()
    except ET.ParseError:
        return None

    # Top-level values (not nested in a record)
    svc_type = None
    svc_subtype = None
    stateless = None
    ns_name = None

    for v in root.iter("value"):
        name = v.get("name")
        if name == "svc_type":
            svc_type = v.text
        elif name == "svc_subtype":
            svc_subtype = v.text
        elif name == "stateless":
            stateless = v.text
        elif name == "node_nsName":
            ns_name = v.text

    # Extract input/output signatures from svc_sig
    sig_in_fields = []
    sig_out_fields = []
    for rec in root.iter("record"):
        rec_name = rec.get("name")
        if rec_name == "sig_in":
            sig_in_fields = _extract_fields(rec)
        elif rec_name == "sig_out":
            sig_out_fields = _extract_fields(rec)

    return {
        "svcType": svc_type,
        "svcSubtype": svc_subtype,
        "stateless": stateless == "yes",
        "nsName": ns_name,
        "sigIn": sig_in_fields,
        "sigOut": sig_out_fields,
    }


def parse_flow_xml(filepath):
    """Parse a flow.xml file and extract the service call graph."""
    try:
        tree = ET.parse(filepath)
        root = tree.getroot()
    except ET.ParseError:
        return None

    # Find FLOW element
    flow = root.find(".//FLOW")
    if flow is None:
        # Try direct children
        flow = root

    steps = _extract_flow_steps(flow)
    invokes = [s for s in steps if s["type"] == "INVOKE"]

    # Also parse the sibling node.ndf for service metadata
    ndf_path = os.path.join(os.path.dirname(filepath), "node.ndf")
    svc_meta = parse_service_ndf(ndf_path) if os.path.exists(ndf_path) else None

    return {
        "steps": steps,
        "invokes": invokes,
        "stepCount": len(steps),
        "invokeCount": len(invokes),
        "svcMeta": svc_meta,
    }


def _extract_flow_steps(element, depth=0):
    """Recursively extract flow steps."""
    steps = []
    for child in element:
        tag = child.tag
        if tag in ("INVOKE", "MAP", "BRANCH", "LOOP", "REPEAT", "SEQUENCE", "EXIT"):
            step = {
                "type": tag,
                "service": child.get("SERVICE", ""),
                "depth": depth,
                "children": _extract_flow_steps(child, depth + 1),
            }
            if tag == "MAP":
                # Extract MAP steps (transformations)
                mappings = []
                for mapset in child.findall(".//MAPSET"):
                    for mapcopy in mapset.findall("MAPCOPY"):
                        mappings.append({
                            "from": mapcopy.get("FROM", ""),
                            "to": mapcopy.get("TO", ""),
                        })
                step["mappings"] = mappings
            steps.append(step)
    return steps


def scan_packages(base_path):
    """Scan all packages and extract schemas + flows."""
    packages = {}
    base = Path(base_path)

    for pkg_dir in sorted(base.iterdir()):
        if not pkg_dir.is_dir():
            continue

        pkg_name = pkg_dir.name
        ns_dir = pkg_dir / "ns"
        if not ns_dir.exists():
            continue

        pkg = {
            "name": pkg_name,
            "docTypes": [],
            "flowServices": [],
            "references": defaultdict(list),
        }

        # Find all node.ndf files — separate doc types from services
        for ndf in sorted(ns_dir.rglob("node.ndf")):
            # Check if this is a service (has svc_type) or a doc type
            svc_meta = parse_service_ndf(str(ndf))
            if svc_meta and svc_meta.get("svcType"):
                # It's a service — check for flow.xml sibling
                flow_path = ndf.parent / "flow.xml"
                if flow_path.exists():
                    flow = parse_flow_xml(str(flow_path))
                else:
                    # AdapterService or java service — no flow.xml
                    flow = {"steps": [], "invokes": [], "stepCount": 0, "invokeCount": 0, "svcMeta": svc_meta}
                if flow:
                    flow["filePath"] = str(ndf)
                    flow["svcMeta"] = svc_meta
                    rel = ndf.relative_to(ns_dir)
                    flow["serviceName"] = str(rel.parent).replace("/", ".")
                    pkg["flowServices"].append(flow)
            else:
                # Doc type
                schema = parse_node_ndf(str(ndf))
                if schema:
                    pkg["docTypes"].append(schema)
                    _collect_refs(schema["fields"], schema["nsName"], pkg["references"])

        packages[pkg_name] = pkg

    return packages


def _collect_refs(fields, source_ns, refs):
    """Collect all type references from fields."""
    for f in fields:
        if f.get("ref"):
            refs[f["ref"]].append(source_ns)
        if f.get("children"):
            _collect_refs(f["children"], source_ns, refs)


def generate_html(packages, output_path):
    """Generate an HTML report of all schemas."""
    total_docs = sum(len(p["docTypes"]) for p in packages.values())
    total_flows = sum(len(p["flowServices"]) for p in packages.values())
    total_fields = sum(
        _count_fields(dt["fields"])
        for p in packages.values()
        for dt in p["docTypes"]
    )

    # Collect all references
    all_refs = defaultdict(list)
    for p in packages.values():
        for k, v in p["references"].items():
            all_refs[k].extend(v)

    # Identify canonical doc types (in Canonicals package)
    canonical_types = []
    for p in packages.values():
        if "Canonical" in p["name"]:
            canonical_types.extend(p["docTypes"])

    html = f"""<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<title>webMethods Schema Inventory — J&J Integration Platform</title>
<style>
  body {{ font-family: -apple-system, sans-serif; max-width: 1200px; margin: 2rem auto; padding: 0 1.5rem; color: #222; background: #fafafa; line-height: 1.5; }}
  h1 {{ font-size: 1.8rem; border-bottom: 3px solid #333; padding-bottom: .5rem; }}
  h2 {{ font-size: 1.3rem; margin-top: 2rem; border-bottom: 1px solid #ccc; padding-bottom: .3rem; color: #1a5276; }}
  h3 {{ font-size: 1.05rem; margin-top: 1.2rem; color: #555; }}
  .stats {{ display: flex; gap: 2rem; margin: 1.5rem 0; }}
  .stat {{ background: #fff; border: 1px solid #ddd; border-radius: 8px; padding: 1rem 1.5rem; text-align: center; }}
  .stat .number {{ font-size: 2rem; font-weight: 700; color: #1a5276; }}
  .stat .label {{ font-size: .85rem; color: #888; }}
  table {{ border-collapse: collapse; width: 100%; margin: 1rem 0; font-size: .85rem; }}
  th, td {{ border: 1px solid #ddd; padding: .4rem .6rem; text-align: left; }}
  th {{ background: #f0f0f0; font-weight: 600; }}
  .ref {{ color: #2980b9; font-style: italic; }}
  .canonical {{ background: #d5f5e3; font-weight: 600; }}
  .field-tree {{ margin-left: 1.5rem; border-left: 2px solid #ddd; padding-left: .75rem; }}
  .field {{ margin: .2rem 0; font-size: .85rem; }}
  .field .name {{ font-weight: 600; }}
  .field .type {{ color: #888; font-size: .8rem; }}
  .field .type.ref {{ color: #2980b9; }}
  .pkg-section {{ background: #fff; border: 1px solid #ddd; border-radius: 8px; padding: 1rem 1.5rem; margin: 1rem 0; }}
  .expand {{ cursor: pointer; color: #2980b9; font-size: .85rem; }}
  details summary {{ cursor: pointer; font-weight: 600; }}
  .flow-step {{ margin-left: 1rem; font-size: .85rem; }}
  .flow-invoke {{ color: #c0392b; font-weight: 600; }}
  .flow-map {{ color: #27ae60; }}
  .flow-branch {{ color: #8e44ad; }}
  footer {{ margin-top: 3rem; border-top: 1px solid #ccc; padding-top: 1rem; font-size: .8rem; color: #888; }}
</style>
</head>
<body>

<h1>webMethods Schema Inventory</h1>
<p><strong>Source:</strong> J&amp;J Integration Platform (MU118) &nbsp; <strong>Packages:</strong> {len(packages)} &nbsp; <strong>Extracted by:</strong> Kade (Chorus Engineer)</p>

<div class="stats">
  <div class="stat"><div class="number">{len(packages)}</div><div class="label">Packages</div></div>
  <div class="stat"><div class="number">{total_docs:,}</div><div class="label">Doc Types</div></div>
  <div class="stat"><div class="number">{total_flows:,}</div><div class="label">Flow Services</div></div>
  <div class="stat"><div class="number">{total_fields:,}</div><div class="label">Total Fields</div></div>
  <div class="stat"><div class="number">{len(all_refs):,}</div><div class="label">Type References</div></div>
</div>

<details>
<summary><h2 style="display:inline">Canonical Doc Types ({len(canonical_types)})</h2></summary>
<p>These are the canonical message types — the integration contracts. Every flow service maps to/from these.</p>
<table>
<tr><th>Canonical Type</th><th>Top-Level Fields</th><th>Total Fields</th><th>References</th><th>Referenced By</th></tr>
"""

    for dt in canonical_types:
        top_fields = len(dt["fields"])
        total = _count_fields(dt["fields"])
        refs_out = _count_refs(dt["fields"])
        refs_in = len(all_refs.get(dt["nsName"], []))
        html += f'<tr class="canonical"><td>{dt["nsName"]}</td><td>{top_fields}</td><td>{total}</td><td>{refs_out}</td><td>{refs_in}</td></tr>\n'

    html += "</table>\n</details>\n"

    # Service type summary
    all_svcs = [f for p in packages.values() for f in p["flowServices"]]
    svc_types = defaultdict(int)
    adapter_svcs = []
    for f in all_svcs:
        meta = f.get("svcMeta") or {}
        st = meta.get("svcType", "flow")
        svc_types[st] += 1
        if st == "AdapterService":
            adapter_svcs.append(f)

    html += "<details>\n<summary><h2 style='display:inline'>Service Endpoint Summary</h2></summary>\n"
    html += "<table><tr><th>Service Type</th><th>Count</th><th>Description</th></tr>\n"
    type_desc = {
        "flow": "Integration logic — maps, transforms, orchestration",
        "AdapterService": "External system connector — JDBC, JMS, file, HTTP",
        "java": "Native Java service",
    }
    for st, count in sorted(svc_types.items(), key=lambda x: -x[1]):
        desc = type_desc.get(st, "")
        html += f"<tr><td><strong>{st}</strong></td><td>{count}</td><td>{desc}</td></tr>\n"
    html += "</table>\n"

    if adapter_svcs:
        html += "<h3>Adapter Services (External Endpoints)</h3>\n"
        html += "<p>These connect to external systems — databases, queues, files. Each represents a system boundary.</p>\n"
        html += "<table><tr><th>Service</th><th>Package</th><th>Input Fields</th><th>Output Fields</th></tr>\n"
        for f in adapter_svcs:
            meta = f.get("svcMeta") or {}
            pkg_name = f["filePath"].split("/extracted/")[1].split("/")[0] if "/extracted/" in f["filePath"] else "?"
            in_c = _count_fields(meta.get("sigIn", []))
            out_c = _count_fields(meta.get("sigOut", []))
            html += f'<tr><td>{f["serviceName"]}</td><td>{pkg_name}</td><td>{in_c}</td><td>{out_c}</td></tr>\n'
        html += "</table>\n"

    html += "</details>\n"

    # Cross-package call graph
    html += "<details>\n<summary><h2 style='display:inline'>Cross-Package Call Graph</h2></summary>\n"
    html += "<p>Which packages call services in other packages — reveals integration dependencies.</p>\n"
    cross_calls = defaultdict(lambda: defaultdict(set))
    for pkg_name, pkg in packages.items():
        for flow in pkg["flowServices"]:
            for inv in flow["invokes"]:
                svc = inv.get("service", "")
                if "." in svc:
                    target_pkg = svc.split(".")[0]
                    if target_pkg != pkg_name:
                        cross_calls[pkg_name][target_pkg].add(svc)

    if cross_calls:
        html += "<table><tr><th>Calling Package</th><th>→ Target Package</th><th>Services Called</th></tr>\n"
        for caller in sorted(cross_calls):
            for target in sorted(cross_calls[caller]):
                svcs = sorted(cross_calls[caller][target])
                html += f'<tr><td>{caller}</td><td>{target}</td><td style="font-size:.75rem">{", ".join(svcs[:5])}{" ..." if len(svcs)>5 else ""} ({len(svcs)})</td></tr>\n'
        html += "</table>\n"
    else:
        html += "<p><em>No cross-package calls detected.</em></p>\n"

    html += "</details>\n"

    # Package-by-package breakdown
    html += "<details>\n<summary><h2 style='display:inline'>Package Inventory ({len(packages)} packages)</h2></summary>\n"
    for pkg_name, pkg in sorted(packages.items()):
        doc_count = len(pkg["docTypes"])
        flow_count = len(pkg["flowServices"])
        html += f"""
<div class="pkg-section">
<details>
<summary>{pkg_name} — {doc_count} doc types, {flow_count} flow services</summary>
"""
        if pkg["docTypes"]:
            html += "<h3>Doc Types</h3>\n<table><tr><th>Name</th><th>Type</th><th>Fields</th><th>References</th></tr>\n"
            for dt in pkg["docTypes"]:
                total = _count_fields(dt["fields"])
                refs = _count_refs(dt["fields"])
                html += f'<tr><td>{dt["nsName"]}</td><td>{dt["fieldType"]}</td><td>{total}</td><td>{refs}</td></tr>\n'
            html += "</table>\n"

        if pkg["flowServices"]:
            # Separate by service type
            adapter_svcs = [f for f in pkg["flowServices"] if f.get("svcMeta", {}) and f["svcMeta"].get("svcType") == "AdapterService"]
            flow_svcs = [f for f in pkg["flowServices"] if not f.get("svcMeta") or f.get("svcMeta", {}).get("svcType") != "AdapterService"]

            if adapter_svcs:
                html += "<h3>Adapter Services (External Endpoints)</h3>\n"
                html += "<table><tr><th>Service</th><th>Type</th><th>Input Fields</th><th>Output Fields</th></tr>\n"
                for flow in adapter_svcs:
                    meta = flow.get("svcMeta", {}) or {}
                    in_count = _count_fields(meta.get("sigIn", []))
                    out_count = _count_fields(meta.get("sigOut", []))
                    html += f'<tr><td>{flow["serviceName"]}</td><td style="color:#c0392b">AdapterService</td><td>{in_count}</td><td>{out_count}</td></tr>\n'
                html += "</table>\n"

            html += "<h3>Flow Services</h3>\n<table><tr><th>Service</th><th>Type</th><th>Steps</th><th>Invokes</th><th>I/O</th><th>Called Services</th></tr>\n"
            for flow in flow_svcs:
                called = ", ".join(set(s["service"] for s in flow["invokes"] if s["service"]))[:120]
                meta = flow.get("svcMeta", {}) or {}
                svc_type = meta.get("svcType", "flow")
                in_count = _count_fields(meta.get("sigIn", []))
                out_count = _count_fields(meta.get("sigOut", []))
                io_text = f"{in_count}→{out_count}" if in_count or out_count else ""
                html += f'<tr><td>{flow["serviceName"]}</td><td>{svc_type}</td><td>{flow["stepCount"]}</td><td>{flow["invokeCount"]}</td><td>{io_text}</td><td style="font-size:.75rem">{called}</td></tr>\n'
            html += "</table>\n"

        html += "</details></div>\n"

    html += "</details>\n"

    # Canonical schema deep dive
    html += "<details>\n<summary><h2 style='display:inline'>Canonical Schema Details</h2></summary>\n"
    for dt in canonical_types:
        html += f"<details><summary>{dt['nsName']}</summary>\n"
        html += '<div class="field-tree">\n'
        html += _render_field_tree(dt["fields"])
        html += "</div></details>\n"

    html += "</details>\n"

    html += f"""
<footer>
Generated by wm-schema-extract.py — Chorus Engineer (Kade) — {len(packages)} packages, {total_docs} doc types, {total_flows} flow services.
</footer>
</body>
</html>"""

    with open(output_path, "w") as f:
        f.write(html)

    return {
        "packages": len(packages),
        "docTypes": total_docs,
        "flowServices": total_flows,
        "totalFields": total_fields,
        "references": len(all_refs),
        "canonicalTypes": len(canonical_types),
    }


def _count_fields(fields):
    """Count total fields recursively."""
    count = len(fields)
    for f in fields:
        if f.get("children"):
            count += _count_fields(f["children"])
    return count


def _count_refs(fields):
    """Count type references recursively."""
    count = sum(1 for f in fields if f.get("ref"))
    for f in fields:
        if f.get("children"):
            count += _count_refs(f["children"])
    return count


def _render_field_tree(fields, depth=0):
    """Render fields as nested HTML."""
    html = ""
    for f in fields:
        indent = "  " * depth
        type_class = "type ref" if f.get("ref") else "type"
        ref_text = f' → <span class="ref">{f["ref"]}</span>' if f.get("ref") else ""
        opt = " (optional)" if f.get("optional") else ""
        dim = " []" if f.get("dim") == "1" else ""
        html += f'{indent}<div class="field"><span class="name">{f["name"]}</span>{dim} <span class="{type_class}">{f["type"]}{ref_text}{opt}</span></div>\n'
        if f.get("children"):
            html += f'{indent}<div class="field-tree">\n'
            html += _render_field_tree(f["children"], depth + 1)
            html += f'{indent}</div>\n'
    return html


if __name__ == "__main__":
    base = sys.argv[1] if len(sys.argv) > 1 else "/tmp/wm-packages/extracted"
    output = sys.argv[2] if len(sys.argv) > 2 else "/tmp/wm-schema-inventory.html"

    print(f"Scanning packages in {base}...", file=sys.stderr)
    packages = scan_packages(base)

    print(f"Generating HTML report...", file=sys.stderr)
    stats = generate_html(packages, output)

    print(f"\n=== Schema Extraction Complete ===", file=sys.stderr)
    for k, v in stats.items():
        print(f"  {k}: {v}", file=sys.stderr)
    print(f"\nOutput: {output}", file=sys.stderr)
