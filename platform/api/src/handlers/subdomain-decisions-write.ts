/**
 * POST /api/athena/subdomains/:id/decisions — create a decision (DEC | ADR | protocol).
 * PUT  /api/athena/subdomains/:id/decisions/:entityId — update a decision by short id.
 *
 * #2716. Per ADR-028 Addendum 2, validation is a single Zod schema referenced by
 * both POST and PUT (and any future MCP `chorus_decisions_create` tool). Writes
 * go to `urn:chorus:instances`. SPARQL prefixes are declared via the shared
 * `SPARQL_PREFIXES` constant.
 *
 * Class-specific deviation from ADR-028 MUST 2 URI-mint rule:
 *   Decisions carry a canonical short `id` ("adr-028", "dec-095") that pre-exists
 *   the graph and must remain stable across label edits. URIs mint as
 *   `<chorus#><id>`, not `<chorus#><subdomainId>-decisions-<slug-of-label>`.
 *   Justified because the label can change (e.g. ADR title rewrite) without
 *   breaking citations. Noted in the ADR-028 reconcile (#2716 AC 8).
 */

import { z } from 'zod';
import type { FetchResult } from './sessions';
import type { DomainFacetDeps } from './domain-facets';

const CHORUS_PREFIX = 'https://jeffbridwell.com/chorus#';

const SPARQL_PREFIXES =
  'PREFIX chorus: <https://jeffbridwell.com/chorus#> ' +
  'PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>';

export interface DecisionWriteDeps extends DomainFacetDeps {
  sparqlUpdate: (update: string) => Promise<void>;
}

export const DecisionInputSchema = z.object({
  id: z
    .string()
    .min(1)
    .regex(/^(adr|dec|protocol)-[a-z0-9-]+$/i, {
      message: 'id must match pattern <adr|dec|protocol>-<slug>, e.g. adr-028',
    }),
  label: z.string().min(1, { message: 'label required (non-empty)' }),
  comment: z.string().min(1, { message: 'comment required (markdown body)' }),
  decisionType: z.enum(['DEC', 'ADR', 'protocol']),
  status: z.string().optional(),
  date: z.string().optional(),
  enforcementLevel: z.string().optional(),
  domains: z.array(z.string()).optional(),
});

export type DecisionInput = z.infer<typeof DecisionInputSchema>;

function escapeLiteral(s: string): string {
  return s
    .replace(/\\/g, '\\\\')
    .replace(/"/g, '\\"')
    .replace(/\r/g, '\\r')
    .replace(/\n/g, '\\n')
    .replace(/\t/g, '\\t');
}

function tripleSet(uri: string, input: DecisionInput): string {
  const escapedComment = escapeLiteral(input.comment);
  const parts: string[] = [
    `<${uri}> a chorus:Decision`,
    `<${uri}> chorus:id "${escapeLiteral(input.id)}"`,
    `<${uri}> rdfs:label "${escapeLiteral(input.label)}"`,
    `<${uri}> rdfs:comment "${escapedComment}"`,
    `<${uri}> chorus:decisionType "${input.decisionType}"`,
  ];
  if (input.status) parts.push(`<${uri}> chorus:status "${escapeLiteral(input.status)}"`);
  if (input.date) parts.push(`<${uri}> chorus:date "${escapeLiteral(input.date)}"`);
  if (input.enforcementLevel)
    parts.push(`<${uri}> chorus:enforcementLevel "${escapeLiteral(input.enforcementLevel)}"`);
  if (input.domains) {
    for (const dUri of input.domains) {
      parts.push(`<${uri}> chorus:hasDomain <${dUri}>`);
    }
  }
  return parts.join(' . ') + ' .';
}

function envelopeFor(deps: DecisionWriteDeps, name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}): unknown {
  return deps.envelope(name, data, durationMs, extra);
}

function badRequest(deps: DecisionWriteDeps, name: string, err: z.ZodError, durationMs: number): FetchResult {
  return {
    status: 400,
    body: envelopeFor(
      deps,
      name,
      {
        error: 'Validation failed',
        issues: err.issues.map((i) => ({ path: i.path.join('.'), message: i.message })),
      },
      durationMs,
      { error: true },
    ),
  };
}

export async function createSubdomainDecision(
  deps: DecisionWriteDeps,
  subdomainId: string,
  body: Record<string, unknown> | null | undefined,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  const envelopeName = 'subdomain-decision-create';
  const parsed = DecisionInputSchema.safeParse(body ?? {});
  if (!parsed.success) return badRequest(deps, envelopeName, parsed.error, now() - start);
  const input = parsed.data;
  const entityUri = `${CHORUS_PREFIX}${input.id.toLowerCase()}`;
  const sdUri = `${CHORUS_PREFIX}${subdomainId}`;
  const triples = tripleSet(entityUri, input);
  const subdomainEdge = `<${entityUri}> chorus:hasDomain <${sdUri}> .`;
  const update = `${SPARQL_PREFIXES} INSERT DATA { GRAPH <urn:chorus:instances> { ${triples} ${subdomainEdge} } }`;
  try {
    await deps.sparqlUpdate(update);
    return {
      status: 200,
      body: envelopeFor(
        deps,
        envelopeName,
        { subdomain: subdomainId, uri: entityUri, ...input },
        now() - start,
      ),
    };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return {
      status: 500,
      body: envelopeFor(deps, envelopeName, { error: message }, now() - start, { error: true }),
    };
  }
}

export async function updateSubdomainDecision(
  deps: DecisionWriteDeps,
  subdomainId: string,
  entityId: string,
  body: Record<string, unknown> | null | undefined,
): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const start = now();
  const envelopeName = 'decision-update';
  const parsed = DecisionInputSchema.safeParse({ ...(body ?? {}), id: entityId });
  if (!parsed.success) return badRequest(deps, envelopeName, parsed.error, now() - start);
  const input = parsed.data;
  if (input.id.toLowerCase() !== entityId.toLowerCase()) {
    return {
      status: 400,
      body: envelopeFor(
        deps,
        envelopeName,
        { error: `body.id (${input.id}) does not match URL entityId (${entityId})` },
        now() - start,
        { error: true },
      ),
    };
  }
  const entityUri = `${CHORUS_PREFIX}${entityId.toLowerCase()}`;
  const sdUri = `${CHORUS_PREFIX}${subdomainId}`;
  const triples = tripleSet(entityUri, input);
  const subdomainEdge = `<${entityUri}> chorus:hasDomain <${sdUri}> .`;
  const deleteQuery = `${SPARQL_PREFIXES} DELETE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o } } WHERE { GRAPH <urn:chorus:instances> { <${entityUri}> ?p ?o } }`;
  const insertQuery = `${SPARQL_PREFIXES} INSERT DATA { GRAPH <urn:chorus:instances> { ${triples} ${subdomainEdge} } }`;
  try {
    await deps.sparqlUpdate(deleteQuery);
    await deps.sparqlUpdate(insertQuery);
    return {
      status: 200,
      body: envelopeFor(
        deps,
        envelopeName,
        { subdomain: subdomainId, uri: entityUri, ...input },
        now() - start,
      ),
    };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return {
      status: 500,
      body: envelopeFor(deps, envelopeName, { error: message }, now() - start, { error: true }),
    };
  }
}
