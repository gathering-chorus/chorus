/**
 * Tests for Athena Move 0 tree handler (#2940).
 *
 * Unit tests cover:
 *   - loadTree: validates data/athena/tree.json against TreeSchema (Silas's
 *     gate:arch note on #2928 — fixture round-trip against Move 1 shapes)
 *   - lookupOwnership: Product / Domain / Service lookup, not-found
 *   - computeBlastRadius: Service consumers, Domain consumers (recursive),
 *     Product consumers, not-found
 *   - ownershipMap: flat iri → role table
 *   - ownedUnits: filter by role
 */

import {
  loadTree,
  _resetTreeCache,
  lookupOwnership,
  computeBlastRadius,
  ownershipMap,
  ownedUnits,
} from '../src/handlers/athena-tree';
import { TreeSchema, type Tree } from '../src/handlers/athena-tree-schemas';
import fs from 'node:fs';
import path from 'node:path';
import os from 'node:os';

const REPO_ROOT = process.env.CHORUS_ROOT || path.join(os.homedir(), 'CascadeProjects/chorus-werk/wren-2940');
const TREE_PATH = path.join(REPO_ROOT, 'data/athena/tree.json');

describe('loadTree — fixture round-trip against TreeSchema (#2928 Silas gate:arch note)', () => {
  beforeEach(() => _resetTreeCache());

  test('data/athena/tree.json parses + validates against Zod schema', () => {
    process.env.CHORUS_ROOT = REPO_ROOT;
    const tree = loadTree();
    expect(tree.products.length).toBeGreaterThan(0);
    expect(tree.domains.length).toBeGreaterThan(0);
    expect(tree.services.length).toBeGreaterThanOrEqual(0); // canonical has no Service layer until Move 5
  });

  test('every Product is owned by a Role declared in tree.roles', () => {
    process.env.CHORUS_ROOT = REPO_ROOT;
    const tree = loadTree();
    const roleIris = new Set(tree.roles.map((r) => r.iri));
    for (const p of tree.products) {
      expect(roleIris.has(p.ownedBy)).toBe(true);
    }
  });

  test('every Product.atStep (when set) resolves to a ValueStreamStep IRI', () => {
    process.env.CHORUS_ROOT = REPO_ROOT;
    const tree = loadTree();
    const stepIris = new Set(tree.valueStreamSteps.map((s) => s.iri));
    // atStep is optional on the root Product per canonical chorus-product-tree.html
    // (root contains the steps; not at one). Subproducts/domains/services may also
    // be unplaced in Move-0 partial data. Check only when present.
    for (const p of tree.products) if (p.atStep) expect(stepIris.has(p.atStep)).toBe(true);
    for (const d of tree.domains) if (d.atStep) expect(stepIris.has(d.atStep)).toBe(true);
    for (const s of tree.services) if (s.atStep) expect(stepIris.has(s.atStep)).toBe(true);
  });

  test('every Domain in Product.hasDomain is present in tree.domains', () => {
    process.env.CHORUS_ROOT = REPO_ROOT;
    const tree = loadTree();
    const domainIris = new Set(tree.domains.map((d) => d.iri));
    for (const p of tree.products) {
      for (const ref of p.hasDomain) {
        expect(domainIris.has(ref)).toBe(true);
      }
    }
  });

  test('every Service in Domain.hosts is present in tree.services', () => {
    process.env.CHORUS_ROOT = REPO_ROOT;
    const tree = loadTree();
    const serviceIris = new Set(tree.services.map((s) => s.iri));
    for (const d of tree.domains) {
      for (const ref of d.hosts ?? []) {
        expect(serviceIris.has(ref)).toBe(true);
      }
    }
  });

  test('caches by mtime — second call without file change returns same instance', () => {
    process.env.CHORUS_ROOT = REPO_ROOT;
    const a = loadTree();
    const b = loadTree();
    expect(a).toBe(b);
  });
});

