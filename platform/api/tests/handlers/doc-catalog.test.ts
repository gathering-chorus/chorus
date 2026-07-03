// @test-type: unit — tmpdir fixture repos via GATHERING_REPO/CHORUS_REPO (#2517); no live repos, no live services.
//
// #3606 — doc-catalog.ts sat at 6% covered (262 uncovered statements, the
// single biggest handler gap behind the platform/api 72.95% < 80 floor red).
// Only an integration-tier test existed, which the hermetic nightly doesn't
// run. These are real behavior tests of the pure handler functions the
// integration tier was exercising through HTTP.
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import type { Request, Response } from 'express';
import {
  buildDocCatalog,
  collectAllDocs,
  registerDoc,
  getDomainArtifacts,
  linkDocToDomain,
  listCatalog,
  addDoc,
  domainArtifacts,
  linkArtifact,
  type SourceDir,
} from '../../src/handlers/doc-catalog';

let GTMP: string;
let CTMP: string;
let prevG: string | undefined;
let prevC: string | undefined;

beforeAll(() => {
  prevG = process.env.GATHERING_REPO;
  prevC = process.env.CHORUS_REPO;
  GTMP = fs.mkdtempSync(path.join(os.tmpdir(), 'doccat-g-'));
  CTMP = fs.mkdtempSync(path.join(os.tmpdir(), 'doccat-c-'));
  process.env.GATHERING_REPO = GTMP;
  process.env.CHORUS_REPO = CTMP;
});

afterAll(() => {
  if (prevG === undefined) delete process.env.GATHERING_REPO; else process.env.GATHERING_REPO = prevG;
  if (prevC === undefined) delete process.env.CHORUS_REPO; else process.env.CHORUS_REPO = prevC;
  fs.rmSync(GTMP, { recursive: true, force: true });
  fs.rmSync(CTMP, { recursive: true, force: true });
});

function write(root: string, rel: string, content: string): string {
  const abs = path.join(root, rel);
  fs.mkdirSync(path.dirname(abs), { recursive: true });
  fs.writeFileSync(abs, content);
  return abs;
}

function fixtureSourceDirs(): SourceDir[] {
  return [
    { root: 'chorus', dir: 'fixture-docs', urlPrefix: '/fixture-docs/', source: 'fixture', defaultGroup: 'Fixture Group' },
    { root: 'chorus', dir: 'fixture-skills', urlPrefix: '/fixture-skills/', source: 'skills', defaultGroup: 'Skills', recursive: true },
  ];
}

describe('buildDocCatalog (scan + classify + group)', () => {
  beforeAll(() => {
    write(CTMP, 'fixture-docs/design-notes.md', '# Real Markdown Title\n\nbody');
    write(CTMP, 'fixture-docs/system-page.html', '<html><head><title>System Page — Gathering</title></head></html>');
    write(CTMP, 'fixture-docs/no_title_here.md', 'no heading in this one');
    write(CTMP, 'fixture-docs/ignored.txt', 'not a doc');
    write(CTMP, 'fixture-skills/pull/SKILL.md', '# Pull Skill\n');
  });

  it('scans md + html, extracts titles, skips non-docs', () => {
    const cat = buildDocCatalog(fixtureSourceDirs());
    const all = cat.groups.flatMap((g) => g.docs);
    const titles = all.map((d) => d.title);
    expect(titles).toContain('Real Markdown Title');
    expect(titles).toContain('System Page'); // html <title>, " — Gathering" suffix stripped
    expect(titles).toContain('No Title Here'); // filename fallback, humanized
    expect(all.some((d) => d.href.endsWith('.txt'))).toBe(false);
    expect(cat.totalDocs).toBe(all.length);
  });

  it('recursive source dirs find nested docs (skills tree, #2969)', () => {
    const all = collectAllDocs(fixtureSourceDirs());
    const skill = all.find((d) => d.title === 'Pull Skill');
    expect(skill).toBeDefined();
    expect(skill!.href).toBe('/fixture-skills/pull/SKILL.md');
    expect(skill!.absPath).toBe(path.join(CTMP, 'fixture-skills/pull/SKILL.md'));
  });

  it('every doc lands in exactly one classified group; unmatched titles fall to Other', () => {
    const cat = buildDocCatalog(fixtureSourceDirs());
    const all = cat.groups.flatMap((g) => g.docs);
    // partition: no doc appears in two groups, none dropped
    expect(all.length).toBe(cat.totalDocs);
    expect(new Set(all.map((d) => d.href)).size).toBe(all.length);
    // "no_title_here" matches no GROUP_PATTERNS keyword → Other
    const other = cat.groups.find((g) => g.name === 'Other');
    expect(other).toBeDefined();
    expect(other!.docs.some((d) => d.title === 'No Title Here')).toBe(true);
  });
});

