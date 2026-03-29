#!/usr/bin/env python3
"""OWL Graph Linter — checks product ontology for completeness gaps.

Reads the Chorus Product Ontology OWL file and reports:

Level 1 — Property completeness:
- Orphan nodes (missing required edges)
- Ungoverned entities (missing status/disposition)
- High blast radius nodes (many inbound dependsOn)
- Gate coverage gaps (practices without hasGate)
- Data binding gaps (domains without codebasePath/testCount)

Level 2 — Cross-entity consistency:
- Dangling references (edges pointing to URIs that don't exist)
- Disposition conflicts (dependsOn target with disposition=abandon)
- Governance gaps (role owns dormant domain, decision affects abandoned domain)
- Value stream coherence (service operatesIn domain not connected to any value stream)
- Orphan value streams (value stream with no domains serving it)

Usage: python3 owl-linter.py [path-to-owl]
"""

import sys
import xml.etree.ElementTree as ET
from collections import defaultdict

NS = {
    'rdf': 'http://www.w3.org/1999/02/22-rdf-syntax-ns#',
    'rdfs': 'http://www.w3.org/2000/01/rdf-schema#',
    'owl': 'http://www.w3.org/2002/07/owl#',
    'chorus': 'http://gathering.local/ontology/chorus#',
}

BASE = 'http://gathering.local/ontology/chorus#'

def short(uri):
    return uri.replace(BASE, '') if uri else '?'

