/**
 * MCP Registry Generator (#2452 spike, continuation)
 *
 * SPARQL query → MCP tool declarations.
 *
 * Silas's #2452 feedback amendments applied:
 *   (a) Namespaced tool names: chorus.skill.<id> / chorus.gate.<id>
 *   (b) FAIL-LOUD if any chorus:Skill or chorus:Gate lacks implementedIn —
 *       honest-fold at the registry level; silent skip = registry lies about
 *       what exists.
 *
 * In-memory only for spike. Production would land in event-bus cache with
 * /api/athena/reload invalidation (Silas's additional catch).
 */

const FUSEKI = process.env.FUSEKI_BASE ?? "http://localhost:3030";

// Filter out entities marked chorus:abstract true — the explicit escape for
// category/aspirational declarations that don't map to real implementations.
const SKILL_QUERY = `
PREFIX chorus: <https://jeffbridwell.com/chorus#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?entity ?label ?comment ?impl ?isUtility
WHERE {
  GRAPH <urn:chorus:ontology> {
    ?entity a chorus:Skill .
    FILTER NOT EXISTS { ?entity chorus:abstract true }
    OPTIONAL { ?entity rdfs:label ?label }
    OPTIONAL { ?entity rdfs:comment ?comment }
    OPTIONAL { ?entity chorus:implementedIn ?impl }
    BIND(EXISTS { ?entity a chorus:UtilitySkill } AS ?isUtility)
  }
}
ORDER BY ?entity
`;

const GATE_QUERY = `
PREFIX chorus: <https://jeffbridwell.com/chorus#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?entity ?label ?comment ?impl
WHERE {
  GRAPH <urn:chorus:ontology> {
    ?entity a chorus:Gate .
    FILTER NOT EXISTS { ?entity chorus:abstract true }
    # Exclude gate superclass instances (BuildGate / DesignGate / ProvingGate /
    # DirectionGate) that are declared as Gate instances but are really categories.
    # Ideally these would be rdfs:subClassOf chorus:Gate or carry chorus:abstract true;
    # for the spike we filter them explicitly. Drift ticket: see README.
    FILTER NOT EXISTS {
      ?instance rdfs:subClassOf ?entity .
    }
    OPTIONAL { ?entity rdfs:label ?label }
    OPTIONAL { ?entity rdfs:comment ?comment }
    OPTIONAL { ?entity chorus:implementedIn ?impl }
  }
}
ORDER BY ?entity
`;

async function sparql(query) {
  const resp = await fetch(`${FUSEKI}/pods/query`, {
    method: "POST",
    headers: {
      "Content-Type": "application/x-www-form-urlencoded",
      Accept: "application/sparql-results+json",
    },
    body: new URLSearchParams({ query }).toString(),
  });
  if (!resp.ok) throw new Error(`Fuseki error: ${resp.status}`);
  return resp.json();
}

function slug(uri) {
  return uri.split("#").pop() ?? uri;
}

/**
 * Build namespaced MCP tool name per Silas's amendment (a).
 * chorus:skill-pull   → chorus.skill.pull
 * chorus:gate-product → chorus.gate.product
 */
function namespacedName(category, entityId) {
  const shortId = entityId.replace(/^(skill|gate)-/, "");
  return `chorus.${category}.${shortId}`;
}

/**
 * Generate MCP tool declarations from the graph.
 * FAIL-LOUD if any entity lacks implementedIn. Registry must not lie.
 */
export async function generateRegistry({ includeUtility = false } = {}) {
  const [skillResult, gateResult] = await Promise.all([
    sparql(SKILL_QUERY),
    sparql(GATE_QUERY),
  ]);

  const missing = [];
  const tools = [];

  // Skills
  for (const b of skillResult.results?.bindings ?? []) {
    const entityUri = b.entity?.value;
    const entityId = slug(entityUri);
    const isUtility = b.isUtility?.value === "true";
    if (isUtility && !includeUtility) continue;

    const impl = b.impl?.value;
    if (!impl) {
      missing.push({ kind: "skill", id: entityId });
      continue;
    }

    tools.push({
      name: namespacedName("skill", entityId),
      description: b.comment?.value ?? b.label?.value ?? entityId,
      _source: { kind: "skill", id: entityId, implementedIn: impl },
      inputSchema: {
        type: "object",
        properties: {
          card_id: {
            type: "integer",
            description: "Card ID (for skills that operate on cards; ignored otherwise)",
          },
        },
      },
    });
  }

  // Gates
  for (const b of gateResult.results?.bindings ?? []) {
    const entityUri = b.entity?.value;
    const entityId = slug(entityUri);
    const impl = b.impl?.value;
    if (!impl) {
      missing.push({ kind: "gate", id: entityId });
      continue;
    }

    tools.push({
      name: namespacedName("gate", entityId),
      description: b.comment?.value ?? b.label?.value ?? entityId,
      _source: { kind: "gate", id: entityId, implementedIn: impl },
      inputSchema: {
        type: "object",
        properties: {
          card_id: {
            type: "integer",
            description: "Card ID the gate evaluates against",
          },
        },
      },
    });
  }

  // Silas amendment (b): fail-loud on any missing implementedIn.
  if (missing.length > 0) {
    const summary = missing
      .map((m) => `  - chorus:${m.kind}-${m.id}`)
      .join("\n");
    throw new Error(
      `Registry generation failed — ${missing.length} entities lack chorus:implementedIn:\n${summary}\n` +
        `Honest-fold discipline: registry must not lie about what exists. ` +
        `Fix the graph, or mark these entities with chorus:abstract true explicitly.`,
    );
  }

  return {
    generatedAt: new Date().toISOString(),
    count: tools.length,
    tools,
  };
}

// CLI mode: print generated registry for inspection
if (import.meta.url === `file://${process.argv[1]}`) {
  try {
    const reg = await generateRegistry();
    console.log(JSON.stringify(reg, null, 2));
  } catch (err) {
    console.error("GENERATOR FAILED:", err.message);
    process.exitCode = 1;
  }
}
