/**
 * Athena Move 0 — handler that loads + serves `data/athena/tree.json` (#2940).
 *
 * Three operations exposed via this handler family:
 *   - GET tree              → full tree (TreeSchema-validated)
 *   - GET ownership(iri)    → owner role + path (product/domain/service)
 *   - GET blast-radius(iri) → inferred consumers + dependents (recursive)
 *
 * Wired to MCP via `chorus_tree_get` / `chorus_ownership_lookup` /
 * `chorus_blast_radius` in mcp/server.ts.
 *
 * Path resolution: process.env.CHORUS_ROOT || ~/CascadeProjects/chorus —
 * matches the pattern used by chorus-cost.ts / chorus-perf.ts. Cached after
 * first read until the file mtime changes (operational reload without
 * service restart).
 */

import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  TreeSchema,
  type Tree,
  type Product,
  type Domain,
  type Service,
  type OwnershipResult,
  type BlastRadiusResult,
} from './athena-tree-schemas';

interface CacheEntry {
  tree: Tree;
  mtimeMs: number;
}

let cache: CacheEntry | null = null;

function repoRoot(): string {
  return process.env.CHORUS_ROOT || path.join(os.homedir(), 'CascadeProjects/chorus');
}

function treePath(): string {
  return path.join(repoRoot(), 'data/athena/tree.json');
}

export interface LoadTreeDeps {
  readFileSync?: (p: string, enc: 'utf8') => string;
  statSync?: (p: string) => { mtimeMs: number };
}

/**
 * Load + validate tree.json. Returns cached value if the file mtime is unchanged
 * since last read; re-reads + re-validates otherwise.
 *
 * Throws ZodError with detailed path if validation fails — the Move-0 source
 * of truth must conform to the schema, refusal is loud.
 */
export function loadTree(deps: LoadTreeDeps = {}): Tree {
  const readFileSync = deps.readFileSync ?? fs.readFileSync;
  const statSync = deps.statSync ?? fs.statSync;
  const p = treePath();
  const stat = statSync(p);
  if (cache && cache.mtimeMs === stat.mtimeMs) {
    return cache.tree;
  }
  const raw = readFileSync(p, 'utf8');
  const parsed = JSON.parse(raw);
  const tree = TreeSchema.parse(parsed); // throws on schema violation
  cache = { tree, mtimeMs: stat.mtimeMs };
  return tree;
}

// Clear cache — for tests that swap the tree underneath us.
export function _resetTreeCache(): void {
  cache = null;
}

/**
 * Resolve an IRI to its owner + path in the structural model.
 *
 * Search order: products → domains → services. Returns null if no match.
 * For a Domain match, walks Product[].hasDomain to find the containing
 * Product. For a Service match, walks Domain[].hosts to find the
 * containing Domain, then up to the containing Product.
 */
export function lookupOwnership(tree: Tree, iri: string): OwnershipResult | null {
  const product = tree.products.find((p) => p.iri === iri);
  if (product) {
    return {
      iri,
      kind: 'product',
      owner: product.ownedBy,
      product: iri,
    };
  }
  const domain = tree.domains.find((d) => d.iri === iri);
  if (domain) {
    const owningProduct = tree.products.find((p) => p.hasDomain.includes(iri));
    return {
      iri,
      kind: 'domain',
      owner: domain.ownedBy ?? '',
      product: owningProduct?.iri,
      domain: iri,
    };
  }
  const service = tree.services.find((s) => s.iri === iri);
  if (service) {
    const hostingDomain = tree.domains.find((d) => (d.hosts ?? []).includes(iri));
    const owningProduct = hostingDomain
      ? tree.products.find((p) => p.hasDomain.includes(hostingDomain.iri))
      : undefined;
    return {
      iri,
      kind: 'service',
      owner: service.ownedBy ?? '',
      product: owningProduct?.iri,
      domain: hostingDomain?.iri,
      service: iri,
    };
  }
  return null;
}

/**
 * Compute blast-radius for an IRI.
 *
 * - For a Service: consumers = Products with `consumes` edge to this Service.
 * - For a Domain: consumers = (a) Products with `hasDomain → this`,
 *                              (b) Services hosted by this Domain whose
 *                                  consumers we then collect recursively.
 * - For a Product: consumers = (recursively) all consumers of its hosted
 *                  Services + Products that hasChild this Product.
 *
 * Dependents = direct Service.dependsOn would surface here when stored;
 * tree.json doesn't carry that yet so dependents is empty until populated.
 *
 * Returns null if IRI not found.
 */
export function computeBlastRadius(tree: Tree, iri: string): BlastRadiusResult | null {
  const ownership = lookupOwnership(tree, iri);
  if (!ownership) return null;

  const consumers = new Set<string>();

  if (ownership.kind === 'service') {
    for (const p of tree.products) {
      if ((p.consumes ?? []).includes(iri)) consumers.add(p.iri);
    }
    return { iri, consumers: [...consumers], dependents: [], hosts: [] };
  }

  if (ownership.kind === 'domain') {
    for (const p of tree.products) {
      if (p.hasDomain.includes(iri)) consumers.add(p.iri);
    }
    const domain = tree.domains.find((d) => d.iri === iri);
    const hosts = (domain?.hosts ?? []) as string[];
    for (const serviceIri of hosts) {
      const inner = computeBlastRadius(tree, serviceIri);
      if (inner) for (const c of inner.consumers) consumers.add(c);
    }
    return { iri, consumers: [...consumers], dependents: [], hosts };
  }

  // product
  const product = tree.products.find((p) => p.iri === iri)!;
  for (const p of tree.products) {
    if (p.hasChild.includes(iri)) consumers.add(p.iri);
  }
  for (const domainIri of product.hasDomain) {
    const inner = computeBlastRadius(tree, domainIri);
    if (inner) for (const c of inner.consumers) consumers.add(c);
  }
  return { iri, consumers: [...consumers], dependents: [] };
}

/**
 * Flat ownership map: { iri → role-iri } across all Products, Domains, Services.
 * Useful for SessionStart envelope injection.
 */
export function ownershipMap(tree: Tree): Record<string, string> {
  const map: Record<string, string> = {};
  for (const p of tree.products) map[p.iri] = p.ownedBy;
  for (const d of tree.domains) if (d.ownedBy) map[d.iri] = d.ownedBy;
  for (const s of tree.services) if (s.ownedBy) map[s.iri] = s.ownedBy;
  return map;
}

/**
 * Owned-units for a role: Products + Domains + Services where ownedBy = role.
 * Used by SessionStart envelope injection.
 */
export function ownedUnits(
  tree: Tree,
  roleIri: string,
): { products: Product[]; domains: Domain[]; services: Service[] } {
  return {
    products: tree.products.filter((p) => p.ownedBy === roleIri),
    domains: tree.domains.filter((d) => d.ownedBy === roleIri),
    services: tree.services.filter((s) => s.ownedBy === roleIri),
  };
}
