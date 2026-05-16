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

// IRI in the chorus namespace, e.g. "chorus:athena" or "chorus:domain-cards".
const Iri = z.string().regex(/^chorus:[a-z0-9][a-z0-9-]*$/i, {
  message:
    'IRI must be of the form "chorus:<slug>" with slug ∈ [a-zA-Z0-9-] starting alphanumeric',
});

const Status = z.enum(['exploring', 'building', 'operating', 'retiring']);

const RoleKind = z.enum(['human', 'agent']);

const ValueStreamStep = z.object({
  iri: Iri,
  label: z.string().min(1),
  inStream: Iri,
});

const Role = z.object({
  iri: Iri,
  label: z.string().min(1),
  kind: RoleKind,
  emoji: z.string().optional(),
});

const ProductSchema = z.object({
  iri: Iri,
  label: z.string().min(1),
  comment: z.string().min(1),
  vision: z.string().min(1),
  valueProposition: z.string().min(1),
  audience: z.array(Iri).min(1),
  status: Status,
  gaps: z.array(z.string().min(1)).min(1),
  ownedBy: Iri,
  atStep: Iri,
  hasDesignDoc: z.array(Iri).min(1),
  // Containment edges stored on Product side
  hasChild: z.array(Iri).default([]),
  hasDomain: z.array(Iri).default([]),
  consumes: z.array(Iri).default([]),
});

const DomainSchema = z.object({
  iri: Iri,
  label: z.string().min(1),
  comment: z.string().min(1),
  ownedBy: Iri,
  atStep: Iri,
  status: Status,
  gaps: z.array(z.string().min(1)).min(1),
  hasDesignDoc: z.array(Iri).min(1),
  // Containment edges stored on Domain side
  hosts: z.array(Iri).default([]),
  contains: z.array(Iri).default([]), // cookbook req-1: every Domain links records via chorus:contains
  hasChild: z.array(Iri).default([]),
});

const ServiceSchema = z.object({
  iri: Iri,
  label: z.string().min(1),
  overview: z.string().min(1),
  ownedBy: Iri,
  atStep: Iri,
  status: Status,
  asIs: z.string().min(1),
  toBe: z.string().min(1),
  implementationPlan: z.string().min(1),
  pathToClose: z.string().min(1),
  gaps: z.array(z.string().min(1)).min(1),
  notInScope: z.array(z.string().min(1)).min(1),
  hasDesignDoc: z.array(Iri).min(1),
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
});

export type Tree = z.infer<typeof TreeSchema>;
export type Product = z.infer<typeof ProductSchema>;
export type Domain = z.infer<typeof DomainSchema>;
export type Service = z.infer<typeof ServiceSchema>;

// Returned by chorus_ownership_lookup.
export const OwnershipResultSchema = z.object({
  iri: Iri,
  kind: z.enum(['product', 'domain', 'service']),
  owner: Iri,
  product: Iri.optional(),
  domain: Iri.optional(),
  service: Iri.optional(),
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
