#!/usr/bin/env node
/**
 * doc-tag-coverage.ts — apply doc-tagger to all catalog entries and
 * report coverage (#2520).
 *
 * Reads /api/doc-catalog from chorus-api, derives a (product, subproduct,
 * subdomain) tag for every entry via inferTags(), prints coverage broken
 * down by product, and lists docs that came back as confidence=none.
 *
 * Run: ts-node platform/scripts/doc-tag-coverage.ts
 */

import { inferTags, type DocTags } from '../api/src/handlers/doc-tagger';

const CATALOG_URL = 'http://localhost:3340/api/doc-catalog';

interface CatalogDoc {
  href: string;
  source: string;
  title: string;
}

async function main() {
  const res = await fetch(CATALOG_URL);
  if (!res.ok) {
    console.error(`Catalog fetch failed: ${res.status}`);
    process.exit(1);
  }
  const data = await res.json() as { totalDocs: number; groups: { docs: CatalogDoc[] }[] };
  const docs: CatalogDoc[] = data.groups.flatMap(g => g.docs);

  const results: Array<{ doc: CatalogDoc; tags: DocTags }> = docs.map(doc => {
    // Reconstruct a sourcePath from href + source label.
    // Catalog hrefs are URLs; tagger expects filesystem-relative paths.
    // Approximation: prefix source label.
    const basename = doc.href.split('/').pop() || '';
    const sourcePath = `${doc.source}/${basename}`;
    const tags = inferTags({ sourcePath, basename });
    return { doc, tags };
  });

  // Coverage breakdown
  const byProduct: Record<string, number> = {};
  const bySubproduct: Record<string, number> = {};
  const byConfidence: Record<string, number> = {};
  let tagged = 0;
  let withSubdomain = 0;
  for (const r of results) {
    if (r.tags.product) {
      byProduct[r.tags.product] = (byProduct[r.tags.product] || 0) + 1;
      tagged++;
    }
    if (r.tags.subproduct) {
      bySubproduct[r.tags.subproduct] = (bySubproduct[r.tags.subproduct] || 0) + 1;
    }
    if (r.tags.subdomain) withSubdomain++;
    byConfidence[r.tags.confidence] = (byConfidence[r.tags.confidence] || 0) + 1;
  }

  console.log(`Total catalog entries: ${docs.length}`);
  console.log(`Tagged with product:   ${tagged} (${Math.round(100 * tagged / docs.length)}%)`);
  console.log(`Tagged with subdomain: ${withSubdomain} (${Math.round(100 * withSubdomain / docs.length)}%)`);
  console.log();
  console.log('By product:');
  for (const [k, n] of Object.entries(byProduct).sort((a,b) => b[1]-a[1])) {
    console.log(`  ${n.toString().padStart(4)}  ${k}`);
  }
  console.log();
  console.log('By subproduct:');
  for (const [k, n] of Object.entries(bySubproduct).sort((a,b) => b[1]-a[1])) {
    console.log(`  ${n.toString().padStart(4)}  ${k}`);
  }
  console.log();
  console.log('By confidence:');
  for (const [k, n] of Object.entries(byConfidence).sort((a,b) => b[1]-a[1])) {
    console.log(`  ${n.toString().padStart(4)}  ${k}`);
  }

  const untagged = results.filter(r => !r.tags.product);
  if (untagged.length) {
    console.log(`\nUntagged (${untagged.length}):`);
    for (const r of untagged.slice(0, 20)) {
      console.log(`  ${r.doc.source.padEnd(20)} ${r.doc.href}`);
    }
    if (untagged.length > 20) console.log(`  ... and ${untagged.length - 20} more`);
  }
}

main().catch(e => { console.error(e); process.exit(1); });
