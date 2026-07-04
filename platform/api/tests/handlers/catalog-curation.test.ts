// @test-type: unit — fake sparqlQuery/sparqlUpdate/envelope deps; no Fuseki, no live services.
//
// #3606 — catalog-curation.ts sat at 10.7% covered (191 uncovered statements,
// #2 handler gap behind the platform/api coverage red). The module is fully
// dependency-injected by design (#2549: "pure functions + DI so tests run
// without Fuseki") but only an integration path exercised it. Real behavior
// tests: validation, SPARQL construction, spine emits, read-side folds.
import {
  validateTags,
  decodeHrefId,
  writeCatalogTags,
  writeCatalogLineage,
  readCatalogDoc,
  readCatalogAudit,
  readCatalogCurated,
  readCatalogDrift,
  type CurationDeps,
} from '../../src/handlers/catalog-curation';

function b64url(s: string): string {
  return Buffer.from(s, 'utf8').toString('base64url');
}

interface FakeDeps extends CurationDeps {
  updates: string[];
  queries: string[];
  spine: Array<{ event: string; fields: Record<string, string> }>;
  queryResults: Array<{ results: { bindings: unknown[] } }>;
}

function fakeDeps(): FakeDeps {
  const d: FakeDeps = {
    updates: [],
    queries: [],
    spine: [],
    queryResults: [],
    sparqlUpdate: async (q: string) => { d.updates.push(q); return {}; },
    sparqlQuery: async (q: string) => {
      d.queries.push(q);
      return d.queryResults.shift() ?? { results: { bindings: [] } };
    },
    envelope: (name, data, _ms, extra) => ({ name, data, ...(extra ?? {}) }),
    emitSpine: (event, fields) => { d.spine.push({ event, fields }); },
    now: () => new Date('2026-07-03T12:00:00.000Z'),
  };
  return d;
}

describe('validateTags', () => {
  it('requires an object body with href', () => {
    expect(validateTags(null).ok).toBe(false);
    expect(validateTags({}).ok).toBe(false);
  });

  it('rejects unknown vocabulary values with the field named', () => {
    const r = validateTags({ href: '/d', product: 'nonsense' });
    expect(r).toEqual({ ok: false, error: 'unknown product: nonsense' });
  });

  it('accepts valid five-field tags and free-form domain', () => {
    const r = validateTags({ href: '/d', product: 'chorus', subproduct: 'werk', role: 'kade', domain: 'anything-goes' });
    expect(r).toMatchObject({ ok: true, tags: { href: '/d', product: 'chorus', subproduct: 'werk', role: 'kade', domain: 'anything-goes' } });
  });

  it('rejects a non-string domain', () => {
    expect(validateTags({ href: '/d', domain: 7 }).ok).toBe(false);
  });
});

describe('decodeHrefId', () => {
  it('round-trips a base64url href id', () => {
    expect(decodeHrefId(b64url('/designing/docs/x.html'))).toBe('/designing/docs/x.html');
  });
});

describe('writeCatalogTags', () => {
  it('400 envelope on validation failure, no sparql issued', async () => {
    const d = fakeDeps();
    const r = await writeCatalogTags(d, { href: '/d', role: 'nobody' });
    expect(r.status).toBe(400);
    expect(d.updates).toHaveLength(0);
  });

  it('200 writes an upsert naming the graph, the tags, and curatedAt; emits spine', async () => {
    const d = fakeDeps();
    const r = await writeCatalogTags(d, { href: '/docs/a.md', product: 'chorus', subproduct: 'athena' });
    expect(r.status).toBe(200);
    expect(d.updates).toHaveLength(1);
    const u = d.updates[0];
    expect(u).toContain('urn:chorus:instances');
    expect(u).toContain('"chorus"');
    expect(u).toContain('"athena"');
    expect(u).toContain('2026-07-03T12:00:00.000Z');
    expect(d.spine).toEqual([
      { event: 'catalog.tag.curated', fields: { href: '/docs/a.md', after: expect.stringContaining('product=chorus') } },
    ]);
  });
});

describe('writeCatalogLineage', () => {
  it('400 on missing fields and unknown predicate', async () => {
    const d = fakeDeps();
    expect((await writeCatalogLineage(d, {})).status).toBe(400);
    expect((await writeCatalogLineage(d, { subject_href: '/a', object_href: '/b', predicate: 'inspiredBy' })).status).toBe(400);
    expect(d.updates).toHaveLength(0);
  });

  it('200 inserts the edge between the two doc URIs and emits spine', async () => {
    const d = fakeDeps();
    const r = await writeCatalogLineage(d, { subject_href: '/new.html', object_href: '/old.html', predicate: 'supersedes' });
    expect(r.status).toBe(200);
    expect(d.updates[0]).toContain(`catalog-doc-${b64url('/new.html')}`);
    expect(d.updates[0]).toContain(`catalog-doc-${b64url('/old.html')}`);
    expect(d.updates[0]).toContain('chorus:supersedes');
    expect(d.spine[0].event).toBe('catalog.lineage.linked');
  });
});

