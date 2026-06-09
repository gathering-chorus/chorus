/**
 * Athena Move 0 — Zod schemas mirroring `data/athena/tree.json` (#2940).
 *
 * Single declarative source per ADR-028 I-4 (one source, multiple call shapes):
 *   - MCP tool input/output validation (chorus_tree_get / ownership_lookup / blast_radius)
 *   - HTTP handler request/response shaping
 *   - The fixture round-trip test against Move 1 SHACL (Silas's gate:arch note on #2928)
 *
 * Transition to Move 1: when SHACL lands in chorus.ttl + shapes.ttl, these schemas
 * regenerate from the SHACL contract (per the SHACL → OpenAPI generation requirement
 * in athena-subproduct-design.html). Until then, hand-synced to mirror the schema
 * documented at designing/docs/athena-subproduct-design.html.
 *
 * Canonical edges (containment direction stored on subject side):
 *   - Product → hasChild → Product       (recursive)
 *   - Product → hasDomain → Domain
 *   - Domain → hasChild → Domain          (recursive)
 *   - Domain → hosts → Service
 *   - Domain → contains → Record          (cookbook req-1)
 *   - Product → consumes → Service        (cross-cutting, optional)
 *   - {Product,Domain,Service} → atStep → ValueStreamStep
 *   - {Product,Domain,Service} → ownedBy → Role
 *   - {Product,Domain,Service} → hasDesignDoc → Document
 *
 * Dropped: inParent, inProduct, hostedBy, chorus:domain back-pointer, chorus:service
 * back-pointer (the latter two replaced by chorus:contains for records and
 * chorus:atService for external references).
 */

import { z } from 'zod';

// IRI in the chorus namespace, e.g. "chorus:athena" or "chorus:cards".
const Iri = z.string().regex(/^chorus:[a-z0-9][a-z0-9-]*$/i, {
  message:
    'IRI must be of the form "chorus:<slug>" with slug ∈ [a-zA-Z0-9-] starting alphanumeric',
});

const Status = z.enum(['exploring', 'building', 'operating', 'retiring']);

const RoleKind = z.enum(['human', 'agent']);

const ValueStreamStep = z.object({
  iri: Iri,
  label: z.string().min(1),
  inStream: Iri.optional(),
});

const Role = z.object({
  iri: Iri,
  label: z.string().min(1),
  kind: RoleKind,
  emoji: z.string().optional(),
});

// Lean shape per chorus-product-tree.html (2026-05-14). Required: iri, label,
// ownedBy, containment edges. Rich fields optional until Move 1's SHACL enforces.
const ProductSchema = z.object({
  iri: Iri,
  label: z.string().min(1),
  ownedBy: Iri,
  atStep: Iri.optional(),
  hasChild: z.array(Iri).default([]),
  hasDomain: z.array(Iri).default([]),
  // Optional rich fields (Move 1+):
  comment: z.string().optional(),
  vision: z.string().optional(),
  valueProposition: z.string().optional(),
  audience: z.array(Iri).optional(),
  status: Status.optional(),
  gaps: z.array(z.string()).optional(),
  hasDesignDoc: z.array(Iri).optional(),
  consumes: z.array(Iri).optional(),
});

const DomainSchema = z.object({
  iri: Iri,
  label: z.string().min(1),
  ownedBy: Iri.optional(),
  atStep: Iri.optional(),
  placement: z.string().optional(),
  comment: z.string().optional(),
  status: Status.optional(),
  gaps: z.array(z.string()).optional(),
  hasDesignDoc: z.array(Iri).optional(),
  hosts: z.array(Iri).optional(),
  contains: z.array(Iri).optional(),
  hasChild: z.array(Iri).optional(),
  // #3291: declarative path-prefix rules — which path subtrees this domain owns.
  // Read by the crawler (file→domain attribution) AND CI (ADR-038), never hardcoded
  // (no parallel store; OWL-as-source). Longest-prefix-wins picks the primary edge.
  hasMapsTo: z.array(z.string()).optional(),
});

