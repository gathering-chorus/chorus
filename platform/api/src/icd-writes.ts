// ICD write handlers (extracted from server.ts for #2205 wave 22).
// First handler: POST /api/icd/domains/:id/fields — upserts a CanonicalField.

interface Req { params: Record<string, string>; body: any }
interface Res {
  status: (s: number) => Res;
  json: (b: any) => Res | void;
}

export interface IcdFieldUpsertDeps {
  resolveDomain: (domainId: string) => Promise<string | null>;
  client: {
    query: (q: string) => Promise<any>;
    update: (u: string) => Promise<void>;
  };
  pfx: string;
  graph: string;
  icdSlug: (s: string) => string;
  escSparql: (s: string) => string;
}

const SEVERITY_MAP: Record<string, string> = {
  violation: 'icd:Violation',
  enrichment: 'icd:Enrichment',
  warning: 'icd:Warning',
  info: 'icd:Info',
};

export async function handleIcdFieldUpsert(req: Req, res: Res, deps: IcdFieldUpsertDeps): Promise<void> {
  try {
    const { name, severity, datatype, constraint, cardinality, bestSource, description, order } = req.body;
    if (!name || !severity) { res.status(400).json!({ error: 'name and severity are required' }); return; }
    const validSev = Object.keys(SEVERITY_MAP);
    if (!validSev.includes(severity)) {
      res.status(400).json!({ error: `severity must be one of: ${validSev.join(', ')}` });
      return;
    }

    const domainUri = await deps.resolveDomain(req.params.id);
    if (!domainUri) {
      res.status(404).json!({ error: `Domain '${req.params.id}' not found` });
      return;
    }

    const slug = deps.icdSlug(req.params.id);
    const fieldSlug = deps.icdSlug(name);
    const fieldUri = `https://jeffbridwell.com/icd/field/${slug}/${fieldSlug}`;
    const typeUri = `https://jeffbridwell.com/icd/type/${slug}`;

    const exists = await deps.client.query(
      `${deps.pfx} SELECT ?f WHERE { GRAPH <${deps.graph}> { <${fieldUri}> a icd:CanonicalField } } LIMIT 1`,
    );
    const isNew = exists.results.bindings.length === 0;

    await deps.client.update(`${deps.pfx}
      DELETE WHERE { GRAPH <${deps.graph}> { <${fieldUri}> ?p ?o } };
      INSERT DATA { GRAPH <${deps.graph}> {
        <${fieldUri}> a icd:CanonicalField ;
          icd:canonicalName "${deps.escSparql(name)}" ; icd:displayName "${deps.escSparql(name)}" ;
          icd:severity ${SEVERITY_MAP[severity]} ; icd:datatype "${deps.escSparql(datatype || 'xsd:string')}" ;
          icd:cardinality "${deps.escSparql(cardinality || '1')}" ; icd:fieldOrder ${order ?? 0} ;
          icd:inDomain <${domainUri}> ; icd:inConsumerType <${typeUri}> .
        ${constraint ? `<${fieldUri}> icd:constraint "${deps.escSparql(constraint)}" .` : ''}
        ${bestSource ? `<${fieldUri}> icd:bestSource "${deps.escSparql(bestSource)}" .` : ''}
        ${description ? `<${fieldUri}> icd:fieldTypeDescription "${deps.escSparql(description)}" .` : ''}
        <${typeUri}> icd:hasCanonicalField <${fieldUri}> .
      } }`);

    res.status(isNew ? 201 : 200).json!({ ok: true, domain: req.params.id, field: name, created: isNew });
  } catch (err) {
    res.status(500).json!({ error: 'Failed to upsert ICD field', detail: String(err) });
  }
}
