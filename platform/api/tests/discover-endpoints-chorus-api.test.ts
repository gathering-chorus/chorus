/**
 * #2485 Move 8 — chorus-api route scanner for discover-endpoints.
 */
import { parseChorusApiRoutes } from '../src/discover-endpoints-chorus-api';

describe('parseChorusApiRoutes', () => {
  test('tags concrete /api/loom/<slug> route to loom-<slug> when subdomain is valid', () => {
    const src = `
      app.get('/api/loom/decisions', handler);
      app.get('/api/loom/principles', handler);
    `;
    const valid = new Set(['loom-decisions', 'loom-principles', 'chorus-domain']);
    const entries = parseChorusApiRoutes(src, valid);
    const decisions = entries.find((e) => e.path === '/api/loom/decisions');
    expect(decisions).toBeDefined();
    expect(decisions && decisions.domainId).toBe('loom-decisions');
    expect(decisions && decisions.method).toBe('GET');
    const principles = entries.find((e) => e.path === '/api/loom/principles');
    expect(principles && principles.domainId).toBe('loom-principles');
  });

  test('falls back to chorus-domain for routes that do not match loom-*', () => {
    const src = `app.get('/api/athena/health', handler);`;
    const entries = parseChorusApiRoutes(src, new Set(['chorus-domain']));
    expect(entries).toHaveLength(1);
    expect(entries[0].domainId).toBe('chorus-domain');
    expect(entries[0].path).toBe('/api/athena/health');
  });

  test('skips parameterized routes containing : (need per-subdomain instantiation)', () => {
    const src = `
      app.get('/api/athena/subdomains/:id/code', handler);
      app.get('/api/chorus/domain/:name/tests', handler);
    `;
    const entries = parseChorusApiRoutes(src, new Set(['chorus-domain']));
    expect(entries).toEqual([]);
  });

  test('emits one entry per HTTP verb on the same path', () => {
    const src = `
      app.get('/api/athena/health', handler);
      app.post('/api/athena/health', handler);
    `;
    const entries = parseChorusApiRoutes(src, new Set(['chorus-domain']));
    expect(entries).toHaveLength(2);
    const methods = entries.map((e) => e.method).sort();
    expect(methods).toEqual(['GET', 'POST']);
  });

  test('does not tag loom-<slug> when that subdomain is not in valid set', () => {
    const src = `app.get('/api/loom/cookbook-substrate-class-domain', handler);`;
    const valid = new Set(['loom-decisions', 'chorus-domain']);
    const entries = parseChorusApiRoutes(src, valid);
    expect(entries).toHaveLength(1);
    expect(entries[0].domainId).toBe('chorus-domain');
  });
});
