/**
 * doc-catalog-tree unit tests (#2521).
 */
import {
  buildHierarchyTree,
  buildSubdomainIndex,
  buildSubproductIndex,
  type TaggedDoc,
  type AthenaShape,
} from '../../src/handlers/doc-catalog-tree';

const ATHENA: AthenaShape = {
  products: [
    { id: 'chorusProduct', label: 'Chorus' },
    { id: 'gathering', label: 'Gathering' },
    { id: 'borgProduct', label: 'Borg' },
  ],
  subproducts: [
    { id: 'loom', label: 'Loom', product: 'chorusProduct' },
    { id: 'athena-product', label: 'Athena', product: 'chorusProduct' },
    { id: 'werk-product', label: 'Werk', product: 'chorusProduct' },
  ],
  subdomains: [
    { id: 'loom-decisions', label: 'Decisions', subproduct: 'loom' },
    { id: 'loom-principles', label: 'Principles', subproduct: 'loom' },
    { id: 'athena-domain', label: 'Athena', subproduct: 'athena-product' },
    { id: 'cards-service', label: 'Cards', subproduct: 'werk-product' },
    { id: 'photos-domain', label: 'Photos', subproduct: null, product: 'gathering' },
    { id: 'blog-domain', label: 'Blog', subproduct: null, product: 'gathering' },
  ],
};

function doc(href: string, product?: string, subproduct?: string, subdomain?: string): TaggedDoc {
  return { href, source: 'test', title: href,
    tags: { confidence: 'high', signal: 'path', product, subproduct, subdomain } };
}

describe('buildHierarchyTree (#2521)', () => {
  test('subdomain doc rolls up through subproduct → product', () => {
    const tree = buildHierarchyTree(
      [doc('/x.md', 'chorus', 'loom', 'loom-decisions')],
      ATHENA,
    );
    const chorus = tree.products.find(p => p.label === 'Chorus');
    expect(chorus).toBeDefined();
    expect(chorus!.docCount).toBe(1);
    const loom = chorus!.subproducts!.find(s => s.id === 'loom');
    expect(loom!.docCount).toBe(1);
    const decisions = loom!.subdomains!.find(s => s.id === 'loom-decisions');
    expect(decisions!.docCount).toBe(1);
  });

  test('gathering: subdomain attaches directly to product (no subproduct level)', () => {
    const tree = buildHierarchyTree(
      [doc('/p.md', 'gathering', undefined, 'photos-domain')],
      ATHENA,
    );
    const gathering = tree.products.find(p => p.label === 'Gathering');
    expect(gathering!.docCount).toBe(1);
    expect(gathering!.subproducts).toBeUndefined();
    expect(gathering!.subdomains!.find(s => s.id === 'photos-domain')!.docCount).toBe(1);
  });

  test('untagged docs go to untagged bucket', () => {
    const tree = buildHierarchyTree([doc('/x.md')], ATHENA);
    expect(tree.untagged.docCount).toBe(1);
  });

  test('product-only tag counts at product node, no subproduct rollup', () => {
    const tree = buildHierarchyTree([doc('/x.md', 'chorus')], ATHENA);
    const chorus = tree.products.find(p => p.label === 'Chorus');
    expect(chorus!.docCount).toBe(1);
  });

  test('counts roll up across multiple docs', () => {
    const docs = [
      doc('/a', 'chorus', 'loom', 'loom-decisions'),
      doc('/b', 'chorus', 'loom', 'loom-principles'),
      doc('/c', 'chorus', 'athena-product', 'athena-domain'),
    ];
    const tree = buildHierarchyTree(docs, ATHENA);
    const chorus = tree.products.find(p => p.label === 'Chorus');
    expect(chorus!.docCount).toBe(3);
    const loom = chorus!.subproducts!.find(s => s.id === 'loom');
    expect(loom!.docCount).toBe(2);
  });

  test('totalDocs published at root', () => {
    const tree = buildHierarchyTree(
      [doc('/a', 'chorus'), doc('/b', 'gathering'), doc('/c')],
      ATHENA,
    );
    expect(tree.totalDocs).toBe(3);
  });
});

// #2627: refactor extracted these phase helpers; tests pin the contracts
// so the orchestrator can shrink without losing the wiring.
describe('buildHierarchyTree extracted helpers (#2627)', () => {
  test('buildSubdomainIndex routes by subproduct, by product, and by id', () => {
    const idx = buildSubdomainIndex(ATHENA);
    expect(idx.byId['loom-decisions']).toBeDefined();
    expect(idx.bySubproduct['loom']).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ id: 'loom-decisions' }),
        expect.objectContaining({ id: 'loom-principles' }),
      ]),
    );
    // Subdomain attached directly to a product (not via subproduct) lands in byProduct
    const productOnly: AthenaShape = {
      ...ATHENA,
      subdomains: [{ id: 'orphan-sd', label: 'Orphan', product: 'gathering' }],
    };
    const idx2 = buildSubdomainIndex(productOnly);
    expect(idx2.byProduct['gathering']).toEqual([expect.objectContaining({ id: 'orphan-sd' })]);
    expect(idx2.bySubproduct['gathering']).toBeUndefined();
  });

  test('buildSubproductIndex carries subdomains forward and indexes by product+id', () => {
    const subIdx = buildSubdomainIndex(ATHENA);
    const spIdx = buildSubproductIndex(ATHENA, subIdx);
    expect(spIdx.byId['loom']?.subdomains.map(s => s.id)).toEqual(
      expect.arrayContaining(['loom-decisions', 'loom-principles']),
    );
    expect(spIdx.byProduct['chorusProduct']?.map(sp => sp.id)).toEqual(
      expect.arrayContaining(['loom', 'athena-product', 'werk-product']),
    );
  });
});
