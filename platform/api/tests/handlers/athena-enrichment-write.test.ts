/**
 * #2206 — envelope enrichment write endpoints.
 *
 * Each POST handler accepts a body, writes the triple to Fuseki via
 * injected sparqlUpdate, AND appends the triple to a durable seed file
 * via injected appendSeed. Both side effects together = write survives
 * Fuseki rebuild.
 *
 * Red-first tests — handler doesn't exist yet.
 */
import {
  fetchAthenaServiceDescription,
  fetchAthenaPersistenceDescription,
  fetchAthenaServiceEdge,
  type AthenaEnrichmentDeps,
} from '../../src/handlers/athena-enrichment-write';

function makeDeps(over: Partial<AthenaEnrichmentDeps> = {}): AthenaEnrichmentDeps & {
  updates: string[];
  seeds: string[];
} {
  const updates: string[] = [];
  const seeds: string[] = [];
  return {
    sparqlUpdate: async (update: string) => { updates.push(update); },
    appendSeed: (triple: string) => { seeds.push(triple); },
    now: () => 1_000_000,
    updates,
    seeds,
    ...over,
  };
}

describe('#2206 POST /api/athena/subdomains/:id/services/:entityId/description', () => {
  test('200 on valid body; writes INSERT to Fuseki AND appends triple to seed', async () => {
    const deps = makeDeps();
    const r = await fetchAthenaServiceDescription(
      deps,
      { subdomainId: 'chorus-domain', entityId: 'pulse', body: { description: 'Pulse writes pulse-latest.json.' } },
    );
    expect(r.status).toBe(200);

    expect(deps.updates).toHaveLength(1);
    expect(deps.updates[0]).toContain('chorus-domain-service-pulse');
    expect(deps.updates[0]).toContain('rdfs:comment');
    expect(deps.updates[0]).toContain('Pulse writes pulse-latest.json.');

    expect(deps.seeds).toHaveLength(1);
    expect(deps.seeds[0]).toContain('chorus-domain-service-pulse');
    expect(deps.seeds[0]).toContain('Pulse writes pulse-latest.json.');
  });

  test('400 when body.description missing', async () => {
    const deps = makeDeps();
    const r = await fetchAthenaServiceDescription(
      deps,
      { subdomainId: 'chorus-domain', entityId: 'pulse', body: {} as { description: string } },
    );
    expect(r.status).toBe(400);
    expect(deps.updates).toHaveLength(0);
    expect(deps.seeds).toHaveLength(0);
  });

  test('400 when entityId contains illegal chars', async () => {
    const deps = makeDeps();
    const r = await fetchAthenaServiceDescription(
      deps,
      { subdomainId: 'chorus-domain', entityId: '../evil', body: { description: 'x' } },
    );
    expect(r.status).toBe(400);
    expect(deps.updates).toHaveLength(0);
  });

  test('description with quotes escapes properly in SPARQL', async () => {
    const deps = makeDeps();
    await fetchAthenaServiceDescription(
      deps,
      { subdomainId: 'chorus-domain', entityId: 'pulse', body: { description: 'He said "hello" today.' } },
    );
    expect(deps.updates[0]).toContain('\\"hello\\"');
  });

  test('sparqlUpdate throws → 500, seed not written', async () => {
    const deps = makeDeps({ sparqlUpdate: async () => { throw new Error('fuseki down'); } });
    const r = await fetchAthenaServiceDescription(
      deps,
      { subdomainId: 'chorus-domain', entityId: 'pulse', body: { description: 'x' } },
    );
    expect(r.status).toBe(500);
    expect(deps.seeds).toHaveLength(0);
  });
});

describe('#2206 POST /api/athena/subdomains/:id/persistence/:entityId/description', () => {
  test('writes to Persistence entity URI + seed', async () => {
    const deps = makeDeps();
    const r = await fetchAthenaPersistenceDescription(
      deps,
      { subdomainId: 'chorus-domain', entityId: 'pulse-latest-json', body: { description: 'Latest pulse snapshot.' } },
    );
    expect(r.status).toBe(200);
    expect(deps.updates[0]).toContain('chorus-domain-store-pulse-latest-json');
    expect(deps.seeds[0]).toContain('chorus-domain-store-pulse-latest-json');
  });
});

describe('#2206 POST /api/athena/subdomains/:id/services/:entityId/{reads|writes|consumes}', () => {
  test('reads edge: service → persistence target', async () => {
    const deps = makeDeps();
    const r = await fetchAthenaServiceEdge(
      deps,
      {
        subdomainId: 'chorus-domain',
        entityId: 'pulse',
        predicate: 'reads',
        body: { target: 'role-declared-json' },
      },
    );
    expect(r.status).toBe(200);
    expect(deps.updates[0]).toContain('chorus-domain-service-pulse');
    expect(deps.updates[0]).toContain('chorus:reads');
    expect(deps.updates[0]).toContain('chorus-domain-store-role-declared-json');
  });

  test('writes edge uses chorus:writes predicate', async () => {
    const deps = makeDeps();
    await fetchAthenaServiceEdge(
      deps,
      {
        subdomainId: 'chorus-domain',
        entityId: 'pulse',
        predicate: 'writes',
        body: { target: 'pulse-latest-json' },
      },
    );
    expect(deps.updates[0]).toContain('chorus:writes');
  });

  test('consumes edge targets Service (not Persistence)', async () => {
    const deps = makeDeps();
    await fetchAthenaServiceEdge(
      deps,
      {
        subdomainId: 'chorus-domain',
        entityId: 'chorus-api',
        predicate: 'consumes',
        body: { target: 'pulse' },
      },
    );
    expect(deps.updates[0]).toContain('chorus:consumes');
    // consumes target is another Service, not a Persistence store
    expect(deps.updates[0]).toContain('chorus-domain-service-pulse');
  });

  test('invalid predicate → 400', async () => {
    const deps = makeDeps();
    const r = await fetchAthenaServiceEdge(
      deps,
      {
        subdomainId: 'chorus-domain',
        entityId: 'pulse',
        predicate: 'destroys' as 'reads' | 'writes' | 'consumes',
        body: { target: 'x' },
      },
    );
    expect(r.status).toBe(400);
    expect(deps.updates).toHaveLength(0);
  });

  test('missing body.target → 400', async () => {
    const deps = makeDeps();
    const r = await fetchAthenaServiceEdge(
      deps,
      { subdomainId: 'chorus-domain', entityId: 'pulse', predicate: 'reads', body: {} as { target: string } },
    );
    expect(r.status).toBe(400);
  });
});
