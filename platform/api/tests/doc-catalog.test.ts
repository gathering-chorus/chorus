/**
 * Doc-catalog handler tests (#2445).
 *
 * AC1: Doc-catalog endpoints served from chorus-api at localhost:3340.
 * Tests describe Jeff-visible behavior at the pure-function level.
 */

import {
  buildDocCatalog,
  registerDoc,
  linkDocToDomain,
  getDomainArtifacts,
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
});
