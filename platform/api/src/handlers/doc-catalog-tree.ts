/* eslint-disable security/detect-object-injection, @typescript-eslint/no-unnecessary-condition --
 * Maps keyed by Athena product/subproduct/subdomain IDs from a validated
 * shape (server composes from Athena queries; tests pass fixture). Not
 * user input. Defensive existence checks intentional over dynamic data.
 */
/**
 * doc-catalog-tree — pure function that builds the hierarchy tree (#2521).
 *
 * Inputs:
 *   taggedDocs   — catalog entries with DocTags from the tagger
 *   athenaShape  — products/subproducts/subdomains as Athena currently knows them
 *
 * Output:
 *   { totalDocs, products[], untagged }
 *   where each product carries docCount + optional subproducts[] / subdomains[].
 *   Counts roll up: subdomain doc increments subproduct AND product.
 */

import type { DocTags } from './doc-tagger';

export interface TaggedDoc {
  href: string;
  source: string;
  title: string;
  tags: DocTags;
}

export interface AthenaProduct { id: string; label: string; }
export interface AthenaSubproduct { id: string; label: string; product: string; }
export interface AthenaSubdomain {
  id: string;
  label: string;
  subproduct: string | null;
  product?: string;  // for subdomains that attach directly to a product
}

export interface AthenaShape {
  products: AthenaProduct[];
  subproducts: AthenaSubproduct[];
  subdomains: AthenaSubdomain[];
}

export interface SubdomainNode {
  id: string;
  label: string;
  docCount: number;
}

export interface SubproductNode {
  id: string;
  label: string;
  docCount: number;
  subdomains: SubdomainNode[];
}

export interface ProductNode {
  id: string;
  label: string;
  docCount: number;
  subproducts?: SubproductNode[];
  subdomains?: SubdomainNode[];
}

export interface HierarchyTree {
  totalDocs: number;
  products: ProductNode[];
  untagged: { docCount: number };
}

// Map Athena product IDs to the tagger's product enum values.
// Athena uses long URIs (chorusProduct, gathering, borgProduct, akashaProduct);
// the tagger uses lowercase enum (chorus, gathering, borg, akasha).
const PRODUCT_LABEL_TO_ENUM: Record<string, string> = {
  'Chorus': 'chorus',
  'Chorus (Product)': 'chorus',
  'Gathering': 'gathering',
  'Borg': 'borg',
  'Akasha': 'akasha',
};

function productEnumFor(p: AthenaProduct): string {
  return PRODUCT_LABEL_TO_ENUM[p.label] ?? p.label.toLowerCase();
}

// eslint-disable-next-line complexity
export function buildHierarchyTree(taggedDocs: TaggedDoc[], shape: AthenaShape): HierarchyTree {
  // Initialize product/subproduct/subdomain nodes from the Athena shape.
  // Subdomains attach to subproduct when present, else directly to product.
  const subdomainsBySubproduct: Record<string, SubdomainNode[]> = {};
  const subdomainsByProduct: Record<string, SubdomainNode[]> = {};
  const subdomainNodeById: Record<string, SubdomainNode> = {};

  for (const sd of shape.subdomains) {
    const node: SubdomainNode = { id: sd.id, label: sd.label, docCount: 0 };
    subdomainNodeById[sd.id] = node;
    if (sd.subproduct) {
      if (!subdomainsBySubproduct[sd.subproduct]) subdomainsBySubproduct[sd.subproduct] = [];
      subdomainsBySubproduct[sd.subproduct].push(node);
    } else if (sd.product) {
      if (!subdomainsByProduct[sd.product]) subdomainsByProduct[sd.product] = [];
      subdomainsByProduct[sd.product].push(node);
    }
  }

  const subproductsByProduct: Record<string, SubproductNode[]> = {};
  const subproductNodeById: Record<string, SubproductNode> = {};
  for (const sp of shape.subproducts) {
    const node: SubproductNode = {
      id: sp.id, label: sp.label, docCount: 0,
      subdomains: subdomainsBySubproduct[sp.id] || [],
    };
    subproductNodeById[sp.id] = node;
    if (!subproductsByProduct[sp.product]) subproductsByProduct[sp.product] = [];
    subproductsByProduct[sp.product].push(node);
  }

  const productNodes: ProductNode[] = shape.products.map(p => {
    const subproducts = subproductsByProduct[p.id];
    const directSubdomains = subdomainsByProduct[p.id];
    const node: ProductNode = { id: p.id, label: p.label, docCount: 0 };
    if (subproducts && subproducts.length) node.subproducts = subproducts;
    if (directSubdomains && directSubdomains.length) node.subdomains = directSubdomains;
    return node;
  });

  // Build a label→ProductNode index by both Athena id and tagger enum
  const productNodeByEnum: Record<string, ProductNode> = {};
  for (const p of shape.products) {
    const enumName = productEnumFor(p);
    const node = productNodes.find(n => n.id === p.id);
    if (node) productNodeByEnum[enumName] = node;
  }

  let untagged = 0;
  for (const doc of taggedDocs) {
    if (!doc.tags.product) { untagged++; continue; }
    const product = productNodeByEnum[doc.tags.product];
    if (!product) { untagged++; continue; }
    product.docCount++;
    if (doc.tags.subproduct) {
      const sp = subproductNodeById[doc.tags.subproduct];
      if (sp) sp.docCount++;
    }
    if (doc.tags.subdomain) {
      const sd = subdomainNodeById[doc.tags.subdomain];
      if (sd) sd.docCount++;
    }
  }

  return {
    totalDocs: taggedDocs.length,
    products: productNodes,
    untagged: { docCount: untagged },
  };
}
