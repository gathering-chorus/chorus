import { handleIcdFieldUpsert, handleIcdMappingUpsert, handleIcdSectionPut } from '../src/icd-writes';

function fakeRes() {
  const self: any = { status_: 200, body_: null };
  self.status = (s: number) => { self.status_ = s; return self; };
  self.json = (b: any) => { self.body_ = b; return self; };
  return self;
}

function makeDeps(overrides: any = {}) {
  const updates: string[] = [];
  const queries: string[] = [];
  return {
    state: { updates, queries },
    deps: {
      resolveDomain: jest.fn(async (id: string) => id === 'missing' ? null : `urn:domain:${id}`),
      client: {
        query: jest.fn(async (q: string) => {
          queries.push(q);
          // By default — field does not exist (isNew=true).
          return overrides.fieldExists ? { results: { bindings: [{ f: { value: 'x' } }] } } : { results: { bindings: [] } };
        }),
        update: jest.fn(async (u: string) => { updates.push(u); }),
      },
      pfx: 'PREFIX icd:',
      graph: 'urn:icd:current',
      icdSlug: (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, ''),
      escSparql: (s: string) => s.replace(/"/g, '\\"'),
    },
  };
}

describe('handleIcdFieldUpsert', () => {
  it('400s when name is missing', async () => {
    const { deps } = makeDeps();
    const res = fakeRes();
    await handleIcdFieldUpsert(
      { params: { id: 'd' }, body: { severity: 'warning' } } as any,
      res,
      deps,
    );
    expect(res.status_).toBe(400);
    expect(res.body_.error).toMatch(/name and severity/);
  });

  it('400s when severity is missing', async () => {
    const { deps } = makeDeps();
    const res = fakeRes();
    await handleIcdFieldUpsert(
      { params: { id: 'd' }, body: { name: 'title' } } as any,
      res,
      deps,
    );
    expect(res.status_).toBe(400);
  });

  it('400s when severity is invalid', async () => {
    const { deps } = makeDeps();
    const res = fakeRes();
    await handleIcdFieldUpsert(
      { params: { id: 'd' }, body: { name: 'title', severity: 'banana' } } as any,
      res,
      deps,
    );
    expect(res.status_).toBe(400);
    expect(res.body_.error).toMatch(/severity must be one of/);
  });

  it('404s when domain cannot be resolved', async () => {
    const { deps } = makeDeps();
    const res = fakeRes();
    await handleIcdFieldUpsert(
      { params: { id: 'missing' }, body: { name: 'title', severity: 'warning' } } as any,
      res,
      deps,
    );
    expect(res.status_).toBe(404);
    expect(res.body_.error).toMatch(/Domain 'missing' not found/);
  });

  it('responds 201 + created=true when field is new', async () => {
    const { deps, state } = makeDeps();
    const res = fakeRes();
    await handleIcdFieldUpsert(
      { params: { id: 'chorus' }, body: { name: 'severity_label', severity: 'warning' } } as any,
      res,
      deps,
    );
    expect(res.status_).toBe(201);
    expect(res.body_.created).toBe(true);
    expect(res.body_.field).toBe('severity_label');
    expect(state.updates).toHaveLength(1);
  });

  it('responds 200 + created=false when field already exists', async () => {
    const { deps } = makeDeps({ fieldExists: true });
    const res = fakeRes();
    await handleIcdFieldUpsert(
      { params: { id: 'chorus' }, body: { name: 'severity_label', severity: 'warning' } } as any,
      res,
      deps,
    );
    expect(res.status_).toBe(200);
    expect(res.body_.created).toBe(false);
  });

  it('interpolates escaped name + severity mapping into the SPARQL update', async () => {
    const { deps, state } = makeDeps();
    const res = fakeRes();
    await handleIcdFieldUpsert(
      {
        params: { id: 'chorus' },
        body: { name: 'tricky "value"', severity: 'enrichment', datatype: 'xsd:decimal', cardinality: '0..*', order: 3 },
      } as any,
      res,
      deps,
    );
    const update = state.updates[0];
    expect(update).toContain('icd:Enrichment');
    expect(update).toContain('icd:canonicalName "tricky \\"value\\""');
    expect(update).toContain('xsd:decimal');
    expect(update).toContain('icd:fieldOrder 3');
  });

  it('includes optional triples only when their inputs are provided', async () => {
    const { deps, state } = makeDeps();
    const res = fakeRes();
    await handleIcdFieldUpsert(
      { params: { id: 'chorus' }, body: { name: 'x', severity: 'info', constraint: 'c', description: 'd' } } as any,
      res,
      deps,
    );
    const update = state.updates[0];
    expect(update).toContain('icd:constraint');
    expect(update).toContain('icd:fieldTypeDescription');
    // bestSource omitted
    expect(update).not.toContain('icd:bestSource');
  });

  it('500s with detail when sparql update throws', async () => {
    const { deps } = makeDeps();
    deps.client.update = jest.fn(async () => { throw new Error('graph locked'); });
    const res = fakeRes();
    await handleIcdFieldUpsert(
      { params: { id: 'chorus' }, body: { name: 'x', severity: 'info' } } as any,
      res,
      deps,
    );
    expect(res.status_).toBe(500);
    expect(res.body_.error).toBe('Failed to upsert ICD field');
    expect(res.body_.detail).toContain('graph locked');
  });
});

describe('handleIcdMappingUpsert', () => {
  function mappingDeps(overrides: any = {}) {
    const updates: string[] = [];
    const d = {
      resolveDomain: jest.fn(async (id: string) => id === 'missing' ? null : `urn:domain:${id}`),
      client: {
        query: jest.fn(async (q: string) => {
          // Provider existence check precedes mapping existence check.
          if (q.includes('icd:Provider')) {
            return overrides.providerMissing ? { results: { bindings: [] } } : { results: { bindings: [{ p: { value: 'x' } }] } };
          }
          return overrides.mappingExists ? { results: { bindings: [{ m: { value: 'x' } }] } } : { results: { bindings: [] } };
        }),
        update: jest.fn(async (u: string) => { updates.push(u); }),
      },
      pfx: 'PREFIX icd:', graph: 'urn:icd:current',
      icdSlug: (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, ''),
      escSparql: (s: string) => s.replace(/"/g, '\\"'),
    };
    return { deps: d, updates };
  }

  it('400s when required fields missing', async () => {
    const { deps } = mappingDeps();
    const res = fakeRes();
    await handleIcdMappingUpsert(
      { params: { id: 'd' }, body: { providerId: 'p' } } as any, res, deps,
    );
    expect(res.status_).toBe(400);
  });

  it('404 when provider not found', async () => {
    const { deps } = mappingDeps({ providerMissing: true });
    const res = fakeRes();
    await handleIcdMappingUpsert(
      { params: { id: 'chorus' }, body: { providerId: 'nope', sourceField: 's', mapsTo: 'm', confidence: '0.9' } } as any,
      res, deps,
    );
    expect(res.status_).toBe(404);
    expect(res.body_.error).toMatch(/Provider 'nope' not found/);
  });

  it('201 when new mapping, 200 when existing', async () => {
    const { deps: d1 } = mappingDeps();
    const res1 = fakeRes();
    await handleIcdMappingUpsert(
      { params: { id: 'chorus' }, body: { providerId: 'p', sourceField: 's', mapsTo: 'm', confidence: '0.9' } } as any,
      res1, d1,
    );
    expect(res1.status_).toBe(201);

    const { deps: d2 } = mappingDeps({ mappingExists: true });
    const res2 = fakeRes();
    await handleIcdMappingUpsert(
      { params: { id: 'chorus' }, body: { providerId: 'p', sourceField: 's', mapsTo: 'm', confidence: '0.9' } } as any,
      res2, d2,
    );
    expect(res2.status_).toBe(200);
  });

  it('truncates mapsTo at first comma for the field slug', async () => {
    const { deps, updates } = mappingDeps();
    const res = fakeRes();
    await handleIcdMappingUpsert(
      { params: { id: 'chorus' }, body: { providerId: 'p', sourceField: 's', mapsTo: 'primary,fallback', confidence: '0.9' } } as any,
      res, deps,
    );
    expect(updates[0]).toContain('/field/chorus/primary');
    expect(updates[0]).not.toContain('/field/chorus/primary,fallback');
  });

  it('500s with detail on update throw', async () => {
    const { deps } = mappingDeps();
    deps.client.update = jest.fn(async () => { throw new Error('blocked'); });
    const res = fakeRes();
    await handleIcdMappingUpsert(
      { params: { id: 'chorus' }, body: { providerId: 'p', sourceField: 's', mapsTo: 'm', confidence: '0.9' } } as any,
      res, deps,
    );
    expect(res.status_).toBe(500);
    expect(res.body_.detail).toContain('blocked');
  });
});

describe('handleIcdSectionPut', () => {
  function sectionDeps(overrides: any = {}) {
    const updates: string[] = [];
    return {
      updates,
      deps: {
        resolveDomain: jest.fn(async (id: string) => id === 'missing' ? null : `urn:domain:${id}`),
        client: {
          query: jest.fn(async (q: string) => {
            if (q.includes('icd:Provider')) {
              return overrides.providerMissing ? { results: { bindings: [] } } : { results: { bindings: [{ p: { value: 'x' } }] } };
            }
            return { results: { bindings: [] } };
          }),
          update: jest.fn(async (u: string) => { updates.push(u); }),
        },
        pfx: 'PFX', graph: 'G',
        icdSlug: (s: string) => s.toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-|-$/g, ''),
        escSparql: (s: string) => s.replace(/"/g, '\\"'),
      },
    };
  }

  it('400 when title missing', async () => {
    const { deps } = sectionDeps();
    const res = fakeRes();
    await handleIcdSectionPut(
      { params: { id: 'd', pid: 'p' }, body: {} } as any, res, deps,
    );
    expect(res.status_).toBe(400);
  });

  it('404 when domain missing', async () => {
    const { deps } = sectionDeps();
    const res = fakeRes();
    await handleIcdSectionPut(
      { params: { id: 'missing', pid: 'p' }, body: { title: 'T' } } as any, res, deps,
    );
    expect(res.status_).toBe(404);
  });

  it('404 when provider missing', async () => {
    const { deps } = sectionDeps({ providerMissing: true });
    const res = fakeRes();
    await handleIcdSectionPut(
      { params: { id: 'd', pid: 'p' }, body: { title: 'T' } } as any, res, deps,
    );
    expect(res.status_).toBe(404);
  });

  it('ok with paragraphs, risks, nonFunctionals, and mermaid in one shot', async () => {
    const { deps, updates } = sectionDeps();
    const res = fakeRes();
    await handleIcdSectionPut(
      {
        params: { id: 'd', pid: 'p' },
        body: {
          title: 'Overview',
          paragraphs: ['first', 'second'],
          risks: [{ status: 'open', text: 'r1' }],
          nonFunctionals: { volume: 'high', freshness: '1h', latency: '50ms', auth: 'none' },
          mermaid: 'graph TD;',
        },
      } as any, res, deps,
    );
    expect(res.body_.ok).toBe(true);
    // Two update calls: DELETE-block + INSERT-block.
    expect(updates).toHaveLength(2);
    const insert = updates[1];
    expect(insert).toContain('icd:hasParagraph');
    expect(insert).toContain('icd:hasRiskItem');
    expect(insert).toContain('icd:nfVolume');
    expect(insert).toContain('icd:mermaidSource');
  });

  it('section type defaults to "content" when absent', async () => {
    const { deps, updates } = sectionDeps();
    const res = fakeRes();
    await handleIcdSectionPut(
      { params: { id: 'd', pid: 'p' }, body: { title: 'T' } } as any, res, deps,
    );
    expect(updates[1]).toContain('icd:sectionType "content"');
  });
});
