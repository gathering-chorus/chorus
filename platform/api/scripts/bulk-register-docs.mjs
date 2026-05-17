#!/usr/bin/env node
/**
 * #2969 — Bulk-register the 40 published-but-unregistered docs in
 * personal-site/public/gathering-docs/ using the new chorus_doc_catalog_add
 * MCP tool (compiled from this werk). This is the AC6 end-to-end proof:
 * the tool's code path (compiled from src/mcp/server.ts) drives 40 real
 * registrations against the live chorus-api at localhost:3340.
 *
 * Run from werk root: node platform/api/scripts/bulk-register-docs.mjs
 */
import { buildMcpServer } from '../dist/mcp/server.js';
import fs from 'fs';
import path from 'path';

const PUB_DIR = '/Users/jeffbridwell/CascadeProjects/jeff-bridwell-personal-site/public/gathering-docs';
const API_BASE = 'http://localhost:3340';

// Use the running chorus-api's HTTP endpoint via the new MCP tool.
// fetchImpl shape matches what the tool's executeDocCatalogAdd expects.
const fetchImpl = async (url, init) => {
  const r = await fetch(url, init);
  return {
    ok: r.ok,
    status: r.status,
    json: () => r.json(),
    text: () => r.text(),
  };
};

const server = buildMcpServer(() => 'silas', { fetchImpl, apiBase: API_BASE });
const handler = server._requestHandlers.get('tools/call');

// Discover already-registered hrefs so we skip them and report cleanly.
const registryPath = path.resolve(process.cwd(), 'platform/api/data/doc-catalog-registry.json');
const registered = new Set(JSON.parse(fs.readFileSync(registryPath, 'utf8')).map((e) => e.href));

const candidates = fs
  .readdirSync(PUB_DIR)
  .filter((f) => f.endsWith('.html') || f.endsWith('.md'))
  .map((f) => ({ filePath: path.join(PUB_DIR, f), href: `/gathering-docs/${f}` }))
  .filter((c) => !registered.has(c.href));

console.log(`bulk-register-docs: ${candidates.length} unregistered candidates`);

const results = { success: [], failed: [] };
for (const c of candidates) {
  try {
    const result = await handler(
      { method: 'tools/call', params: { name: 'chorus_doc_catalog_add', arguments: c } },
      {},
    );
    results.success.push(c.href);
    process.stdout.write('.');
  } catch (err) {
    results.failed.push({ href: c.href, error: String(err.message || err) });
    process.stdout.write('x');
  }
}
process.stdout.write('\n');

console.log(`\nregistered: ${results.success.length}`);
console.log(`failed:     ${results.failed.length}`);
if (results.failed.length > 0) {
  console.log('failures:');
  for (const f of results.failed) console.log(`  ${f.href}: ${f.error}`);
  process.exit(1);
}
