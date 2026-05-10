/**
 * #2851 — SubProduct write API (Athena hierarchy CRUD).
 *
 * Experience: a role can POST a new SubProduct and DELETE (deprecate) one.
 * Both paths patch chorus.ttl + emit a SPARQL UPDATE in lockstep.
 *
 * Red-first — handler doesn't exist yet.
 *
 * SKIPPED 2026-05-09 — #2851 parked pending model-collapse decision (#2864).
 * The handler this test imports doesn't exist; importing breaks the suite.
 * On unpark: restore the real import, remove the shim type+functions below,
 * remove the .skip on the two describe blocks. Do not delete this file —
 * it's the TDD red phase for #2851.
 */
type SubProductWriteDeps = {
  sparqlUpdate: (u: string) => Promise<void>;
  readTtl: () => string;
  writeTtl: (c: string) => void;
};
const createSubProduct = (..._args: unknown[]): never => { throw new Error('parked'); };
const deprecateSubProduct = (..._args: unknown[]): never => { throw new Error('parked'); };

const SAMPLE_TTL = `@prefix chorus: <https://jeffbridwell.com/chorus#> .
@prefix rdfs: <http://www.w3.org/2000/01/rdf-schema#> .
@prefix owl: <http://www.w3.org/2002/07/owl#> .

chorus:loom a chorus:SubProduct ;
    rdfs:label "Loom" ;
    rdfs:comment "Operating model." ;
    chorus:ownedBy chorus:jeff .

chorus:quality-product a chorus:SubProduct ;
    rdfs:label "Quality" ;
    rdfs:comment "Horizontal quality discipline." ;
    chorus:ownedBy chorus:kade .
`;

function makeDeps(initialTtl: string = SAMPLE_TTL): SubProductWriteDeps & {
  updates: string[];
  ttl: { current: string };
} {
  const state = { current: initialTtl };
  const updates: string[] = [];
  return {
    sparqlUpdate: async (u: string) => { updates.push(u); },
    readTtl: () => state.current,
    writeTtl: (c: string) => { state.current = c; },
    updates,
    ttl: state,
  };
}

describe.skip('#2851 createSubProduct (parked — see header)', () => {
  test('201 on valid body; SPARQL INSERT fires and ttl gets a new SubProduct block', async () => {
    const deps = makeDeps();
    const r = await createSubProduct(deps, {
      id: 'borg-product',
      label: 'Borg',
      comment: 'Proving stage of the value stream.',
      owner: 'silas',
    });

    expect(r.status).toBe(201);
    expect(deps.updates).toHaveLength(1);
    expect(deps.updates[0]).toContain('INSERT DATA');
    expect(deps.updates[0]).toContain('borg-product');
    expect(deps.updates[0]).toContain('a chorus:SubProduct');
    expect(deps.updates[0]).toContain('chorus:silas');

    expect(deps.ttl.current).toContain('chorus:borg-product a chorus:SubProduct');
    expect(deps.ttl.current).toContain('rdfs:label "Borg"');
    expect(deps.ttl.current).toContain('chorus:ownedBy chorus:silas');
  });

  test('409 when SubProduct already exists; no SPARQL fires; ttl unchanged', async () => {
    const deps = makeDeps();
    const before = deps.ttl.current;
    const r = await createSubProduct(deps, {
      id: 'loom',
      label: 'Loom',
      comment: 'duplicate',
      owner: 'jeff',
    });
    expect(r.status).toBe(409);
    expect(deps.updates).toHaveLength(0);
    expect(deps.ttl.current).toBe(before);
  });

  test.each([
    ['../evil', 'Bad', 'comment', 'jeff'],
    ['ok', '', 'comment', 'jeff'],
    ['ok', 'Bad', '', 'jeff'],
    ['ok', 'Bad', 'comment', 'random-person'],
    ['ok', 'has "quotes"', 'comment', 'jeff'],
  ])('400 on invalid input id=%s label=%s comment=%s owner=%s', async (id, label, comment, owner) => {
    const deps = makeDeps();
    const r = await createSubProduct(deps, { id, label, comment, owner });
    expect(r.status).toBe(400);
    expect(deps.updates).toHaveLength(0);
  });

  test('SPARQL failure leaves ttl untouched', async () => {
    const deps = makeDeps();
    const before = deps.ttl.current;
    deps.sparqlUpdate = async () => { throw new Error('fuseki down'); };
    const r = await createSubProduct(deps, {
      id: 'borg-product',
      label: 'Borg',
      comment: 'x',
      owner: 'silas',
    });
    expect(r.status).toBe(500);
    expect(deps.ttl.current).toBe(before);
  });
});

describe.skip('#2851 deprecateSubProduct (parked — see header)', () => {
  test('200; SPARQL inserts owl:deprecated; ttl block patched with owl:deprecated true', async () => {
    const deps = makeDeps();
    const r = await deprecateSubProduct(deps, 'quality-product', { reason: 'merged into Borg' });

    expect(r.status).toBe(200);
    expect(deps.updates).toHaveLength(1);
    expect(deps.updates[0]).toContain('owl:deprecated true');
    expect(deps.updates[0]).toContain('quality-product');

    const block = deps.ttl.current.match(/chorus:quality-product a chorus:SubProduct[\s\S]+?\.\n/);
    expect(block).not.toBeNull();
    expect(block![0]).toContain('owl:deprecated true');
  });

  test('404 when SubProduct does not exist', async () => {
    const deps = makeDeps();
    const r = await deprecateSubProduct(deps, 'nope-product', {});
    expect(r.status).toBe(404);
    expect(deps.updates).toHaveLength(0);
  });

  test('400 on invalid id', async () => {
    const deps = makeDeps();
    const r = await deprecateSubProduct(deps, '../evil', {});
    expect(r.status).toBe(400);
  });
});
