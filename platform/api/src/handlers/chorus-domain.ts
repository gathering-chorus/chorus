/**
 * GET /api/chorus/domain/:name — canonical domain view (#2198, deferred from #2188).
 *
 * Dependencies injected:
 *   domainRegistry     — Record<string, {product, step, description}>
 *   getCards           — () => Array<{id, title, status, owner, type, tags}>
 *   readDomainHtml     — (domainName) => string | null  (HTML content of artifacts/domain-<name>.html)
 *   fetchCompleteness  — async (subdomainId) => {percentage, present, missing, lifecycle} | null
 *   sparql             — async (query) => SparqlResult
 *   now                — () => number (default Date.now)
 *
 * Behavior:
 *   - 404 if domain not in registry (with validDomains list)
 *   - Cards filter: status not in {Done, Won't Do} AND tags include `domain:<name>`
 *   - Sections path 1: domain HTML — parse h2 sections with tables + lists
 *   - Sections path 2 fallback: if no HTML AND completeness resolves a subdomain, query
 *     chorus:hasScenario / hasContract / hasPriorArt / hasIntegration / hasService / hasPersistence
 *     / hasPipeline / hasGap / hasActor / hasPage / hasLogSource per predicate, build items[] + itemDetails[]
 *     with ownership walk-up (direct owner wins; parent-sub-domain owner as fallback with ownerInherited=true).
 *   - Completeness: try <name>-service first, then <name>-domain
 *   - hasIcd: true for 8 ICD domains
 *
 * Cache stays at adapter layer. Handler is pure.
 */
import type { FetchResult } from './codebase-topology';

export interface DomainMeta {
  product: string;
  step: string;
  description: string;
}

export interface DomainBoardCard {
  id: string;
  title: string;
  status: string;
  owner: string;
  type?: string;
  tags: string;
}

export interface Completeness {
  percentage: number;
  present: unknown[];
  missing: unknown[];
  lifecycle?: unknown;
}

type Sparql = (query: string) => Promise<{ results?: { bindings?: unknown[] } } | null>;

export interface ChorusDomainDeps {
  domainRegistry: Record<string, DomainMeta>;
  getCards: () => DomainBoardCard[];
  readDomainHtml: (domainName: string) => string | null;
  fetchCompleteness: (subdomainId: string) => Promise<Completeness | null>;
  sparql: Sparql;
  now?: () => number;
}

const ICD_DOMAINS = new Set(['photos', 'stories', 'people', 'music', 'documents', 'social', 'notes', 'webmethods']);

const SECTION_PREDS: Array<[string, string]> = [
  ['scenarios', 'hasScenario'],
  ['contract', 'hasContract'],
  ['prior_art', 'hasPriorArt'],
  ['integrations', 'hasIntegration'],
  ['services', 'hasService'],
  ['persistence', 'hasPersistence'],
  ['pipeline', 'hasPipeline'],
  ['gaps', 'hasGap'],
  ['actors', 'hasActor'],
  ['pages', 'hasPage'],
  ['logs', 'hasLogSource'],
];

interface HtmlSection {
  title: string;
  table?: string[][];
  items?: string[];
  itemDetails?: unknown[];
}

export function parseDomainHtml(html: string): Record<string, HtmlSection> {
  const sections: Record<string, HtmlSection> = {};
  const h2Parts = html.split(/<h2>/);
  for (const part of h2Parts.slice(1)) {
    const titleMatch = part.match(/^([^<]+)<\/h2>/);
    if (!titleMatch) continue;
    const title = titleMatch[1].trim();
    const sectionName = title.toLowerCase().replace(/\s+/g, '_');

    const rows: string[][] = [];
    const trMatches = part.match(/<tr>([\s\S]*?)<\/tr>/g) || [];
    for (const tr of trMatches) {
      const cells = (tr.match(/<t[dh][^>]*>([\s\S]*?)<\/t[dh]>/g) || [])
        .map((cell) => cell.replace(/<[^>]+>/g, '').trim());
      if (cells.length > 0) rows.push(cells);
    }

    const listItems = (part.match(/<li[^>]*>([\s\S]*?)<\/li>/g) || [])
      .map((li) => li.replace(/<[^>]+>/g, '').trim())
      .filter((s) => s.length > 0);

    sections[sectionName] = {
      title,
      ...(rows.length > 0 ? { table: rows } : {}),
      ...(listItems.length > 0 ? { items: listItems } : {}),
    };
  }
  return sections;
}

interface SparqlBindingValue { value: string }
interface SectionBinding {
  e?: SparqlBindingValue;
  label?: SparqlBindingValue;
  comment?: SparqlBindingValue;
  owners?: SparqlBindingValue;
  reads?: SparqlBindingValue;
  writes?: SparqlBindingValue;
  consumes?: SparqlBindingValue;
}

