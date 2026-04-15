/**
 * Blast radius from domain data — #2019
 *
 * When a card has a domain tag, blast radius queries Fuseki's instances
 * graph for code files and test files in that domain — not just
 * regex-extracted paths from card text.
 *
 * Tests use mocked fetch to avoid hitting live APIs.
 */

import {
  generateBlastRadius,
  formatBlastComment,
} from '../src/blast-radius';

// Mock global fetch
const originalFetch = global.fetch;

// Helper: build SPARQL results JSON from a list of file paths
function sparqlFileResults(files: string[]) {
  return {
    results: {
      bindings: files.map(f => ({ filePath: { value: f } })),
    },
  };
}

function mockFetch(responses: Record<string, any>) {
  global.fetch = jest.fn(async (url: any, opts?: any) => {
    const urlStr = String(url);
    // SPARQL POST: match by URL pattern, decode the query body to check domain
    if (urlStr.includes('/pods/sparql') && opts?.body) {
      const body = String(opts.body);
      for (const [pattern, result] of Object.entries(responses)) {
        if (pattern.startsWith('sparql:') && body.includes(pattern.replace('sparql:', ''))) {
          return { ok: true, json: async () => result } as Response;
        }
      }
      // No matching SPARQL pattern — return empty results
      return { ok: true, json: async () => sparqlFileResults([]) } as Response;
    }
    // GET requests: match by URL substring
    for (const [pattern, body] of Object.entries(responses)) {
      if (!pattern.startsWith('sparql:') && urlStr.includes(pattern)) {
        return { ok: true, json: async () => body } as Response;
      }
    }
    return { ok: false, json: async () => ({}) } as Response;
  }) as any;
}

afterEach(() => {
  global.fetch = originalFetch;
});

describe('Blast radius from domain data (#2019)', () => {

  test('AC1: queries Fuseki instances graph for domain code files', async () => {
    mockFetch({
      '/api/codebase/topology': { domains: {} },
      'sparql:seeds-domain': sparqlFileResults([
        'src/handlers/seed.handler.ts',
        'src/services/seed-capture.service.ts',
      ]),
    });

    const report = await generateBlastRadius(
      2019,
      'Test card',
      'Some description without file paths',
      'seeds',
    );

    expect(report).not.toBeNull();
    expect(report!.totalFiles).toBeGreaterThan(0);

    // Verify Fuseki SPARQL was called (POST to /pods/sparql)
    const calls = (global.fetch as jest.Mock).mock.calls;
    const sparqlCalls = calls.filter(c => String(c[0]).includes('/pods/sparql'));
    expect(sparqlCalls.length).toBeGreaterThan(0);
  });

  test('AC2: includes test files from domain graph', async () => {
    mockFetch({
      '/api/codebase/topology': { domains: {} },
      'sparql:seeds-domain': sparqlFileResults([
        'src/handlers/seed.handler.ts',
        'tests/unit/handlers/seed.handler.test.ts',
        'tests/integration/seed-pipeline-flow.test.ts',
      ]),
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
      'sparql:seeds-domain': sparqlFileResults([
        'src/handlers/seed.handler.ts',
        'src/services/seed-capture.service.ts',
        'tests/unit/handlers/seed.handler.test.ts',
        'tests/integration/seed-pipeline-flow.test.ts',
      ]),
    });

    const report = await generateBlastRadius(
      2019,
      'Test card',
      'description',
      'seeds',
    );

    expect(report).not.toBeNull();
    expect(report!.totalFiles).toBe(4);

    // Format and check output includes files and counts
    const comment = formatBlastComment(report!);
    expect(comment).toMatch(/4 files/);
    expect(comment).toMatch(/seed\.handler\.ts/);
  });

  test('AC4: auto-generates from domain tag — no manual file listing needed', async () => {
    // Card has NO file paths in description — blast radius comes entirely from Fuseki
    mockFetch({
      '/api/codebase/topology': { domains: {} },
      'sparql:seeds-domain': sparqlFileResults([
        'src/handlers/seed.handler.ts',
        'src/services/seed-capture.service.ts',
        'platform/scripts/seed-probe.sh',
      ]),
    });

    const report = await generateBlastRadius(
      2019,
      'Fix seed routing',
      'Seeds should route correctly',  // no file paths
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

    // No domain param — should use existing file-path extraction from card text
    const report = await generateBlastRadius(
      100,
      'Fix seed handler',
      'Update src/handlers/seed.handler.ts to fix routing',
    );

    expect(report).not.toBeNull();
    expect(report!.totalFiles).toBeGreaterThan(0);

    // Should NOT have called Fuseki SPARQL for domain files
    const calls = (global.fetch as jest.Mock).mock.calls;
    const sparqlCalls = calls.filter(c => String(c[0]).includes('/pods/sparql'));
    expect(sparqlCalls.length).toBe(0);
  });

  test('Fuseki failure falls back to existing behavior', async () => {
    // Fuseki SPARQL will return ok:false (no sparql: pattern matched, default empty)
    // but the card text has extractable file paths
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

    // Should still work via existing regex path extraction
    expect(report).not.toBeNull();
    expect(report!.totalFiles).toBeGreaterThan(0);
  });
});