describe('readCatalogDoc', () => {
  it('400 on undecodable id', async () => {
    const d = fakeDeps();
    // base64url alphabet can't contain '!'
    const r = await readCatalogDoc(d, '!!!');
    expect([400, 404]).toContain(r.status);
  });

  it('404 when the doc has no CatalogDoc row', async () => {
    const d = fakeDeps();
    d.queryResults.push({ results: { bindings: [] } });
    const r = await readCatalogDoc(d, b64url('/nope.md'));
    expect(r.status).toBe(404);
  });

  it('200 folds tags + bidirectional lineage from the two queries', async () => {
    const d = fakeDeps();
    const uriOf = (href: string) => `https://jeffbridwell.com/chorus#catalog-doc-${b64url(href)}`;
    d.queryResults.push({
      results: { bindings: [{ product: { value: 'chorus' }, subdomain: { value: 'tests-domain' } }] },
    });
    d.queryResults.push({
      results: {
        bindings: [
          { direction: { value: 'out' }, predicate: { value: 'https://jeffbridwell.com/chorus#supersedes' }, other: { value: uriOf('/old.html') } },
          { direction: { value: 'in' }, predicate: { value: 'https://jeffbridwell.com/chorus#derivedFrom' }, other: { value: uriOf('/parent.html') } },
          { direction: { value: 'in' }, predicate: { value: 'https://jeffbridwell.com/chorus#unknownPred' }, other: { value: uriOf('/x.html') } },
        ],
      },
    });
    const r = await readCatalogDoc(d, b64url('/doc.html'));
    expect(r.status).toBe(200);
    const detail = (r.body as { data: { href: string; tags: { product?: string }; lineage: { in: unknown[]; out: unknown[] } } }).data;
    expect(detail.href).toBe('/doc.html');
    expect(detail.tags.product).toBe('chorus');
    expect(detail.lineage.out).toEqual([{ subject_href: '/doc.html', predicate: 'supersedes', object_href: '/old.html' }]);
    expect(detail.lineage.in).toEqual([{ subject_href: '/parent.html', predicate: 'derivedFrom', object_href: '/doc.html' }]);
  });
});

describe('readCatalogAudit', () => {
  it('returns events from the injected reader', async () => {
    const events = [{ timestamp: 't', event: 'catalog.tag.curated', role: 'kade', fields: { href: '/d' } }];
    const r = await readCatalogAudit(
      { readEvents: async () => events, envelope: (name, data) => ({ name, data }) },
      b64url('/d'),
    );
    expect(r.status).toBe(200);
    expect((r.body as { data: { events: unknown[] } }).data.events).toBe(events);
  });
});

describe('readCatalogCurated', () => {
  it('lists curated docs keyed by href, skipping rows without one', async () => {
    const d = fakeDeps();
    d.queryResults.push({
      results: {
        bindings: [
          { href: { value: '/a.md' }, product: { value: 'gathering' } },
          { product: { value: 'chorus' } }, // no href — dropped
        ],
      },
    });
    const r = await readCatalogCurated(d);
    const curated = (r.body as { data: { curated: Array<{ href: string }> } }).data.curated;
    expect(curated).toHaveLength(1);
    expect(curated[0].href).toBe('/a.md');
  });
});

describe('readCatalogDrift', () => {
  it('reports only docs whose persisted tags diverge from path-implied tags', async () => {
    const d = fakeDeps();
    d.queryResults.push({
      results: {
        bindings: [
          // service-design-* filename implies product=chorus (doc-tagger filename
          // signal, verified); persisted says gathering → divergent
          { href: { value: '/designing/docs/service-design-werk.html' }, product: { value: 'gathering' } },
          // agreeing row → not drift
          { href: { value: '/designing/docs/service-design-loom.html' }, product: { value: 'chorus' } },
          // no path signal at all → never drift
          { href: { value: '/roles/wren/artifacts/plan.md' }, product: { value: 'gathering' } },
        ],
      },
    });
    const r = await readCatalogDrift(d);
    const drift = (r.body as { data: { drift: Array<{ href: string; divergence_fields: string[] }> } }).data.drift;
    expect(drift).toHaveLength(1);
    expect(drift[0].href).toBe('/designing/docs/service-design-werk.html');
    expect(drift[0].divergence_fields).toContain('product');
  });
});

describe('readCatalogAudit — invalid id (#3606)', () => {
  it('400 on an empty hrefb64 (decodes to nothing), reader never called', async () => {
    let called = 0;
    const r = await readCatalogAudit(
      { readEvents: async () => { called++; return []; }, envelope: (name, data, _ms, extra) => ({ name, data, ...(extra ?? {}) }) },
      '',
    );
    expect(r.status).toBe(400);
    expect(called).toBe(0);
  });
});
