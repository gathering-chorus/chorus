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

export async function handleIcdMappingUpsert(req: Req, res: Res, deps: IcdFieldUpsertDeps): Promise<void> {
  try {
    const { providerId, sourceField, mapsTo, confidence, transform, description, coverageLabel, coverageClass, order } = req.body;
    if (!providerId || !sourceField || !mapsTo || !confidence) {
      res.status(400).json!({ error: 'providerId, sourceField, mapsTo, and confidence are required' });
      return;
    }

    const domainUri = await deps.resolveDomain(req.params.id);
    if (!domainUri) {
      res.status(404).json!({ error: `Domain '${req.params.id}' not found` });
      return;
    }

    const slug = deps.icdSlug(req.params.id);
    const provSlug = deps.icdSlug(providerId);
    const provUri = `https://jeffbridwell.com/icd/provider/${slug}/${provSlug}`;
    const mappingSlug = deps.icdSlug(sourceField);
    const mappingUri = `https://jeffbridwell.com/icd/mapping/${slug}/${mappingSlug}`;
    const mapsToSlug = deps.icdSlug(String(mapsTo).split(',')[0].trim());
    const fieldUri = `https://jeffbridwell.com/icd/field/${slug}/${mapsToSlug}`;

    const provExists = await deps.client.query(
      `${deps.pfx} SELECT ?p WHERE { GRAPH <${deps.graph}> { <${provUri}> a icd:Provider } } LIMIT 1`,
    );
    if (provExists.results.bindings.length === 0) {
      res.status(404).json!({ error: `Provider '${providerId}' not found` });
      return;
    }

    const exists = await deps.client.query(
      `${deps.pfx} SELECT ?m WHERE { GRAPH <${deps.graph}> { <${mappingUri}> a icd:FieldMapping } } LIMIT 1`,
    );
    const isNew = exists.results.bindings.length === 0;

    await deps.client.update(`${deps.pfx}
      DELETE WHERE { GRAPH <${deps.graph}> { <${mappingUri}> ?p ?o } };
      INSERT DATA { GRAPH <${deps.graph}> {
        <${mappingUri}> a icd:FieldMapping ;
          icd:mappingOrder ${order ?? 0} ; icd:sourceTable "${deps.escSparql(providerId)}" ;
          icd:sourceField "${deps.escSparql(sourceField)}" ; icd:mapsTo <${fieldUri}> ;
          icd:mapsToName "${deps.escSparql(mapsTo)}" ; icd:confidence "${deps.escSparql(confidence)}" ;
          icd:fromProvider <${provUri}> .
        ${transform ? `<${mappingUri}> icd:transform "${deps.escSparql(transform)}" .` : ''}
        ${description ? `<${mappingUri}> icd:fieldDescription "${deps.escSparql(description)}" .` : ''}
        ${coverageLabel ? `<${mappingUri}> icd:fieldCoverageLabel "${deps.escSparql(coverageLabel)}" .` : ''}
        ${coverageClass ? `<${mappingUri}> icd:fieldCoverageClass "${deps.escSparql(coverageClass)}" .` : ''}
        <${provUri}> icd:hasMapping <${mappingUri}> .
      } }`);

    res.status(isNew ? 201 : 200).json!({ ok: true, domain: req.params.id, provider: providerId, sourceField, created: isNew });
  } catch (err) {
    res.status(500).json!({ error: 'Failed to upsert ICD mapping', detail: String(err) });
  }
}

export async function handleIcdSectionPut(req: Req, res: Res, deps: IcdFieldUpsertDeps): Promise<void> {
  try {
    const { title, type, paragraphs, risks, nonFunctionals, mermaid } = req.body;
    if (!title) { res.status(400).json!({ error: 'title is required' }); return; }

    const domainUri = await deps.resolveDomain(req.params.id);
    if (!domainUri) { res.status(404).json!({ error: `Domain '${req.params.id}' not found` }); return; }

    const slug = deps.icdSlug(req.params.id);
    const provSlug = deps.icdSlug(req.params.pid);
    const secSlug = deps.icdSlug(title);
    const provUri = `https://jeffbridwell.com/icd/provider/${slug}/${provSlug}`;
    const secUri = `https://jeffbridwell.com/icd/section/${slug}/${secSlug}`;

    const provExists = await deps.client.query(
      `${deps.pfx} SELECT ?p WHERE { GRAPH <${deps.graph}> { <${provUri}> a icd:Provider } } LIMIT 1`,
    );
    if (provExists.results.bindings.length === 0) {
      res.status(404).json!({ error: `Provider '${req.params.pid}' not found` });
      return;
    }

    await deps.client.update(`${deps.pfx}
      DELETE WHERE { GRAPH <${deps.graph}> { <${secUri}> icd:hasParagraph ?para . ?para ?pp ?po . } };
      DELETE WHERE { GRAPH <${deps.graph}> { <${secUri}> icd:hasRiskItem ?risk . ?risk ?rp ?ro . } };
      DELETE WHERE { GRAPH <${deps.graph}> { <${secUri}> ?p ?o } }`);

    const sType = type || 'content';
    let triples = `<${secUri}> a icd:Section ; icd:sectionTitle "${deps.escSparql(title)}" ; icd:sectionType "${deps.escSparql(sType)}" ; icd:sectionOrder 0 . <${provUri}> icd:hasSection <${secUri}> .`;

    if (paragraphs) {
      for (let i = 0; i < paragraphs.length; i++) {
        const pUri = `${secUri}/para-${i}`;
        triples += ` <${secUri}> icd:hasParagraph <${pUri}> . <${pUri}> a icd:Paragraph ; icd:paragraphOrder ${i} ; icd:paragraphLabel "" ; icd:paragraphText "${deps.escSparql(paragraphs[i])}" .`;
      }
    }
    if (risks) {
      for (let i = 0; i < risks.length; i++) {
        const rUri = `${secUri}/risk-${i}`;
        triples += ` <${secUri}> icd:hasRiskItem <${rUri}> . <${rUri}> a icd:RiskItem ; icd:riskOrder ${i} ; icd:riskStatus "${deps.escSparql(risks[i].status)}" ; icd:riskText "${deps.escSparql(risks[i].text)}" .`;
      }
    }
    if (nonFunctionals) {
      const nf = nonFunctionals;
      triples += ` <${secUri}> icd:nfVolume "${deps.escSparql(nf.volume)}" ; icd:nfFreshness "${deps.escSparql(nf.freshness)}" ; icd:nfLatency "${deps.escSparql(nf.latency)}" ; icd:nfAuth "${deps.escSparql(nf.auth)}" .`;
    }
    if (mermaid) {
      triples += ` <${secUri}> icd:mermaidSource """${mermaid}""" .`;
    }

    await deps.client.update(`${deps.pfx} INSERT DATA { GRAPH <${deps.graph}> { ${triples} } }`);
    res.json!({ ok: true, domain: req.params.id, provider: req.params.pid, section: title });
  } catch (err) {
    res.status(500).json!({ error: 'Failed to update ICD section', detail: String(err) });
  }
}
