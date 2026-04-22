# Practice-authoring guidance

Before proposing a new skill, ritual, or habit, do the mapping *first*:

1. **Name the principle it expresses.** Which of the 24 loom-principles does this practice embody? If none, the practice is ceremony — stop and either (a) name the missing principle first, or (b) don't build it.
2. **Name the policy it operationalizes (if any).** Zero is fine — that means the practice is voluntary. If there is a policy, use `chorus:operationalizes` to link to it.
3. **Write the TTL** in `roles/silas/ontology/chorus.ttl` section 11. Pattern:
   ```turtle
   chorus:practice-<slug> a chorus:Practice ;
       rdfs:label "<short label>" ;
       rdfs:comment "<one paragraph — what the practice is + where it lives (skill, hook, ritual)>" ;
       chorus:expresses chorus:principle-<one-or-more> ;
       chorus:operationalizes chorus:policy-<zero-or-one> ;
       chorus:ceremonyRisk false .
   ```
4. **Add to `chorus:loom-practices chorus:contains`.**
5. **Load into Fuseki** via INSERT DATA into `urn:chorus:ontology`.

## The rule of thumb

- Principle without policy = **toothless**
- Policy without principle = **arbitrary**
- Practice without principle = **ceremony** (retire or name the principle)
- Practice without policy = **voluntary** (fine unless the principle says "required")
- Principle without practice = **abstract** (flag `chorus:abstract true`; aspirational claim we don't enact)

## When to retire a practice

If a practice is consistently skipped, broken, or worked around:
1. Check its `chorus:expresses` edge. Is the principle still load-bearing? If not, retire the practice.
2. Check its `chorus:operationalizes` edge. Is the policy enforced anywhere? If the policy is dormant, the practice is ritual — retire both.
3. Ceremony retires faster than principle. A principle deserves a second practice before it's abstract; a practice without a principle should die on first failure.

## Querying the triangle

```sparql
# Principles + policies that enforce them + practices that express them
PREFIX chorus: <https://jeffbridwell.com/chorus#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?principle ?pLabel ?policy ?polLabel ?practice ?prLabel WHERE {
  GRAPH <urn:chorus:ontology> {
    ?principle a chorus:Principle ; rdfs:label ?pLabel .
    OPTIONAL { ?policy chorus:enforces ?principle ; rdfs:label ?polLabel }
    OPTIONAL { ?practice chorus:expresses ?principle ; rdfs:label ?prLabel }
  }
} ORDER BY ?principle
```

## Cross-references

- Card: #2348 (this mapping)
- Parents: #2337 (principles) + #2339 (policies)
- Principle source: `roles/silas/ontology/chorus.ttl` section 9
- Policy source: `roles/silas/ontology/chorus.ttl` section 9 + Fuseki-direct loads from #2339
- Memory: `project_assemblage_model`, `project_ontology_as_reasoning_surface`, `feedback_outcomes_not_rituals`
