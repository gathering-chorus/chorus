## Silas Investigation: Ontology Audit (5-min timebox)

### What's in Fuseki (ontology graphs only)
| Graph | Triples | What |
|-------|---------|------|
| urn:borg:topology | 95 | 8 Services, 2 Hosts, infra topology |
| urn:chorus:ontology | 444 | 23 Classes, 29 ObjectProperties — roles, gates, spine, artifacts |
| urn:chorus:framework | 220 | Bridge layer — 11 domains, 5 APIs, 8 classes, 7 properties |
| urn:gathering:ontology:chorus-product | 1,880 | Wren's product ontology — 64 classes, 42 properties, practices, decisions, stories |
| urn:gathering:icd/ontology | 284 | ICD schema |
| urn:framework:bridge | 220 | Duplicate of urn:chorus:framework (same data, loaded twice?) |

### Cross-references: all resolve
- fw:Service → borg:Service ✓
- fw:Gate → chorus:Gate ✓  
- fw:Role → chorus:Role ✓
- fw:Artifact → chorus:ArtifactType ✓

### Issues found
1. **Duplicate graph**: `urn:framework:bridge` has same 220 triples as `urn:chorus:framework`. One should be dropped.
2. **Namespace gap**: borg uses `urn:borg:ontology/`, chorus uses `https://jeffbridwell.com/chorus#`, jb uses `https://jeffbridwell.com/ontology#`. Three different patterns. Framework bridges them but the base URIs are inconsistent.
3. **No jb ontology graph**: The jb classes (Photo, Track, Story etc.) are referenced by framework.ttl but there's no `urn:jb:ontology` graph declaring them. They exist implicitly in domain data graphs.
4. **chorus-product is the richest**: 1,880 triples — but it's a separate product model, not the same as `urn:chorus:ontology`. Two chorus graphs with different schemas.

### What's queryable end-to-end
- domain→service→gate→role: YES (21 results via framework bridge)
- Cross-graph joins: work (borg services ↔ chorus gates ↔ fw domains)
- Product ontology classes: isolated in own graph, no bridge to framework yet

### What "stabilizing" means (Silas view)
1. Drop the duplicate `urn:framework:bridge` graph
2. Decide: should chorus-product merge into chorus:ontology, or stay separate?
3. Add explicit jb:ontology graph so domain classes are declared, not just referenced
4. Namespace convergence plan: pick one pattern and migrate (but this is a big card)
