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
  attributeFile,
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

  test('#3275: skill/hook/verb instances loaded + ownership resolves to validated owners', () => {
    process.env.CHORUS_ROOT = REPO_ROOT;
    const tree = loadTree();
    // C: the instance layer is populated (94 skills+hooks+verbs — was 95 before
    // the acp skill was retired, #3422)
    expect(tree.instances.length).toBeGreaterThanOrEqual(94);
    // AC3: ownership_lookup answers at the leaf, with Jeff's validated owners
    expect(lookupOwnership(tree, 'chorus:skill-demo')?.owner).toBe('chorus:role-wren');
    expect(lookupOwnership(tree, 'chorus:verb-werk-commit')?.owner).toBe('chorus:role-kade');
    expect(lookupOwnership(tree, 'chorus:verb-werk-demo')?.owner).toBe('chorus:role-wren'); // proving verb → wren
    expect(lookupOwnership(tree, 'chorus:hook-icd-write-gate')?.owner).toBe('chorus:role-kade'); // convergence, not silas
    expect(lookupOwnership(tree, 'chorus:hook-search-hierarchy')?.owner).toBe('chorus:role-wren');
    // acp fully retired (#3422) — the skill-acp node is removed from the tree
    // (it was deprecated/supersededBy chorus-werk; the skill is now gone entirely)
    const acp = tree.instances.find((i) => i.iri === 'chorus:skill-acp');
    expect(acp).toBeUndefined();
    // AC1: top rooted — chorus parent has the children, no name collision
    const root = tree.products.find((p) => p.iri === 'chorus:chorus');
    expect(root?.hasChild).toContain('chorus:chorus-chorus');
    expect(tree.products.filter((p) => p.iri === 'chorus:chorus').length).toBe(1);
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
    const unresolved = [
      ...tree.products.filter((p) => p.atStep && !stepIris.has(p.atStep)).map((p) => p.iri),
      ...tree.domains.filter((d) => d.atStep && !stepIris.has(d.atStep)).map((d) => d.iri),
      ...tree.services.filter((s) => s.atStep && !stepIris.has(s.atStep)).map((s) => s.iri),
    ];
    expect(unresolved).toEqual([]);
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
      hasMapsTo: ['platform/d1/'],
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
      hasMapsTo: ['platform/'],
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
  instances: [
    {
      iri: 'chorus:skill-test-demo',
      label: 'test-demo',
      instanceType: 'skill',
      inDomain: 'chorus:domain-d1',
      ownedBy: 'chorus:role-alice',
      mapsTo: 'platform/d1/demo/',
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

  test('resolves Instance IRI to its owner + containing domain/product (#3275)', () => {
    const r = lookupOwnership(TEST_TREE, 'chorus:skill-test-demo');
    expect(r).toEqual({
      iri: 'chorus:skill-test-demo',
      kind: 'instance',
      owner: 'chorus:role-alice',
      product: 'chorus:product-a',
      domain: 'chorus:domain-d1',
      instance: 'chorus:skill-test-demo',
    });
  });

  test('returns null for unknown IRI', () => {
    expect(lookupOwnership(TEST_TREE, 'chorus:nope')).toBeNull();
  });
});

describe('attributeFile (#3291)', () => {
  test('file→instance resolves to owner + kind (1, longest-prefix-wins)', () => {
    const a = attributeFile(TEST_TREE, 'platform/d1/demo/main.ts');
    expect(a.instance).toBe('chorus:skill-test-demo');
    expect(a.instanceOwner).toBe('chorus:role-alice');
    expect(a.instanceKind).toBe('skill');
  });

  test('file→domain is 1:N (source=prefix), longest match is primary — not collapsed', () => {
    // platform/d1/demo/main.ts is under d1 (platform/d1/) AND d2 (platform/).
    const a = attributeFile(TEST_TREE, 'platform/d1/demo/main.ts');
    const byDomain = Object.fromEntries(a.domains.map((e) => [e.domain, e]));
    expect(byDomain['chorus:domain-d1']).toMatchObject({ source: 'prefix', primary: true });
    expect(byDomain['chorus:domain-d2']).toMatchObject({ source: 'prefix', primary: false });
    expect(a.domains.length).toBe(2); // 1:N preserved (the forward-compat lock)
    expect(a.domains.every((e) => e.source === 'prefix')).toBe(true); // phase 1 = prefix only
    // AT-MOST-ONE-PRIMARY (Silas's invariant): never 2 primaries → ownership unambiguous.
    expect(a.domains.filter((e) => e.primary).length).toBeLessThanOrEqual(1);
  });

  test('at-most-one-primary holds even on equal-length prefix matches (#3291 invariant)', () => {
    // Two domains with SAME-length prefixes both matching the file — only one is primary.
    const tied: Tree = {
      ...TEST_TREE,
      domains: [
        { iri: 'chorus:domain-d1', label: 'd1', ownedBy: 'chorus:role-alice', hasMapsTo: ['lib/x/'] },
        { iri: 'chorus:domain-d2', label: 'd2', ownedBy: 'chorus:role-bob', hasMapsTo: ['lib/x/'] },
      ],
      instances: [],
    };
    const a = attributeFile(tied, 'lib/x/thing.ts');
    expect(a.domains.length).toBe(2);
    expect(a.domains.filter((e) => e.primary).length).toBe(1); // exactly one, deterministic
  });

  test('unmatched file → no instance, empty domains (honest "needs mapsTo")', () => {
    const a = attributeFile(TEST_TREE, 'some/unmapped/file.ts');
    expect(a.instance).toBeUndefined();
    expect(a.domains).toEqual([]);
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

  test('Product is never its own consumer — self-reference filtered (#2957)', () => {
    // product-a hasDomain d1; d1 hosts s1 which nothing consumes. The only path
    // back is the domain inverse re-adding product-a itself — must be filtered.
    const r = computeBlastRadius(TEST_TREE, 'chorus:product-a');
    expect(r).not.toBeNull();
    expect(r!.consumers).not.toContain('chorus:product-a');
    expect(r!.consumers).toEqual([]);
  });

  test('Product keeps real cross-product consumers while filtering self (#2957)', () => {
    // product-b hasDomain d2; d2 hosts s2; product-a consumes s2.
    // → product-a is a genuine consumer; product-b (self) is filtered out.
    const r = computeBlastRadius(TEST_TREE, 'chorus:product-b');
    expect(r).not.toBeNull();
    expect(r!.consumers).toContain('chorus:product-a');
    expect(r!.consumers).not.toContain('chorus:product-b');
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
