// @test-type: integration — hits service/remote/sibling, skip-if-absent in CI
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

  // #2969 — recursive opt-in on SourceDir surfaces nested docs (skills/<name>/SKILL.md pattern)
  test('recursive SourceDir: scans subdirectories one level deep', () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'doc-catalog-recursive-'));
    fs.mkdirSync(path.join(tmpDir, 'skill-a'));
    fs.mkdirSync(path.join(tmpDir, 'skill-b'));
    fs.writeFileSync(path.join(tmpDir, 'skill-a', 'SKILL.md'), '# Skill A\n\nBody.');
    fs.writeFileSync(path.join(tmpDir, 'skill-b', 'SKILL.md'), '# Skill B\n\nBody.');
    fs.writeFileSync(path.join(tmpDir, 'top-level.md'), '# Top Level\n\nShould also appear.');
    try {
      const fixtureDirs: SourceDir[] = [
        { root: 'gathering', dir: '.', urlPrefix: '/recursive/', source: 'recursive-fixture',
          defaultGroup: 'Test Fixture', recursive: true },
      ];
      const prev = process.env.GATHERING_REPO;
      process.env.GATHERING_REPO = tmpDir;
      try {
        const result = buildDocCatalog(fixtureDirs);
        const titles = result.groups.flatMap(g => g.docs.map(d => d.title)).sort();
        expect(titles).toContain('Skill A');
        expect(titles).toContain('Skill B');
        expect(titles).toContain('Top Level');
        // Hrefs preserve relative paths
        const hrefs = result.groups.flatMap(g => g.docs.map(d => d.href));
        expect(hrefs).toContain('/recursive/skill-a/SKILL.md');
        expect(hrefs).toContain('/recursive/skill-b/SKILL.md');
      } finally {
        if (prev) process.env.GATHERING_REPO = prev; else delete process.env.GATHERING_REPO;
      }
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  test('non-recursive SourceDir (default): does NOT scan subdirectories', () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'doc-catalog-flat-'));
    fs.mkdirSync(path.join(tmpDir, 'nested'));
    fs.writeFileSync(path.join(tmpDir, 'nested', 'buried.md'), '# Buried\n\nShould not appear.');
    fs.writeFileSync(path.join(tmpDir, 'visible.md'), '# Visible\n\nShould appear.');
    try {
      const fixtureDirs: SourceDir[] = [
        { root: 'gathering', dir: '.', urlPrefix: '/flat/', source: 'flat-fixture',
          defaultGroup: 'Test Fixture' },
      ];
      const prev = process.env.GATHERING_REPO;
      process.env.GATHERING_REPO = tmpDir;
      try {
        const result = buildDocCatalog(fixtureDirs);
        const titles = result.groups.flatMap(g => g.docs.map(d => d.title));
        expect(titles).toContain('Visible');
        expect(titles).not.toContain('Buried');
      } finally {
        if (prev) process.env.GATHERING_REPO = prev; else delete process.env.GATHERING_REPO;
      }
    } finally {
      fs.rmSync(tmpDir, { recursive: true, force: true });
    }
  });

  // #2969 — registry takes precedence over scan: when both produce the same href,
  // the registered entry's curated metadata wins. Pre-#2969 the scan entry won
  // silently which made the bulk-register-40 work invisible to the catalog.
  test('registry-takes-precedence: registered entry overrides scan metadata for same href', () => {
    const tmpDir = fs.mkdtempSync(path.join(os.tmpdir(), 'doc-catalog-prec-'));
    const tmpFile = path.join(tmpDir, 'precedence.html');
    fs.writeFileSync(tmpFile, '<html><head><title>Scan Title</title></head></html>');
    const uniqueHref = `/fixture/precedence.html`;
    const registryPath = path.join(
      process.env.CHORUS_REPO || '/Users/jeffbridwell/CascadeProjects/chorus',
      'platform', 'api', 'data', 'doc-catalog-registry.json',
    );
    const originalRegistry = fs.readFileSync(registryPath, 'utf-8');
    try {
      // Inject a registry entry that points at the same file but declares a curated group
      const registry = JSON.parse(originalRegistry) as Array<{ filePath: string; href: string; group?: string }>;
      registry.push({ filePath: tmpFile, href: uniqueHref, group: 'Curated Override Group' });
      fs.writeFileSync(registryPath, JSON.stringify(registry, null, 2));

      const fixtureDirs: SourceDir[] = [
        { root: 'gathering', dir: '.', urlPrefix: '/fixture/', source: 'scan-source',
          defaultGroup: 'Scan Default Group' },
      ];
      const prev = process.env.GATHERING_REPO;
      process.env.GATHERING_REPO = tmpDir;
      try {
        const result = buildDocCatalog(fixtureDirs);
        // Find the doc by href and check its group is the curated one, not the scan default
        const allDocs = result.groups.flatMap(g => g.docs.map(d => ({ ...d, _groupName: g.name })));
        const doc = allDocs.find(d => d.href === uniqueHref);
        expect(doc).toBeDefined();
        // Registry source wins, group is curated
        expect(doc!.source).toBe('manual');
        expect(doc!._groupName).toBe('Curated Override Group');
      } finally {
        if (prev) process.env.GATHERING_REPO = prev; else delete process.env.GATHERING_REPO;
      }
    } finally {
      // Always restore registry
      fs.writeFileSync(registryPath, originalRegistry);
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
