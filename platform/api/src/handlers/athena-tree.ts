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
 * Path resolution: imported from lib/chorus-paths (the one root source, #3197).
 * Cached after first read until the file mtime changes (operational reload
 * without service restart).
 */

import fs from 'node:fs';
import path from 'node:path';
import { CHORUS_ROOT } from '../lib/chorus-paths'; // #3197 — single root source
import {
  TreeSchema,
  type Tree,
  type Product,
  type Domain,
  type Service,
  type OwnershipResult,
  type BlastRadiusResult,
  type FileAttribution,
  type DomainEdge,
} from './athena-tree-schemas';

interface CacheEntry {
  tree: Tree;
  mtimeMs: number;
}

let cache: CacheEntry | null = null;

function repoRoot(): string {
  return CHORUS_ROOT;
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
  const instance = (tree.instances).find((i) => i.iri === iri);
  if (instance) {
    const owningProduct = tree.products.find((p) =>
      p.hasDomain.includes(instance.inDomain),
    );
    return {
      iri,
      kind: 'instance',
      owner: instance.ownedBy,
      product: owningProduct?.iri,
      domain: instance.inDomain,
      instance: iri,
    };
  }
  return null;
}

/**
 * #3291: attribute a file path to its place in the model.
 *  - file→INSTANCE = 1 (longest-prefix-wins on Instance.mapsTo) — the ADR-038
 *    substrate seam: a changed impl file → its instance → owner + kind.
 *  - file→DOMAIN = 1:N (Domain.hasMapsTo prefixes) — the crawler's coupling
 *    signal. PHASE 1 emits prefix-source edges only (longest = primary); phase 2
 *    adds source='import' cross-cutting edges, phase 3 source='annotation'.
 * A file with no match yields empty edges (the honest "needs mapsTo" signal —
 * never silently attributed). The result shape is 1:N + source from day one so
 * phase 2 drops in with no migration (the forward-compat lock).
 */
export function attributeFile(tree: Tree, filePath: string): FileAttribution {
  const norm = filePath.replace(/^\.?\//, '');
  const underPrefix = (p: string): boolean => {
    const pp = p.replace(/^\.?\//, '');
    return norm === pp || norm.startsWith(pp.endsWith('/') ? pp : pp + '/');
  };

  // file→instance: longest mapsTo prefix wins (exactly one answer).
  let bestInst: { iri: string; len: number } | null = null;
  for (const i of tree.instances) {
    if (!i.mapsTo || !underPrefix(i.mapsTo)) continue;
    const len = i.mapsTo.replace(/^\.?\//, '').length;
    if (!bestInst || len > bestInst.len) bestInst = { iri: i.iri, len };
  }
  const inst = bestInst ? (tree.instances).find((i) => i.iri === bestInst!.iri) : undefined;

  // file→domain: prefix-source edges (phase 1). Longest matching prefix = primary;
  // additional matches stay as (non-primary) edges — 1:N, never collapsed to 1:1.
  const matches: Array<{ domain: string; len: number }> = [];
  for (const d of tree.domains) {
    for (const pre of d.hasMapsTo ?? []) {
      if (underPrefix(pre)) matches.push({ domain: d.iri, len: pre.replace(/^\.?\//, '').length });
    }
  }
  // AT-MOST-ONE-PRIMARY (Silas's invariant): exactly one prefix edge is primary —
  // the longest match, ties broken by domain iri for determinism. So "which domain
  // owns this file" is never ambiguous (0 primaries if no match, else exactly 1).
  // PHASE-2 CONTRACT: import-source edges are NEVER primary — the primary is always
  // the prefix home. Only prefix matches are considered here, so a future import
  // edge structurally cannot steal primary.
  let primaryDomain = '';
  let primaryLen = -1;
  for (const m of matches) {
    if (m.len > primaryLen || (m.len === primaryLen && m.domain < primaryDomain)) {
      primaryLen = m.len;
      primaryDomain = m.domain;
    }
  }
  const domains: DomainEdge[] = matches.map((m) => ({
    domain: m.domain,
    source: 'prefix' as const,
    primary: m.domain === primaryDomain,
  }));

  return {
    path: filePath,
    instance: inst?.iri,
    instanceOwner: inst?.ownedBy,
    instanceKind: inst?.instanceType,
    domains,
  };
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
// cog-override: multi-type graph traversal — structurally complex, not simplifiable without losing semantics
function serviceBlastRadius(tree: Tree, iri: string): BlastRadiusResult {
  const consumers = new Set<string>();
  for (const p of tree.products) {
    if ((p.consumes ?? []).includes(iri)) consumers.add(p.iri);
  }
  return { iri, consumers: [...consumers], dependents: [], hosts: [] };
}

function consumersFromHosts(tree: Tree, hosts: string[]): string[] {
  const out = new Set<string>();
  for (const serviceIri of hosts) {
    const inner = computeBlastRadius(tree, serviceIri);
    if (inner) for (const c of inner.consumers) out.add(c);
  }
  return [...out];
}

function domainBlastRadius(tree: Tree, iri: string): BlastRadiusResult {
  const consumers = new Set<string>();
  for (const p of tree.products) {
    if (p.hasDomain.includes(iri)) consumers.add(p.iri);
  }
  const domain = tree.domains.find((d) => d.iri === iri);
  const hosts = (domain?.hosts ?? []) as string[];
  for (const c of consumersFromHosts(tree, hosts)) consumers.add(c);
  return { iri, consumers: [...consumers], dependents: [], hosts };
}

function consumersFromDomains(tree: Tree, domains: string[], exclude: string): string[] {
  const out = new Set<string>();
  for (const domainIri of domains) {
    const inner = computeBlastRadius(tree, domainIri);
    if (inner) for (const c of inner.consumers) if (c !== exclude) out.add(c);
  }
  return [...out];
}

function productBlastRadius(tree: Tree, iri: string): BlastRadiusResult {
  const consumers = new Set<string>();
  const product = tree.products.find((p) => p.iri === iri)!;
  for (const p of tree.products) {
    if (p.hasChild.includes(iri)) consumers.add(p.iri);
  }
  for (const c of consumersFromDomains(tree, product.hasDomain, iri)) consumers.add(c);
  return { iri, consumers: [...consumers], dependents: [] };
}

export function computeBlastRadius(tree: Tree, iri: string): BlastRadiusResult | null {
  const ownership = lookupOwnership(tree, iri);
  if (!ownership) return null;
  if (ownership.kind === 'service') return serviceBlastRadius(tree, iri);
  if (ownership.kind === 'domain') return domainBlastRadius(tree, iri);
  return productBlastRadius(tree, iri);
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
