/**
 * Blast radius from domain data — #2019, #2059
 *
 * When a card has a domain tag, blast radius queries the Chorus API
 * code-files endpoint for domain code files — DEC-093 compliant.
 *
 * Tests use mocked fetch to avoid hitting live APIs.
 */

import {
  generateBlastRadius,
  formatBlastComment,
} from '../src/blast-radius';

// Mock global fetch
const originalFetch = global.fetch;

function mockFetch(responses: Record<string, any>) {
  global.fetch = jest.fn(async (url: any) => {
    const urlStr = String(url);
    for (const [pattern, body] of Object.entries(responses)) {
      if (urlStr.includes(pattern)) {
        return { ok: true, json: async () => body } as Response;
      }
    }
    return { ok: false, json: async () => ({}) } as Response;
  }) as any;
}

afterEach(() => {
  global.fetch = originalFetch;
});

describe('Blast radius from domain data (#2019, #2059)', () => {

  test('AC1: queries Chorus API code-files endpoint for domain files', async () => {
    mockFetch({
      '/api/codebase/topology': { domains: {} },
      '/api/chorus/domain/seeds/code': {
        data: {
          files: [
            { path: 'src/handlers/seed.handler.ts' },
            { path: 'src/services/seed-capture.service.ts' },
          ],
        },
      },
    });

    const report = await generateBlastRadius(
      2019,
      'Test card',
      'Some description without file paths',
      'seeds',
    );

    expect(report).not.toBeNull();
    expect(report!.totalFiles).toBeGreaterThan(0);

    // Verify Chorus API was called (not direct Fuseki)
    const calls = (global.fetch as jest.Mock).mock.calls.map(c => String(c[0]));
    expect(calls.some(u => u.includes('/api/chorus/domain/seeds/code'))).toBe(true);
    expect(calls.some(u => u.includes('/pods/sparql'))).toBe(false);
  });

  test('AC2: includes test files from domain', async () => {
    mockFetch({
      '/api/codebase/topology': { domains: {} },
      '/api/chorus/domain/seeds/code': {
        data: {
          files: [
            { path: 'src/handlers/seed.handler.ts' },
            { path: 'tests/unit/handlers/seed.handler.test.ts' },
            { path: 'tests/integration/seed-pipeline-flow.test.ts' },
          ],
        },
      },
    });

    const report = await generateBlastRadius(
      2019,
      'Test card',
      'No file paths here',
      'seeds',
    );

    expect(report).not.toBeNull();
    const allFiles = report!.dimensions.flatMap(d => d.files);
    const testFiles = allFiles.filter(f => /\.test\.|\.spec\./.test(f));
    expect(testFiles.length).toBeGreaterThanOrEqual(2);
  });

  test('AC3: output shows file count, test count, and lists files', async () => {
    mockFetch({
      '/api/codebase/topology': { domains: {} },
      '/api/chorus/domain/seeds/code': {
        data: {
          files: [
            { path: 'src/handlers/seed.handler.ts' },
            { path: 'src/services/seed-capture.service.ts' },
            { path: 'tests/unit/handlers/seed.handler.test.ts' },
            { path: 'tests/integration/seed-pipeline-flow.test.ts' },
          ],
        },
      },
    });

    const report = await generateBlastRadius(
      2019,
      'Test card',
      'description',
      'seeds',
    );

    expect(report).not.toBeNull();
    expect(report!.totalFiles).toBe(4);

    const comment = formatBlastComment(report!);
    expect(comment).toMatch(/4 files/);
    expect(comment).toMatch(/seed\.handler\.ts/);
  });

  test('AC4: auto-generates from domain tag — no manual file listing needed', async () => {
    mockFetch({
      '/api/codebase/topology': { domains: {} },
      '/api/chorus/domain/seeds/code': {
        data: {
          files: [
            { path: 'src/handlers/seed.handler.ts' },
            { path: 'src/services/seed-capture.service.ts' },
            { path: 'platform/scripts/seed-probe.sh' },
          ],
        },
      },
    });

    const report = await generateBlastRadius(
      2019,
      'Fix seed routing',
      'Seeds should route correctly',
      'seeds',
    );

    expect(report).not.toBeNull();
    expect(report!.totalFiles).toBe(3);
    const allFiles = report!.dimensions.flatMap(d => d.files);
    expect(allFiles).toContain('src/handlers/seed.handler.ts');
  });

  test('without domain param, falls back to existing behavior', async () => {
    mockFetch({
      '/api/codebase/topology': { domains: { seeds: {} } },
      '/api/codebase/node/src%2Fhandlers%2Fseed.handler.ts': {
        path: 'src/handlers/seed.handler.ts',
        type: 'handler',
        domain: 'seeds',
        spoke: 'api',
        connections: 2,
        connected: [],
      },
    });

    const report = await generateBlastRadius(
      100,
      'Fix seed handler',
      'Update src/handlers/seed.handler.ts to fix routing',
    );

    expect(report).not.toBeNull();
    expect(report!.totalFiles).toBeGreaterThan(0);

    // Should NOT have called the /code endpoint (no domain provided)
    const calls = (global.fetch as jest.Mock).mock.calls.map(c => String(c[0]));
    expect(calls.some(u => u.includes('/api/chorus/domain/seeds/code'))).toBe(false);
  });

  test('code-files API failure falls back to existing behavior', async () => {
    // code-files endpoint returns ok:false, card text has extractable paths
    mockFetch({
      '/api/codebase/topology': { domains: { seeds: {} } },
      '/api/codebase/node/src%2Fhandlers%2Fseed.handler.ts': {
        path: 'src/handlers/seed.handler.ts',
        type: 'handler',
        domain: 'seeds',
        spoke: 'api',
        connections: 0,
        connected: [],
      },
    });

    const report = await generateBlastRadius(
      100,
      'Fix seed handler',
      'Update src/handlers/seed.handler.ts',
      'seeds',
    );

    expect(report).not.toBeNull();
    expect(report!.totalFiles).toBeGreaterThan(0);
  });
});