async function buildSparqlSections(
  deps: ChorusDomainDeps,
  subdomainId: string,
  sections: Record<string, HtmlSection>,
): Promise<void> {
  const sdUri = `https://jeffbridwell.com/chorus#${subdomainId}`;

  const parentOwnerQuery = `
    PREFIX chorus: <https://jeffbridwell.com/chorus#>
    PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
    SELECT ?ownerLabel WHERE {
      GRAPH <urn:chorus:ontology> {
        <${sdUri}> chorus:ownedBy ?owner .
        OPTIONAL { ?owner rdfs:label ?ownerLabel }
      }
    } LIMIT 1
  `;
  const parentResult = await deps.sparql(parentOwnerQuery).catch(() => null);
  const parentOwner: string | null =
    ((parentResult?.results?.bindings?.[0] as { ownerLabel?: SparqlBindingValue } | undefined)?.ownerLabel?.value) || null;

  const sectionQuery = (pred: string) => `
    PREFIX chorus: <https://jeffbridwell.com/chorus#>
    PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
    SELECT ?e ?label ?comment
           (GROUP_CONCAT(DISTINCT ?ownerLabel; separator="||") AS ?owners)
           (GROUP_CONCAT(DISTINCT ?readLabel; separator="||") AS ?reads)
           (GROUP_CONCAT(DISTINCT ?writeLabel; separator="||") AS ?writes)
           (GROUP_CONCAT(DISTINCT ?consumesLabel; separator="||") AS ?consumes)
    WHERE {
      GRAPH <urn:chorus:instances> {
        <${sdUri}> chorus:${pred} ?e .
        OPTIONAL { ?e rdfs:label ?label }
        OPTIONAL { ?e rdfs:comment ?comment }
        OPTIONAL { ?e chorus:ownedBy ?ownerEnt . OPTIONAL { ?ownerEnt rdfs:label ?ownerLabel } }
        OPTIONAL { ?e chorus:reads ?readTarget . OPTIONAL { ?readTarget rdfs:label ?readLabel } }
        OPTIONAL { ?e chorus:writes ?writeTarget . OPTIONAL { ?writeTarget rdfs:label ?writeLabel } }
        OPTIONAL { ?e chorus:consumes ?consumesTarget . OPTIONAL { ?consumesTarget rdfs:label ?consumesLabel } }
      }
    }
    GROUP BY ?e ?label ?comment
    LIMIT 20
  `;

  const results = await Promise.all(
    SECTION_PREDS.map(([, pred]) => deps.sparql(sectionQuery(pred)).catch(() => null)),
  );

  SECTION_PREDS.forEach(([key], i) => {
    const r = results[i];
    const bindings = (r?.results?.bindings || []) as SectionBinding[];
    if (bindings.length === 0) return;

    const items: string[] = [];
    const itemDetails: Array<Record<string, unknown>> = [];
    const split = (v: string | undefined) => (v ? v.split('||').filter(Boolean) : []);

    for (const b of bindings) {
      const label = b.label?.value;
      if (!label) continue;
      items.push(label);

      const detail: Record<string, unknown> = { label };
      if (b.comment?.value) detail.description = b.comment.value;

      const directOwners = split(b.owners?.value);
      const reads = split(b.reads?.value);
      const writes = split(b.writes?.value);
      const consumes = split(b.consumes?.value);

      if (directOwners.length) {
        detail.owner = directOwners.length === 1 ? directOwners[0] : directOwners;
      } else if (parentOwner) {
        detail.owner = parentOwner;
        detail.ownerInherited = true;
      }
      if (reads.length) detail.reads = reads;
      if (writes.length) detail.writes = writes;
      if (consumes.length) detail.consumes = consumes;
      itemDetails.push(detail);
    }

    if (items.length > 0) {
      sections[key] = { title: key.replace(/_/g, ' '), items, itemDetails };
    }
  });
}

export async function fetchChorusDomain(
  deps: ChorusDomainDeps,
  rawName: string,
): Promise<FetchResult> {
  const name = rawName.toLowerCase();
  const meta = deps.domainRegistry[name];
  if (!meta) {
    return {
      status: 404,
      body: { error: `Unknown domain: ${name}`, validDomains: Object.keys(deps.domainRegistry) },
    };
  }

  const cards = deps.getCards()
    .filter((c) => c.status !== 'Done' && c.status !== "Won't Do" && c.tags.includes(`domain:${name}`))
    .map((c) => ({ id: c.id, title: c.title, status: c.status, owner: c.owner, type: c.type }));
  const wip = cards.filter((c) => c.status === 'WIP');
  const blocked = cards.filter((c) => c.status === 'Blocked');

  let sections: Record<string, HtmlSection> = {};
  try {
    const html = deps.readDomainHtml(name);
    if (html !== null) sections = parseDomainHtml(html);
  } catch { /* html parse failed */ }

  let completeness: Completeness | null = null;
  let subdomainId: string | null = null;
  try {
    const svcResult = await deps.fetchCompleteness(`${name}-service`);
    if (svcResult) {
      completeness = svcResult;
      subdomainId = `${name}-service`;
    } else {
      const domResult = await deps.fetchCompleteness(`${name}-domain`);
      if (domResult) {
        completeness = domResult;
        subdomainId = `${name}-domain`;
      }
    }
  } catch { /* completeness unavailable */ }

  if (subdomainId && Object.keys(sections).length === 0) {
    try {
      await buildSparqlSections(deps, subdomainId, sections);
    } catch { /* sparql sections unavailable */ }
  }

  return {
    status: 200,
    body: {
      domain: name,
      product: meta.product,
      step: meta.step,
      description: meta.description,
      sections,
      cards: {
        total: cards.length,
        wip: wip.length,
        blocked: blocked.length,
        items: cards,
      },
      completeness: completeness
        ? {
            percentage: completeness.percentage,
            present: completeness.present,
            missing: completeness.missing,
            lifecycle: completeness.lifecycle,
          }
        : null,
      hasIcd: ICD_DOMAINS.has(name),
      icdEndpoint: `/api/icd/domains/${name}`,
    },
  };
}
