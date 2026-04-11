/**
 * Framework lint — #1909
 * Static analysis of server.ts for framework pattern compliance.
 */
const fs = require('fs');
const path = require('path');

const SERVER_SRC = fs.readFileSync(path.join(__dirname, '..', 'src', 'server.ts'), 'utf-8');

describe('#1909: Framework lint — Athena endpoint patterns', () => {

  test('no raw fetch to Fuseki update — use athenaSparqlUpdate()', () => {
    // Find fetch calls that go to the update endpoint but aren't inside athenaSparqlUpdate()
    // Pattern: fetch(ATHENA_SPARQL.replace('/sparql', '/update') — these bypass res.ok check
    const rawUpdateCalls = SERVER_SRC.match(/fetch\(ATHENA_SPARQL\.replace\('\/sparql',\s*'\/update'\)/g) || [];
    expect(rawUpdateCalls.length).toBe(0);
  });

  test('every Athena POST endpoint validates required fields', () => {
    // Find all POST /api/athena handlers
    const postHandlers = SERVER_SRC.match(/app\.post\('\/api\/athena\/[^']+'/g) || [];
    expect(postHandlers.length).toBeGreaterThan(0);

    // Each POST handler block should contain a 400 status for missing fields
    for (const handler of postHandlers) {
      const handlerStart = SERVER_SRC.indexOf(handler);
      const handlerBlock = SERVER_SRC.slice(handlerStart, handlerStart + 3000);
      const has400 = handlerBlock.includes('400') || handlerBlock.includes('Missing');
      expect(has400).toBe(true);
    }
  });

  test('Athena SPARQL catch blocks use athenaEnvelope for error responses', () => {
    // Find catch blocks in /api/athena/ route handlers (GET and POST with SPARQL)
    // Exclude utility endpoints (open, reload) that don't use SPARQL
    const athenaRoutes = SERVER_SRC.match(/app\.(get|post)\('\/api\/athena\/[^']+',[\s\S]*?catch\s*\(err:\s*any\)\s*\{[^}]+\}/g) || [];
    expect(athenaRoutes.length).toBeGreaterThan(0);
    const nakedCatches = athenaRoutes.filter(b => {
      const catchBlock = b.match(/catch\s*\(err:\s*any\)\s*\{[^}]+\}/)?.[0] || '';
      return !catchBlock.includes('athenaEnvelope');
    });
    expect(nakedCatches.length).toBe(0);
  });
});
