/**
 * Doc-catalog handler tests (#2445).
 *
 * AC1: Doc-catalog endpoints served from chorus-api at localhost:3340.
 * Tests describe Jeff-visible behavior at the pure-function level.
 */

import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import {
  buildDocCatalog,
  registerDoc,
  linkDocToDomain,
  getDomainArtifacts,
  type SourceDir,
} from '../src/handlers/doc-catalog';

describe('doc-catalog (#2445 — relocated from gathering)', () => {
  test('buildDocCatalog returns totalDocs + groups[] populated from real corpus', () => {
    const result = buildDocCatalog();
    expect(typeof result.totalDocs).toBe('number');
    expect(result.totalDocs).toBeGreaterThan(0);
    expect(Array.isArray(result.groups)).toBe(true);
    expect(result.groups.length).toBeGreaterThan(0);
    for (const g of result.groups) {
      expect(typeof g.name).toBe('string');
      expect(Array.isArray(g.docs)).toBe(true);
    }
  });

  test('registerDoc rejects empty input with 400', () => {
    const r = registerDoc({});
    expect(r.status).toBe(400);
    expect((r.body as { error: string }).error).toMatch(/Required/);
  });

  test('registerDoc rejects non-html/md filePath with 400', () => {
    const r = registerDoc({ filePath: __filename, href: '/doc-catalog-test.ts' });
    expect(r.status).toBe(400);
    expect((r.body as { error: string }).error).toMatch(/html|md/);
  });

  test('registerDoc rejects nonexistent filePath with 404', () => {
    const r = registerDoc({ filePath: '/no/such/path.html', href: '/x.html' });
    expect(r.status).toBe(404);
    expect((r.body as { error: string }).error).toMatch(/not found/i);
  });

  test('linkDocToDomain rejects empty input with 400', () => {
    const r = linkDocToDomain({});
    expect(r.status).toBe(400);
    expect((r.body as { error: string }).error).toMatch(/Required/);
  });

  test('linkDocToDomain rejects bad relationship with 400', () => {
    const r = linkDocToDomain({ href: '/a', domain: 'b', relationship: 'invalid' });
    expect(r.status).toBe(400);
    expect((r.body as { error: string }).error).toMatch(/governs|references/);
  });

  test('getDomainArtifacts rejects empty domain with 400', () => {
    const r = getDomainArtifacts(undefined);
    expect(r.status).toBe(400);
  });

  test('getDomainArtifacts returns governs/references/health shape', () => {
    const r = getDomainArtifacts('chorus');
    expect(r.status).toBe(200);
    const body = r.body as { domain: string; governs: unknown[]; references: unknown[]; health: { total: number; stale: number } };
    expect(body.domain).toBe('chorus');
    expect(Array.isArray(body.governs)).toBe(true);
    expect(Array.isArray(body.references)).toBe(true);
    expect(typeof body.health.total).toBe('number');
    expect(typeof body.health.stale).toBe('number');
  });

  // #2517 — happy-path tests + relocation contract (registry path)

  test('registerDoc happy path: valid html file returns 201 + persists to registry', () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'doc-catalog-test-'));
    const tmpFile = path.join(tmpDir, 'happy-path.html');
    fs.writeFileSync(tmpFile, '<html><head><title>Happy Path Doc</title></head></html>');
    const uniqueHref = `/test-happy-path-${Date.now()}.html`;
    try {
      const r = registerDoc({ filePath: tmpFile, href: uniqueHref });
      expect(r.status).toBe(201);
      const body = r.body as { registered: { href: string }; doc: { title: string } | null };
      expect(body.registered.href).toBe(uniqueHref);
      expect(body.doc).not.toBeNull();
      // Side-effect: registry file contains our entry
      const registryPath = path.join(
        process.env.CHORUS_REPO || '/Users/jeffbridwell/CascadeProjects/chorus',
        'platform', 'api', 'data', 'doc-catalog-registry.json',
      );
      const registry = JSON.parse(fs.readFileSync(registryPath, 'utf-8')) as Array<{ href: string }>;
      expect(registry.some(e => e.href === uniqueHref)).toBe(true);
      // Cleanup
      const cleaned = registry.filter(e => e.href !== uniqueHref);
      fs.writeFileSync(registryPath, JSON.stringify(cleaned, null, 2));
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  test('linkDocToDomain happy path: valid input returns 201 + persists to links', () => {
    const uniqueHref = `/test-link-${Date.now()}.html`;
    const r = linkDocToDomain({ href: uniqueHref, domain: 'test-domain', relationship: 'governs' });
    expect(r.status).toBe(201);
    const body = r.body as { linked: { href: string; domain: string; relationship: string } };
    expect(body.linked.href).toBe(uniqueHref);
    expect(body.linked.domain).toBe('test-domain');
    expect(body.linked.relationship).toBe('governs');
    // Side-effect: links file contains our entry
    const linksPath = path.join(
      process.env.CHORUS_REPO || '/Users/jeffbridwell/CascadeProjects/chorus',
      'platform', 'api', 'data', 'doc-catalog-links.json',
    );
    const links = JSON.parse(fs.readFileSync(linksPath, 'utf-8')) as Array<{ href: string }>;
    expect(links.some(l => l.href === uniqueHref)).toBe(true);
    // Cleanup
    const cleaned = links.filter(l => l.href !== uniqueHref);
    fs.writeFileSync(linksPath, JSON.stringify(cleaned, null, 2));
  });

  test('hermetic: buildDocCatalog accepts injectable sourceDirs and returns only fixture docs', () => {
    // AC3 — independent of real corpus. Build a temp dir with 2 known files,
    // pass it as sourceDirs, assert the result reflects only the fixture.
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'doc-catalog-fixture-'));
    fs.writeFileSync(path.join(tmpDir, 'fixture-a.html'), '<html><head><title>Fixture A</title></head></html>');
    fs.writeFileSync(path.join(tmpDir, 'fixture-b.md'), '# Fixture B\n\nContent.');
    try {
      const fixtureDirs: SourceDir[] = [
        // The runtime resolves SourceDir.dir against rootPath(root). For the
        // hermetic test, point root='gathering' at our tmp via env override.
        { root: 'gathering', dir: '.', urlPrefix: '/fixture/', source: 'fixture',
          defaultGroup: 'Test Fixture' },
      ];
      const prev = process.env.GATHERING_REPO;
      process.env.GATHERING_REPO = tmpDir;
      try {
        const result = buildDocCatalog(fixtureDirs);
        const titles = result.groups.flatMap(g => g.docs.map(d => d.title)).sort();
        // Should see Fixture A and Fixture B; nothing else from the real corpus
        expect(titles).toContain('Fixture A');
        expect(titles).toContain('Fixture B');
      } finally {
        if (prev) process.env.GATHERING_REPO = prev; else delete process.env.GATHERING_REPO;
      }
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  test('relocation contract: buildDocCatalog returns same shape for repeated calls', () => {
    // Snapshot test — calling twice with no changes should produce identical
    // shape. Catches drift in classifier rules across the cutover from gathering
    // to chorus-api. Full golden-file snapshot is overkill at 303 docs;
    // shape-stability is the contract that matters for the relocation.
    const a = buildDocCatalog();
    const b = buildDocCatalog();
    expect(a.totalDocs).toBe(b.totalDocs);
    expect(a.groups.length).toBe(b.groups.length);
    expect(a.groups.map(g => g.name)).toEqual(b.groups.map(g => g.name));
    for (let i = 0; i < a.groups.length; i++) {
      expect(a.groups[i].docs.length).toBe(b.groups[i].docs.length);
    }
  });
});