const ServiceSchema = z.object({
  iri: Iri,
  label: z.string().min(1),
  ownedBy: Iri.optional(),
  atStep: Iri.optional(),
  overview: z.string().optional(),
  status: Status.optional(),
  asIs: z.string().optional(),
  toBe: z.string().optional(),
  implementationPlan: z.string().optional(),
  pathToClose: z.string().optional(),
  gaps: z.array(z.string()).optional(),
  notInScope: z.array(z.string()).optional(),
  hasDesignDoc: z.array(Iri).optional(),
});

// Instance = a leaf invokable unit under a domain (skill / hook / verb), each
// with its own owner. ADR-025's instances layer, finally populated (#3275).
// Jeff's definition: a "skill" is any named, owned, invokable unit — markdown
// skill, hook, or werk verb — so all three shapes live here under chorus:skills.
const InstanceSchema = z.object({
  iri: Iri,
  label: z.string().min(1),
  instanceType: z.enum(['skill', 'hook', 'verb']),
  inDomain: Iri,
  ownedBy: Iri,
  status: Status.optional(),
  deprecated: z.boolean().optional(),
  supersededBy: Iri.optional(),
  comment: z.string().optional(),
  // #3291: the source artifact this invokable unit maps to (verb→crate dir,
  // hook→rust module, skill→skill dir). The file↔instance join ADR-038 reads.
  mapsTo: z.string().optional(),
});

export const TreeSchema = z.object({
  schemaVersion: z.string().min(1),
  namespace: z.string().url(),
  comment: z.string().min(1),
  valueStreamSteps: z.array(ValueStreamStep).min(1),
  roles: z.array(Role).min(1),
  products: z.array(ProductSchema).min(1),
  domains: z.array(DomainSchema).min(1),
  services: z.array(ServiceSchema).default([]),
  instances: z.array(InstanceSchema).default([]),
});

export type Tree = z.infer<typeof TreeSchema>;
export type Product = z.infer<typeof ProductSchema>;
export type Domain = z.infer<typeof DomainSchema>;
export type Service = z.infer<typeof ServiceSchema>;
export type Instance = z.infer<typeof InstanceSchema>;

// #3291: file attribution result. A crawled/changed file resolves to AT MOST one
// instance (file→instance = 1, longest-prefix-wins — the ADR-038 substrate seam)
// and to 1..N domain edges (file→domain = 1:N — the crawler's coupling signal).
// Each domain edge carries its SOURCE so phase-2 import-derived edges drop in with
// NO migration (Silas's forward-compat lock). Phase 1 populates source='prefix' only.
export const AttributionSource = z.enum(['prefix', 'import', 'annotation']);
export type AttributionSourceT = z.infer<typeof AttributionSource>;

export const DomainEdgeSchema = z.object({
  domain: Iri,
  source: AttributionSource,
  primary: z.boolean().optional(), // the longest-prefix-wins primary (prefix source)
});
export type DomainEdge = z.infer<typeof DomainEdgeSchema>;

export const FileAttributionSchema = z.object({
  path: z.string(),
  instance: Iri.optional(),                                 // file→instance = 1 (ADR-038 substrate)
  instanceOwner: Iri.optional(),
  instanceKind: z.enum(['skill', 'hook', 'verb']).optional(),
  domains: z.array(DomainEdgeSchema),                       // file→domain = 1:N (crawler coupling)
});
export type FileAttribution = z.infer<typeof FileAttributionSchema>;

// Returned by chorus_ownership_lookup.
export const OwnershipResultSchema = z.object({
  iri: Iri,
  kind: z.enum(['product', 'domain', 'service', 'instance']),
  owner: Iri,
  product: Iri.optional(),
  domain: Iri.optional(),
  service: Iri.optional(),
  instance: Iri.optional(),
});
export type OwnershipResult = z.infer<typeof OwnershipResultSchema>;

// Returned by chorus_blast_radius.
export const BlastRadiusResultSchema = z.object({
  iri: Iri,
  consumers: z.array(Iri),
  dependents: z.array(Iri),
  hosts: z.array(Iri).optional(),
});
export type BlastRadiusResult = z.infer<typeof BlastRadiusResultSchema>;