def lint(path):
    tree = ET.parse(path)
    root = tree.getroot()

    findings = []
    stats = defaultdict(int)

    # Collect all individuals by class
    individuals = defaultdict(list)  # class -> [(id, label, element)]
    for elem in root:
        tag = elem.tag.replace('{' + NS['chorus'] + '}', 'chorus:')
        about = elem.get('{' + NS['rdf'] + '}about', '')
        label_el = elem.find('rdfs:label', NS)
        label = label_el.text if label_el is not None else short(about)

        if about.startswith(BASE):
            cls = tag.replace('chorus:', '') if tag.startswith('chorus:') else (tag.split('}')[-1] if '}' in tag else tag)
            individuals[cls].append((about, label, elem))

    # Helper: check if element has a property (object or datatype)
    def has_prop(elem, prop_name):
        for child in elem:
            local = child.tag.replace('{' + NS['chorus'] + '}', '')
            if local == prop_name:
                return True
        return False

    def get_prop(elem, prop_name):
        for child in elem:
            local = child.tag.replace('{' + NS['chorus'] + '}', '')
            if local == prop_name:
                ref = child.get('{' + NS['rdf'] + '}resource', '')
                return child.text or short(ref)
        return None

    def count_prop(elem, prop_name):
        count = 0
        for child in elem:
            local = child.tag.replace('{' + NS['chorus'] + '}', '')
            if local == prop_name:
                count += 1
        return count

    # --- PRODUCT RULES ---
    for uri, label, elem in individuals.get('Product', []):
        stats['products'] += 1
        if not has_prop(elem, 'hasValueStream'):
            findings.append(('CRITICAL', f'Product "{label}" has no hasValueStream — orphan product'))

    # --- DOMAIN RULES ---
    for cls in ['GatheringDomain', 'ChorusDomain']:
        for uri, label, elem in individuals.get(cls, []):
            stats['domains'] += 1
            if not has_prop(elem, 'belongsToProduct'):
                findings.append(('HIGH', f'Domain "{label}" has no belongsToProduct — floating domain'))
            if not has_prop(elem, 'servesValueStream'):
                findings.append(('HIGH', f'Domain "{label}" has no servesValueStream — unanchored'))
            if not has_prop(elem, 'status'):
                findings.append(('MEDIUM', f'Domain "{label}" has no status — ungoverned'))
            status = get_prop(elem, 'status')
            test_count = get_prop(elem, 'testCount')
            if status == 'Active' and test_count == '0':
                findings.append(('HIGH', f'Domain "{label}" is Active with testCount=0 — ungoverned territory'))
            if status == 'Active' and not has_prop(elem, 'codebasePath'):
                findings.append(('MEDIUM', f'Domain "{label}" is Active with no codebasePath — unbound to code'))

    # --- SERVICE RULES ---
    for uri, label, elem in individuals.get('Service', []):
        stats['services'] += 1
        if not has_prop(elem, 'operatesIn'):
            findings.append(('CRITICAL', f'Service "{label}" has no operatesIn — unanchored service'))
        if not has_prop(elem, 'ownedBy'):
            findings.append(('HIGH', f'Service "{label}" has no ownedBy — unowned'))
        if not has_prop(elem, 'runsOn'):
            findings.append(('MEDIUM', f'Service "{label}" has no runsOn — where does it run?'))
        if not has_prop(elem, 'disposition'):
            findings.append(('MEDIUM', f'Service "{label}" has no disposition — ungoverned'))
        if not has_prop(elem, 'experience'):
            findings.append(('LOW', f'Service "{label}" has no experience statement'))

        # High blast radius check
        disposition = get_prop(elem, 'disposition')
        depends_count = count_prop(elem, 'dependsOn')
        if disposition == 'rewrite' and depends_count >= 2:
            findings.append(('HIGH', f'Service "{label}" disposition=rewrite with {depends_count} dependsOn — high blast radius rewrite'))

    # --- PRACTICE RULES ---
    for uri, label, elem in individuals.get('Practice', []):
        stats['practices'] += 1
        if not has_prop(elem, 'affinityWith'):
            findings.append(('LOW', f'Practice "{label}" has no affinityWith — unplaced in value stream'))
        applies_count = count_prop(elem, 'appliesTo')
        has_gate = has_prop(elem, 'hasGate')
        if applies_count >= 2 and not has_gate:
            findings.append(('HIGH', f'Practice "{label}" applies to {applies_count} domains but has no gate — ungoverned high-fanout'))
        elif not has_gate:
            findings.append(('LOW', f'Practice "{label}" has no gate — discipline only'))

    # --- ROLE RULES ---
    for uri, label, elem in individuals.get('Role', []):
        stats['roles'] += 1
        if not has_prop(elem, 'ownsDomain'):
            findings.append(('CRITICAL', f'Role "{label}" owns no domains'))
        if not has_prop(elem, 'hasConstraint'):
            findings.append(('HIGH', f'Role "{label}" has no constraints'))
        if not has_prop(elem, 'hasResponsibility'):
            findings.append(('HIGH', f'Role "{label}" has no responsibilities'))

    # --- DECISION RULES ---
    for uri, label, elem in individuals.get('Decision', []):
        stats['decisions'] += 1
        if not has_prop(elem, 'affects'):
            findings.append(('MEDIUM', f'Decision "{label}" affects no domains'))
        if not has_prop(elem, 'createdBy'):
            findings.append(('LOW', f'Decision "{label}" has no createdBy'))
        if not has_prop(elem, 'status'):
            findings.append(('MEDIUM', f'Decision "{label}" has no status (active/superseded/withdrawn)'))

    # --- MACHINE RULES ---
    for uri, label, elem in individuals.get('Machine', []):
        stats['machines'] += 1

    # --- DATASOURCE RULES ---
    for uri, label, elem in individuals.get('DataSource', []):
        stats['datasources'] += 1

    # Check if any service uses readsFrom/writesTo
    any_reads = any(has_prop(e, 'readsFrom') for _, _, e in individuals.get('Service', []))
    any_writes = any(has_prop(e, 'writesTo') for _, _, e in individuals.get('Service', []))
    if not any_reads:
        findings.append(('HIGH', 'No service has readsFrom DataSource — data lineage completely unwired'))
    if not any_writes:
        findings.append(('HIGH', 'No service has writesTo DataSource — data lineage completely unwired'))

    # ========================================
    # LEVEL 2 — Cross-entity consistency
    # ========================================

    # Build lookup indexes
    all_uris = set()
    uri_to_label = {}
    uri_to_class = {}
    uri_to_elem = {}
    for cls, items in individuals.items():
        for uri, label, elem in items:
            all_uris.add(uri)
            uri_to_label[uri] = label
            uri_to_class[uri] = cls
            uri_to_elem[uri] = elem

    def get_refs(elem, prop_name):
        """Get all rdf:resource URIs for a property."""
        refs = []
        for child in elem:
            local = child.tag.replace('{' + NS['chorus'] + '}', '')
            if local == prop_name:
                ref = child.get('{' + NS['rdf'] + '}resource', '')
                if ref:
                    refs.append(ref)
        return refs

    # --- DANGLING REFERENCES ---
    # Every rdf:resource reference should point to a URI that exists as an individual
    edge_props = ['hasValueStream', 'belongsToProduct', 'servesValueStream', 'operatesIn',
                  'supportsStream', 'ownedBy', 'dependsOn', 'runsOn', 'readsFrom', 'writesTo',
                  'ownsDomain', 'hasConstraint', 'hasResponsibility', 'hasGate', 'appliesTo',
                  'affects', 'constrains', 'createdBy', 'supersedes', 'affinityWith',
                  'triggersSkill', 'hasPhase']
    for cls, items in individuals.items():
        for uri, label, elem in items:
            for prop in edge_props:
                for ref in get_refs(elem, prop):
                    if ref.startswith(BASE) and ref not in all_uris:
                        findings.append(('HIGH', f'{cls} "{label}" {prop} → {short(ref)} — dangling reference (target not found)'))

    # --- DISPOSITION CONFLICTS ---
    # Service dependsOn a service with disposition=abandon — depending on a dead end
    for uri, label, elem in individuals.get('Service', []):
        for dep_uri in get_refs(elem, 'dependsOn'):
            if dep_uri in uri_to_elem:
                dep_disp = get_prop(uri_to_elem[dep_uri], 'disposition')
                dep_label = uri_to_label.get(dep_uri, short(dep_uri))
                if dep_disp == 'abandon':
                    findings.append(('CRITICAL', f'Service "{label}" dependsOn "{dep_label}" which has disposition=abandon — dependency on dead end'))
                elif dep_disp == 'rewrite':
                    findings.append(('MEDIUM', f'Service "{label}" dependsOn "{dep_label}" which has disposition=rewrite — upstream instability'))

    # --- DECISION vs DOMAIN DISPOSITION ---
    # Active decision affecting a domain with disposition/status=Abandoned
    domain_status = {}
    for cls in ['GatheringDomain', 'ChorusDomain']:
        for uri, label, elem in individuals.get(cls, []):
            domain_status[uri] = get_prop(elem, 'status')

    for uri, label, elem in individuals.get('Decision', []):
        dec_status = get_prop(elem, 'status')
        if dec_status and dec_status.lower() in ('active', 'accepted'):
            for affected_uri in get_refs(elem, 'affects'):
                ds = domain_status.get(affected_uri)
                if ds and ds.lower() in ('abandoned', 'dormant'):
                    findings.append(('MEDIUM', f'Decision "{label}" (active) affects domain "{uri_to_label.get(affected_uri, "?")}" which is {ds} — stale decision'))

    # --- ROLE OWNS DORMANT DOMAIN ---
    for uri, label, elem in individuals.get('Role', []):
        for owned_uri in get_refs(elem, 'ownsDomain'):
            ds = domain_status.get(owned_uri)
            if ds and ds.lower() in ('abandoned', 'dormant'):
                findings.append(('LOW', f'Role "{label}" ownsDomain "{uri_to_label.get(owned_uri, "?")}" which is {ds} — governing nothing'))

    # --- VALUE STREAM COHERENCE ---
    # Service operatesIn a domain that has no servesValueStream — service disconnected from value delivery
    for uri, label, elem in individuals.get('Service', []):
        for domain_uri in get_refs(elem, 'operatesIn'):
            if domain_uri in uri_to_elem:
                domain_elem = uri_to_elem[domain_uri]
                if not has_prop(domain_elem, 'servesValueStream'):
                    findings.append(('MEDIUM', f'Service "{label}" operatesIn "{uri_to_label.get(domain_uri, "?")}" which serves no value stream — disconnected from value delivery'))

    # --- ORPHAN VALUE STREAMS ---
    # Value stream that no domain serves
    vs_served = set()
    for cls in ['GatheringDomain', 'ChorusDomain']:
        for uri, label, elem in individuals.get(cls, []):
            for vs_uri in get_refs(elem, 'servesValueStream'):
                vs_served.add(vs_uri)
    for uri, label, elem in individuals.get('ValueStream', []):
        if uri not in vs_served:
            findings.append(('HIGH', f'ValueStream "{label}" has no domains serving it — orphan value stream'))

    # --- UNOWNED DOMAINS ---
    # Domain not referenced by any Role's ownsDomain
    owned_domains = set()
    for uri, label, elem in individuals.get('Role', []):
        for d_uri in get_refs(elem, 'ownsDomain'):
            owned_domains.add(d_uri)
    for cls in ['GatheringDomain', 'ChorusDomain']:
        for uri, label, elem in individuals.get(cls, []):
            status = get_prop(elem, 'status')
            if status and status.lower() == 'active' and uri not in owned_domains:
                findings.append(('MEDIUM', f'Domain "{label}" is Active but no Role ownsDomain it — unowned active territory'))

    # --- REPORT ---
    print(f"=== OWL Graph Linter ===")
    print(f"File: {path}")
    print(f"Stats: {dict(stats)}")
    print(f"Findings: {len(findings)}")
    print()

    by_severity = defaultdict(list)
    for sev, msg in findings:
        by_severity[sev].append(msg)

    for sev in ['CRITICAL', 'HIGH', 'MEDIUM', 'LOW']:
        items = by_severity.get(sev, [])
        if items:
            print(f"--- {sev} ({len(items)}) ---")
            for msg in items:
                print(f"  {msg}")
            print()

    total = len(findings)
    critical = len(by_severity.get('CRITICAL', []))
    high = len(by_severity.get('HIGH', []))
    print(f"Summary: {total} findings ({critical} critical, {high} high)")
    return 1 if critical > 0 else 0

if __name__ == '__main__':
    owl_path = sys.argv[1] if len(sys.argv) > 1 else '/Users/jeffbridwell/CascadeProjects/product-manager/artifacts/chorus-product-ontology.owl'
    sys.exit(lint(owl_path))
