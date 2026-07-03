// @test-type: unit — fake sparqlUpdate/readTtl/writeTtl deps; no Fuseki, no disk, brings its own world.
//
// #3606 — athena-owner-write.ts sat at 7.4% covered (87 uncovered statements).
// The handler's whole point is its ORDERING contract (patch in memory → SPARQL
// first → TTL write only on SPARQL success) — exactly the kind of behavior
// unit tests must pin, and nothing did.
import {
  findBlockTerminator,
  patchTtlOwner,
  setSubdomainOwner,
  type AthenaOwnerWriteDeps,
} from '../../src/handlers/athena-owner-write';

const TTL = `@prefix chorus: <https://jeffbridwell.com/chorus#> .

chorus:photos-domain a chorus:SubDomain ;
    rdfs:label "Photos. With a period inside." ;
    chorus:ownedBy chorus:silas ;
    chorus:partOf chorus:gathering .

chorus:music-domain a chorus:SubDomain ;
    chorus:ownedBy chorus:wren ;
    chorus:partOf chorus:gathering .
`;

describe('findBlockTerminator', () => {
  it('finds the block end, skipping periods inside quoted literals', () => {
    const start = TTL.indexOf('chorus:photos-domain');
    const end = findBlockTerminator(TTL, start);
    expect(end).toBeGreaterThan(start);
    const block = TTL.slice(start, end);
    expect(block).toContain('chorus:partOf chorus:gathering .');
    expect(block).not.toContain('music-domain');
  });

  it('returns -1 when no terminator exists', () => {
    expect(findBlockTerminator('chorus:x a chorus:SubDomain ;\n  chorus:ownedBy chorus:kade ;', 0)).toBe(-1);
  });
});

describe('patchTtlOwner', () => {
  it('rewrites ownedBy only inside the target block', () => {
    const patched = patchTtlOwner(TTL, 'photos-domain', 'kade');
    expect(patched).not.toBeNull();
    expect(patched!).toContain('chorus:ownedBy chorus:kade ;');
    // music block untouched
    expect(patched!).toContain('chorus:ownedBy chorus:wren ;');
    expect(patched!).not.toContain('chorus:ownedBy chorus:silas ;');
  });

  it('returns null for a missing subdomain block', () => {
    expect(patchTtlOwner(TTL, 'garden-domain', 'kade')).toBeNull();
  });
});

describe('setSubdomainOwner', () => {
  function fakeDeps(overrides: Partial<AthenaOwnerWriteDeps> = {}) {
    const calls: string[] = [];
    const deps: AthenaOwnerWriteDeps & { calls: string[]; written: string[] } = {
      calls,
      written: [],
      sparqlUpdate: async (u: string) => { calls.push('sparql'); calls.push(u); },
      readTtl: () => { calls.push('read'); return TTL; },
      writeTtl: (c: string) => { calls.push('write'); deps.written.push(c); },
      ...overrides,
    };
    return deps;
  }

  it('400 on invalid subdomainId and on invalid owner', async () => {
    const d = fakeDeps();
    expect((await setSubdomainOwner(d, { subdomainId: '../etc' })).status).toBe(400);
    expect((await setSubdomainOwner(d, { subdomainId: 'photos-domain', body: { owner: 'mallory' } })).status).toBe(400);
    expect(d.calls).toHaveLength(0); // rejected before any dep touched
  });

  it('404 when the subdomain block is not in the TTL; nothing written', async () => {
    const d = fakeDeps();
    const r = await setSubdomainOwner(d, { subdomainId: 'garden-domain', body: { owner: 'kade' } });
    expect(r.status).toBe(404);
    expect(d.calls).not.toContain('sparql');
    expect(d.calls).not.toContain('write');
  });

  it('200: SPARQL fires before TTL write; update targets the ontology graph', async () => {
    const d = fakeDeps();
    const r = await setSubdomainOwner(d, { subdomainId: 'photos-domain', body: { owner: 'KADE ' } }); // normalized
    expect(r.status).toBe(200);
    expect(r.body).toMatchObject({ ok: true, subdomain: 'photos-domain', owner: 'kade' });
    expect(d.calls.indexOf('sparql')).toBeLessThan(d.calls.indexOf('write'));
    const update = d.calls[d.calls.indexOf('sparql') + 1];
    expect(update).toContain('chorus:ownedBy');
    expect(update).toContain('photos-domain');
    expect(d.written[0]).toContain('chorus:ownedBy chorus:kade ;');
  });

  it('500 when SPARQL rejects — disk is never touched (ordering contract)', async () => {
    const d = fakeDeps({ sparqlUpdate: async () => { throw new Error('fuseki 503'); } });
    const r = await setSubdomainOwner(d, { subdomainId: 'photos-domain', body: { owner: 'kade' } });
    expect(r.status).toBe(500);
    expect((r.body as { error: string }).error).toContain('SPARQL update failed');
    expect(d.calls).not.toContain('write');
  });

  it('500 when the TTL read fails', async () => {
    const d = fakeDeps({ readTtl: () => { throw new Error('ENOENT'); } });
    const r = await setSubdomainOwner(d, { subdomainId: 'photos-domain', body: { owner: 'kade' } });
    expect(r.status).toBe(500);
    expect((r.body as { error: string }).error).toContain('Failed to read ontology TTL');
  });

  it('500 when the TTL write fails after SPARQL success (live graph ahead of seed, recoverable)', async () => {
    const d = fakeDeps({ writeTtl: () => { throw new Error('EACCES'); } });
    const r = await setSubdomainOwner(d, { subdomainId: 'photos-domain', body: { owner: 'kade' } });
    expect(r.status).toBe(500);
    expect((r.body as { error: string }).error).toContain('Failed to write ontology TTL');
  });
});