// Minimal in-memory tree for the unit tests on lookup / blast-radius logic.
const TEST_TREE: Tree = TreeSchema.parse({
  schemaVersion: 'test/1',
  namespace: 'https://jeffbridwell.com/chorus#',
  comment: 'test fixture',
  valueStreamSteps: [{ iri: 'chorus:step-x', label: 'X', inStream: 'chorus:stream-test' }],
  roles: [
    { iri: 'chorus:role-alice', label: 'Alice', kind: 'agent' },
    { iri: 'chorus:role-bob', label: 'Bob', kind: 'agent' },
  ],
  products: [
    {
      iri: 'chorus:product-a',
      label: 'A',
      comment: 'product a',
      vision: 'v',
      valueProposition: 'v',
      audience: ['chorus:role-alice'],
      status: 'building',
      gaps: ['none'],
      ownedBy: 'chorus:role-alice',
      atStep: 'chorus:step-x',
      hasDesignDoc: ['chorus:doc-a'],
      hasChild: [],
      hasDomain: ['chorus:domain-d1'],
      consumes: ['chorus:service-s2'],
    },
    {
      iri: 'chorus:product-b',
      label: 'B',
      comment: 'product b',
      vision: 'v',
      valueProposition: 'v',
      audience: ['chorus:role-bob'],
      status: 'building',
      gaps: ['none'],
      ownedBy: 'chorus:role-bob',
      atStep: 'chorus:step-x',
      hasDesignDoc: ['chorus:doc-b'],
      hasChild: [],
      hasDomain: ['chorus:domain-d2'],
      consumes: [],
    },
  ],
  domains: [
    {
      iri: 'chorus:domain-d1',
      label: 'd1',
      comment: 'd1',
      ownedBy: 'chorus:role-alice',
      atStep: 'chorus:step-x',
      status: 'operating',
      gaps: ['none'],
      hasDesignDoc: ['chorus:doc-d1'],
      hosts: ['chorus:service-s1'],
      contains: [],
      hasChild: [],
    },
    {
      iri: 'chorus:domain-d2',
      label: 'd2',
      comment: 'd2',
      ownedBy: 'chorus:role-bob',
      atStep: 'chorus:step-x',
      status: 'operating',
      gaps: ['none'],
      hasDesignDoc: ['chorus:doc-d2'],
      hosts: ['chorus:service-s2'],
      contains: [],
      hasChild: [],
    },
  ],
  services: [
    {
      iri: 'chorus:service-s1',
      label: 's1',
      overview: 'o',
      ownedBy: 'chorus:role-alice',
      atStep: 'chorus:step-x',
      status: 'operating',
      asIs: 'a',
      toBe: 't',
      implementationPlan: 'p',
      pathToClose: 'c',
      gaps: ['none'],
      notInScope: ['none'],
      hasDesignDoc: ['chorus:doc-s1'],
    },
    {
      iri: 'chorus:service-s2',
      label: 's2',
      overview: 'o',
      ownedBy: 'chorus:role-bob',
      atStep: 'chorus:step-x',
      status: 'operating',
      asIs: 'a',
      toBe: 't',
      implementationPlan: 'p',
      pathToClose: 'c',
      gaps: ['none'],
      notInScope: ['none'],
      hasDesignDoc: ['chorus:doc-s2'],
    },
  ],
});

describe('lookupOwnership', () => {
  test('resolves Product IRI to its owner + product path', () => {
    const r = lookupOwnership(TEST_TREE, 'chorus:product-a');
    expect(r).toEqual({
      iri: 'chorus:product-a',
      kind: 'product',
      owner: 'chorus:role-alice',
      product: 'chorus:product-a',
    });
  });

  test('resolves Domain IRI to its owner + containing product', () => {
    const r = lookupOwnership(TEST_TREE, 'chorus:domain-d1');
    expect(r).toEqual({
      iri: 'chorus:domain-d1',
      kind: 'domain',
      owner: 'chorus:role-alice',
      product: 'chorus:product-a',
      domain: 'chorus:domain-d1',
    });
  });

  test('resolves Service IRI to its hosting Domain + containing Product', () => {
    const r = lookupOwnership(TEST_TREE, 'chorus:service-s1');
    expect(r).toEqual({
      iri: 'chorus:service-s1',
      kind: 'service',
      owner: 'chorus:role-alice',
      product: 'chorus:product-a',
      domain: 'chorus:domain-d1',
      service: 'chorus:service-s1',
    });
  });

  test('returns null for unknown IRI', () => {
    expect(lookupOwnership(TEST_TREE, 'chorus:nope')).toBeNull();
  });
});

describe('computeBlastRadius', () => {
  test('Service consumers = Products with consumes edge to it', () => {
    const r = computeBlastRadius(TEST_TREE, 'chorus:service-s2');
    expect(r).not.toBeNull();
    expect(r!.consumers).toContain('chorus:product-a');
    expect(r!.dependents).toEqual([]);
  });

  test('Domain consumers = parent Product + recursive consumers of hosted Services', () => {
    const r = computeBlastRadius(TEST_TREE, 'chorus:domain-d2');
    expect(r).not.toBeNull();
    // Direct: product-b owns d2 (via hasDomain)
    expect(r!.consumers).toContain('chorus:product-b');
    // Recursive: product-a consumes service-s2 which d2 hosts
    expect(r!.consumers).toContain('chorus:product-a');
  });

  test('returns null for unknown IRI', () => {
    expect(computeBlastRadius(TEST_TREE, 'chorus:nope')).toBeNull();
  });
});

describe('ownershipMap', () => {
  test('flattens every Product / Domain / Service into iri → role-iri', () => {
    const m = ownershipMap(TEST_TREE);
    expect(m['chorus:product-a']).toBe('chorus:role-alice');
    expect(m['chorus:product-b']).toBe('chorus:role-bob');
    expect(m['chorus:domain-d1']).toBe('chorus:role-alice');
    expect(m['chorus:service-s2']).toBe('chorus:role-bob');
  });
});

describe('ownedUnits', () => {
  test("returns only units owned by the role", () => {
    const r = ownedUnits(TEST_TREE, 'chorus:role-alice');
    expect(r.products.map((p) => p.iri)).toEqual(['chorus:product-a']);
    expect(r.domains.map((d) => d.iri)).toEqual(['chorus:domain-d1']);
    expect(r.services.map((s) => s.iri)).toEqual(['chorus:service-s1']);
  });

  test('returns empty arrays for a role with no owned units', () => {
    const r = ownedUnits(TEST_TREE, 'chorus:role-eve');
    expect(r.products).toEqual([]);
    expect(r.domains).toEqual([]);
    expect(r.services).toEqual([]);
  });
});
