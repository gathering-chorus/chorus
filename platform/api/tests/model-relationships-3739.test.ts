// @test-type: unit — injects a fake fetch; no Fuseki, no live service, brings its own world.
// #3739 — the model's LINES, induced. Class-level edges (rdfs:domain × rdfs:range,
// chorus-namespace ranges only) join through definesVocabulary to domain-level
// edges. An isolated domain (vocabulary, zero edges) is a SERVED finding, and a
// store failure must never render as "no relationships".
import { induceModel, modelRelationshipsHandler } from '../src/handlers/athena-model-relationships';

const CH = 'https://jeffbridwell.com/chorus#';
const b = (v: string) => ({ type: 'uri', value: v });

function edgeRow(prop: string, from: string, to: string, fromDomain?: string, toDomain?: string) {
  const row: any = { prop: b(CH + prop), a: b(CH + from), b: b(CH + to) };
  if (fromDomain) row.da = b(CH + fromDomain);
  if (toDomain) row.db = b(CH + toDomain);
  return row;
}
const vocabRow = (domain: string, cls: string) => ({ d: b(CH + domain), c: b(CH + cls) });

describe('induceModel', () => {
  it('joins class edges through definesVocabulary into counted domain edges', () => {
    const edges = [
      edgeRow('hasVerbRun', 'Card', 'VerbRun', 'cards', 'werk'),
      edgeRow('hasWitness', 'Card', 'Witness', 'cards', 'werk'),
      edgeRow('gatekeeper', 'Gate', 'Role', 'gates', 'roles'),
    ];
    const vocab = [vocabRow('cards', 'Card'), vocabRow('werk', 'VerbRun'), vocabRow('werk', 'Witness'),
      vocabRow('gates', 'Gate'), vocabRow('roles', 'Role')];
    const m = induceModel(edges, vocab);
    expect(m.classEdges).toHaveLength(3);
    expect(m.classEdges[0]).toEqual(
      expect.objectContaining({ property: 'hasVerbRun', fromClass: 'Card', toClass: 'VerbRun',
        fromDomain: 'cards', toDomain: 'werk' }));
    const cw = m.domainEdges.find(e => e.from === 'cards' && e.to === 'werk');
    expect(cw).toBeDefined();
    expect(cw!.count).toBe(2);
    expect(cw!.properties).toEqual(expect.arrayContaining(['hasVerbRun', 'hasWitness']));
    expect(m.domainEdges.find(e => e.from === 'gates' && e.to === 'roles')!.count).toBe(1);
  });

  it('names isolated domains instead of hiding them (AC3)', () => {
    const edges = [edgeRow('gatekeeper', 'Gate', 'Role', 'gates', 'roles')];
    const vocab = [vocabRow('gates', 'Gate'), vocabRow('roles', 'Role'), vocabRow('principles', 'Principle')];
    const m = induceModel(edges, vocab);
    expect(m.isolatedDomains).toEqual(['principles']);
    expect(m.domains).toEqual(expect.arrayContaining(['gates', 'roles', 'principles']));
    // the drill needs domain → owned classes (AC4)
    expect(m.vocabulary).toEqual({ gates: ['Gate'], roles: ['Role'], principles: ['Principle'] });
  });

  it('keeps an edge whose classes have no owning domain as a class edge, induces no domain edge', () => {
    const edges = [edgeRow('feedsInto', 'Vertebra', 'Vertebra')];
    const m = induceModel(edges, []);
    expect(m.classEdges).toHaveLength(1);
    expect(m.classEdges[0].fromDomain).toBeUndefined();
    expect(m.domainEdges).toHaveLength(0);
  });

  it('a self-referencing domain edge (spine ↔ events shape) keeps direction — one edge each way', () => {
    const edges = [
      edgeRow('emits', 'Vertebra', 'Event', 'spine', 'events'),
      edgeRow('recordedOn', 'Event', 'Vertebra', 'events', 'spine'),
    ];
    const vocab = [vocabRow('spine', 'Vertebra'), vocabRow('events', 'Event')];
    const m = induceModel(edges, vocab);
    expect(m.domainEdges).toHaveLength(2);
    expect(m.domainEdges.map(e => `${e.from}->${e.to}`).sort())
      .toEqual(['events->spine', 'spine->events']);
  });
});

describe('modelRelationshipsHandler', () => {
  const okSparql = (bindings: any[]) => ({
    ok: true, status: 200,
    json: async () => ({ results: { bindings } }),
  }) as unknown as Response;

  function run(handler: any) {
    const res: any = {
      statusCode: 200, body: undefined,
      status(c: number) { this.statusCode = c; return this; },
      json(v: any) { this.body = v; return this; },
    };
    return handler({} as any, res).then(() => res);
  }

  it('serves domainEdges + classEdges + isolatedDomains from the store', async () => {
    const fetchFn = jest.fn()
      .mockResolvedValueOnce(okSparql([edgeRow('gatekeeper', 'Gate', 'Role', 'gates', 'roles')]))
      .mockResolvedValueOnce(okSparql([vocabRow('gates', 'Gate'), vocabRow('roles', 'Role')]));
    const res = await run(modelRelationshipsHandler({ fetchFn }));
    expect(res.statusCode).toBe(200);
    expect(res.body.storeReachable).toBe(true);
    expect(res.body.domainEdges).toHaveLength(1);
    expect(res.body.classEdges).toHaveLength(1);
    expect(res.body.isolatedDomains).toEqual([]);
  });

  it('NEGATIVE PROOF: a store failure serves 502 storeReachable:false, never an empty edge set', async () => {
    const fetchFn = jest.fn().mockResolvedValue({ ok: false, status: 500 } as unknown as Response);
    const res = await run(modelRelationshipsHandler({ fetchFn }));
    expect(res.statusCode).toBe(502);
    expect(res.body.storeReachable).toBe(false);
    expect(res.body.domainEdges).toBeUndefined();
  });

  it('NEGATIVE PROOF: an unreachable store (thrown fetch) is 502, not an empty model', async () => {
    const fetchFn = jest.fn().mockRejectedValue(new Error('ECONNREFUSED'));
    const res = await run(modelRelationshipsHandler({ fetchFn }));
    expect(res.statusCode).toBe(502);
    expect(res.body.storeReachable).toBe(false);
  });
});