describe('registerDoc (validation + persistence)', () => {
  it('400 when filePath or href missing', () => {
    expect(registerDoc({}).status).toBe(400);
    expect(registerDoc({ filePath: '/x.md' }).status).toBe(400);
    expect(registerDoc({ href: '/x' }).status).toBe(400);
  });

  it('404 when the file does not exist', () => {
    const r = registerDoc({ filePath: path.join(CTMP, 'missing.md'), href: '/missing' });
    expect(r.status).toBe(404);
  });

  it('400 for non-doc extensions', () => {
    const abs = write(CTMP, 'fixture-docs/raw.txt', 'x');
    expect(registerDoc({ filePath: abs, href: '/raw' }).status).toBe(400);
  });

  it('201 registers and persists; 409 on duplicate href', () => {
    const abs = write(CTMP, 'manual/registered-doc.md', '# Registered Doc\n');
    const r = registerDoc({ filePath: abs, href: '/manual/registered-doc', group: 'Curated' });
    expect(r.status).toBe(201);
    const body = r.body as { registered: { href: string }; doc: { title: string; source: string; group: string } };
    expect(body.doc.title).toBe('Registered Doc');
    expect(body.doc.source).toBe('manual');
    expect(body.doc.group).toBe('Curated');
    // Persisted to the CHORUS_REPO-relative registry, not the real one.
    const reg = JSON.parse(fs.readFileSync(path.join(CTMP, 'platform/api/data/doc-catalog-registry.json'), 'utf-8'));
    expect(reg.some((e: { href: string }) => e.href === '/manual/registered-doc')).toBe(true);

    expect(registerDoc({ filePath: abs, href: '/manual/registered-doc' }).status).toBe(409);
  });

  it('registered docs appear in the catalog (collectFromRegistry)', () => {
    const all = collectAllDocs(fixtureSourceDirs());
    expect(all.some((d) => d.href === '/manual/registered-doc')).toBe(true);
  });
});

describe('linkDocToDomain + getDomainArtifacts', () => {
  it('400 on missing fields and bad relationship', () => {
    expect(linkDocToDomain({}).status).toBe(400);
    expect(linkDocToDomain({ href: '/a', domain: 'photos', relationship: 'owns' }).status).toBe(400);
  });

  it('201 creates a link; 409 on exact duplicate', () => {
    const r = linkDocToDomain({ href: '/manual/registered-doc', domain: 'photos', relationship: 'governs' });
    expect(r.status).toBe(201);
    expect(linkDocToDomain({ href: '/manual/registered-doc', domain: 'photos', relationship: 'governs' }).status).toBe(409);
  });

  it('400 when domain param missing', () => {
    expect(getDomainArtifacts(undefined).status).toBe(400);
  });

  it('returns manual governs links resolved to docs, with health', () => {
    const r = getDomainArtifacts('photos');
    expect(r.status).toBe(200);
    const body = r.body as { domain: string; governs: Array<{ title: string }>; health: { total: number; undocumented: boolean } };
    expect(body.domain).toBe('photos');
    expect(body.governs.some((d) => d.title === 'Registered Doc')).toBe(true);
    expect(body.health.total).toBeGreaterThanOrEqual(1);
    expect(body.health.undocumented).toBe(false);
  });

  it('infers governs links from service-design-<domain>.html filenames', () => {
    // Default SOURCE_DIRS resolve under the tmp CHORUS_REPO — put a fixture
    // where a real scan would find it.
    write(CTMP, 'designing/docs/service-design-gardening.html', '<title>Gardening Service Design</title>');
    const r = getDomainArtifacts('gardening');
    const body = r.body as { governs: Array<{ href: string }> };
    expect(r.status).toBe(200);
    expect(body.governs.some((d) => d.href === '/designing/docs/service-design-gardening.html')).toBe(true);
  });
});

describe('express adapters', () => {
  function fakeRes(): { res: Response; out: { status: number; json: unknown } } {
    const out = { status: 200, json: undefined as unknown };
    const res = {
      status(code: number) { out.status = code; return this; },
      json(v: unknown) { out.json = v; },
    } as unknown as Response;
    return { res, out };
  }

  it('listCatalog writes the catalog as json', () => {
    const { res, out } = fakeRes();
    listCatalog({} as Request, res);
    expect((out.json as { totalDocs: number }).totalDocs).toBeGreaterThanOrEqual(1);
  });

  it('addDoc surfaces validation status from registerDoc', () => {
    const { res, out } = fakeRes();
    addDoc({ body: {} } as Request, res);
    expect(out.status).toBe(400);
  });

  it('domainArtifacts passes the :domain param through', () => {
    const { res, out } = fakeRes();
    domainArtifacts({ params: { domain: 'photos' } } as unknown as Request, res);
    expect(out.status).toBe(200);
    expect((out.json as { domain: string }).domain).toBe('photos');
  });

  it('linkArtifact surfaces duplicate-link conflicts', () => {
    const { res, out } = fakeRes();
    linkArtifact({ body: { href: '/manual/registered-doc', domain: 'photos', relationship: 'governs' } } as Request, res);
    expect(out.status).toBe(409);
  });
});
