#!/bin/bash
# rebuild-explorer.sh — OWL → JSON → HTML → open
# One command: bash rebuild-explorer.sh
set -euo pipefail
cd "$(dirname "$0")"

echo "Validating OWL..."
riot --validate chorus-product-ontology.owl

echo "Parsing OWL → JSON..."
python3 << 'PYEOF'
import xml.etree.ElementTree as ET, json

base = 'http://gathering.local/ontology/chorus#'
rdf_ns = 'http://www.w3.org/1999/02/22-rdf-syntax-ns#'
rdfs_ns = 'http://www.w3.org/2000/01/rdf-schema#'
owl_ns = 'http://www.w3.org/2002/07/owl#'

tree = ET.parse('chorus-product-ontology.owl')
root = tree.getroot()
class_uris = set(c.get(f'{{{rdf_ns}}}about','') for c in root.findall(f'.//{{{owl_ns}}}Class'))
prop_uris = set(p.get(f'{{{rdf_ns}}}about','') for p in root.findall(f'.//{{{owl_ns}}}ObjectProperty') + root.findall(f'.//{{{owl_ns}}}DatatypeProperty'))
skip = class_uris | prop_uris | {base[:-1]}

nodes, links, about_map = [], [], {}
for el in root:
    about = el.get(f'{{{rdf_ns}}}about','')
    if not about or about in skip: continue
    cls = el.tag.split('}')[-1] if '}' in el.tag else el.tag
    label_el = el.find(f'{{{rdfs_ns}}}label')
    label = label_el.text if label_el is not None else about.split('#')[-1]
    desc = []
    for child in el:
        ct = child.tag.split('}')[-1] if '}' in child.tag else child.tag
        if ct in ('label','comment'): continue
        ref = child.get(f'{{{rdf_ns}}}resource','')
        if ref and ref.startswith(base): links.append((about, ref, ct))
        elif child.text: desc.append(ct+'='+child.text[:30])
    about_map[about] = label
    nodes.append({'id':label,'cls':cls,'desc':' | '.join(desc[:4])})

ids = [n['id'] for n in nodes]
dupes = set(x for x in ids if ids.count(x)>1)
for n in nodes:
    if n['id'] in dupes:
        sfx = n['cls'].replace('ChorusDomain','domain').replace('GatheringDomain','domain').replace('InteractionPattern','pattern').replace('ValueStream','stream')
        new = n['id']+' ('+sfx+')'
        for a,l in list(about_map.items()):
            if l == n['id']:
                for el2 in root:
                    if el2.get(f'{{{rdf_ns}}}about','')==a:
                        ec = el2.tag.split('}')[-1] if '}' in el2.tag else el2.tag
                        if ec == n['cls']: about_map[a] = new
        n['id'] = new

resolved = [{'s':about_map[s],'t':about_map[t],'l':l,'c':''} for s,t,l in links if about_map.get(s) and about_map.get(t)]

with open('owl-nodes.json','w') as f: json.dump(nodes,f)
with open('owl-links.json','w') as f: json.dump(resolved,f)

# Re-inline into explorer
with open('chorus-instance-explorer.html') as f: lines = f.readlines()
new = []
for line in lines:
    if line.startswith('var N='): new.append('var N='+json.dumps(nodes,ensure_ascii=False)+';\n')
    elif line.startswith('var E='): new.append('var E='+json.dumps(resolved,ensure_ascii=False)+';\n')
    else: new.append(line)
with open('chorus-instance-explorer.html','w') as f: f.writelines(new)

from collections import Counter
cc = Counter(n['cls'] for n in nodes)
print(f"{len(nodes)} nodes, {len(resolved)} links")
for k,v in cc.most_common(): print(f"  {k}: {v}")
PYEOF

echo "Opening explorer..."
open chorus-instance-explorer.html
echo "Done."
