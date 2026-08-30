// @test-type: unit — temp-dir fixtures + fake req/res; no live service, brings its own world.
// #4030 — the doc-catalog HTTP wrappers and the artifact classifier. Jeff's
// experience under test: the catalog page groups a doc by what it IS (service
// design, decision, ADR, ontology, manual, product, process…) from its title
// and filename, and a route that blows up answers a 500 with a plain reason
// instead of hanging the request.
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
  buildDocCatalog,
  listCatalog,
  addDoc,
  domainArtifacts,
  linkArtifact,
  type SourceDir,
} from '../src/handlers/doc-catalog';

type Sent = { status: number; body: unknown };
function fakeRes(): { res: any; sent: Sent } {
  const sent: Sent = { status: 200, body: undefined };
  const res = {
    status(n: number) { sent.status = n; return res; },
    json(b: unknown) { sent.body = b; return res; },
  };
  return { res, sent };
}
const throwingReq = (prop: 'body' | 'params') =>
  Object.defineProperty({}, prop, { get() { throw new Error('boom'); } });

describe('doc-catalog artifact classification (#4030)', () => {
  it('types each doc from its title + filename, so the catalog groups mean something', () => {
    const tmp = fs.mkdtempSync(path.join(os.tmpdir(), 'doc-catalog-types-'));
    const files: Array<[string, string, string]> = [
      ['service-design-board.html', 'Board Service Design', 'service-design'],
      ['DEC-041.md', '# DEC-041: Something decided', 'decision'],
      ['ADR-012.md', '# ADR-012: Cross-machine ops', 'adr'],
      ['domain-board.html', 'Domain — Board', 'domain-page'],
      ['predicates.md', '# Ontology predicate list', 'ontology'],
      ['c4-model.html', 'C4 system model', 'architecture'],
      ['homeostasis.md', '# Homeostasis research', 'research'],
      ['playbook.md', '# Incident playbook', 'manual'],
      ['vision.md', '# Product vision', 'product'],
      ['demo-flow.md', '# Demo flow', 'process'],
      ['misc-notes.md', '# Misc notes', 'architecture'],
    ];
    for (const [name, title] of files) {
      const body = name.endsWith('.html')
        ? `<html><head><title>${title}</title></head></html>`
        : title;
      fs.writeFileSync(path.join(tmp, name), body);
    }
    const dirs: SourceDir[] = [{ root: 'gathering', dir: '.', urlPrefix: '/fx/', source: 'fixture', defaultGroup: 'Fixture' }];
    const prev = process.env.GATHERING_REPO;
    process.env.GATHERING_REPO = tmp;
    try {
      const docs = buildDocCatalog(dirs).groups.flatMap((g) => g.docs);
      const byHref = new Map(docs.map((d) => [path.basename(d.href), d.artifactType]));
      for (const [name, , expected] of files) {
        expect([name, byHref.get(name)]).toEqual([name, expected]);
      }
    } finally {
      if (prev) process.env.GATHERING_REPO = prev; else delete process.env.GATHERING_REPO;
      fs.rmSync(tmp, { recursive: true, force: true });
    }
  });
});

describe('doc-catalog routes (#4030)', () => {
  it('POST /doc-catalog with an empty body → 400, not a crash', () => {
    const { res, sent } = fakeRes();
    addDoc({ body: undefined } as any, res);
    expect(sent.status).toBe(400);
  });

  it('GET /domain-artifacts/:domain without a domain → 400', () => {
    const { res, sent } = fakeRes();
    domainArtifacts({ params: { domain: '' } } as any, res);
    expect(sent.status).toBe(400);
  });

  it('POST /link-artifact with a bad relationship → 400', () => {
    const { res, sent } = fakeRes();
    linkArtifact({ body: { href: '/x.html', domain: 'board', relationship: 'owns' } } as any, res);
    expect(sent.status).toBe(400);
  });

  it('a route whose handler throws answers 500 with a plain reason (never a hung request)', () => {
    const quiet = jest.spyOn(console, 'error').mockImplementation(() => {});
    try {
      const a = fakeRes(); addDoc(throwingReq('body') as any, a.res);
      expect(a.sent).toEqual({ status: 500, body: { error: 'Failed to register doc' } });
      const d = fakeRes(); domainArtifacts(throwingReq('params') as any, d.res);
      expect(d.sent).toEqual({ status: 500, body: { error: 'Failed to get domain artifacts' } });
      const l = fakeRes(); linkArtifact(throwingReq('body') as any, l.res);
      expect(l.sent).toEqual({ status: 500, body: { error: 'Failed to link artifact' } });
    } finally {
      quiet.mockRestore();
    }
  });

  it('GET /doc-catalog answers the catalog shape', () => {
    const { res, sent } = fakeRes();
    listCatalog({} as any, res);
    expect(sent.status).toBe(200);
    const body = sent.body as { totalDocs: number; groups: unknown[] };
    expect(typeof body.totalDocs).toBe('number');
    expect(Array.isArray(body.groups)).toBe(true);
  });
});
