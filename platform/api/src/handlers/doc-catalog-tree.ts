/* eslint-disable @typescript-eslint/no-unnecessary-condition --
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
// Athena uses long URIs (chorusProduct, gathering, consultingProduct);
// the tagger uses lowercase enum (chorus, gathering, consulting).
// Borg is a Chorus subproduct as of 2026-04-28; not a top-level product.
// Akasha rolled into Consulting as of 2026-04-28.
const PRODUCT_LABEL_TO_ENUM: Record<string, string> = {
  'Chorus': 'chorus',
  'Chorus (Product)': 'chorus',
  'Gathering': 'gathering',
  'Consulting': 'consulting',
};

function productEnumFor(p: AthenaProduct): string {
  return PRODUCT_LABEL_TO_ENUM[p.label] ?? p.label.toLowerCase();
}

// #2627: extracted phase helpers. Each phase has single-digit cog;
// orchestrator becomes a linear sequence well under threshold 12.

export interface SubdomainIndex {
  bySubproduct: Record<string, SubdomainNode[]>;
  byProduct: Record<string, SubdomainNode[]>;
  byId: Record<string, SubdomainNode>;
}

export function buildSubdomainIndex(shape: AthenaShape): SubdomainIndex {
  const idx: SubdomainIndex = { bySubproduct: {}, byProduct: {}, byId: {} };
  for (const sd of shape.subdomains) {
    const node: SubdomainNode = { id: sd.id, label: sd.label, docCount: 0 };
    idx.byId[sd.id] = node;
    if (sd.subproduct) {
      (idx.bySubproduct[sd.subproduct] ||= []).push(node);
    } else if (sd.product) {
      (idx.byProduct[sd.product] ||= []).push(node);
    }
  }
  return idx;
}

export interface SubproductIndex {
  byProduct: Record<string, SubproductNode[]>;
  byId: Record<string, SubproductNode>;
}

export function buildSubproductIndex(shape: AthenaShape, subdomains: SubdomainIndex): SubproductIndex {
  const idx: SubproductIndex = { byProduct: {}, byId: {} };
  for (const sp of shape.subproducts) {
    const node: SubproductNode = {
      id: sp.id, label: sp.label, docCount: 0,
      subdomains: subdomains.bySubproduct[sp.id] || [],
    };
    idx.byId[sp.id] = node;
    (idx.byProduct[sp.product] ||= []).push(node);
  }
  return idx;
}

function buildProductNodes(
  shape: AthenaShape,
  subproducts: SubproductIndex,
  subdomains: SubdomainIndex,
): ProductNode[] {
  return shape.products.map(p => {
    const node: ProductNode = { id: p.id, label: p.label, docCount: 0 };
    const sps = subproducts.byProduct[p.id];
    const sds = subdomains.byProduct[p.id];
    if (sps?.length) node.subproducts = sps;
    if (sds?.length) node.subdomains = sds;
    return node;
  });
}

function indexProductsByEnum(shape: AthenaShape, productNodes: ProductNode[]): Record<string, ProductNode> {
  const byEnum: Record<string, ProductNode> = {};
  for (const p of shape.products) {
    const node = productNodes.find(n => n.id === p.id);
    if (node) byEnum[productEnumFor(p)] = node;
  }
  return byEnum;
}

function tallyOneDoc(
  doc: TaggedDoc,
  productByEnum: Record<string, ProductNode>,
  subproducts: SubproductIndex,
  subdomains: SubdomainIndex,
): boolean {
  const product = doc.tags.product ? productByEnum[doc.tags.product] : undefined;
  if (!product) return false;
  product.docCount++;
  const sp = doc.tags.subproduct ? subproducts.byId[doc.tags.subproduct] : undefined;
  if (sp) sp.docCount++;
  const sd = doc.tags.subdomain ? subdomains.byId[doc.tags.subdomain] : undefined;
  if (sd) sd.docCount++;
  return true;
}

function tallyDocCounts(
  docs: TaggedDoc[],
  productByEnum: Record<string, ProductNode>,
  subproducts: SubproductIndex,
  subdomains: SubdomainIndex,
): number {
  let untagged = 0;
  for (const doc of docs) {
    if (!tallyOneDoc(doc, productByEnum, subproducts, subdomains)) untagged++;
  }
  return untagged;
}

export function buildHierarchyTree(taggedDocs: TaggedDoc[], shape: AthenaShape): HierarchyTree {
  const subdomains = buildSubdomainIndex(shape);
  const subproducts = buildSubproductIndex(shape, subdomains);
  const productNodes = buildProductNodes(shape, subproducts, subdomains);
  const productByEnum = indexProductsByEnum(shape, productNodes);
  const untagged = tallyDocCounts(taggedDocs, productByEnum, subproducts, subdomains);
  return {
    totalDocs: taggedDocs.length,
    products: productNodes,
    untagged: { docCount: untagged },
  };
}
