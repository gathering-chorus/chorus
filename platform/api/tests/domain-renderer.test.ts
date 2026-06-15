/**
 * #3420/#3351 — page-level unit coverage for the GENERATED Athena domain page's renderer
 * (public/js/domain-renderer.js). The renderer fills the generated shell; it exports its
 * PURE builders for node. We assert they produce the right #3415 system.css structure from
 * mock endpoint data, with honest empty states and NO bespoke styling.
 */
// eslint-disable-next-line @typescript-eslint/no-var-requires
const R = require('../public/js/domain-renderer.js');

describe('#3420 domain-renderer — pure builders', () => {
  test('exports the builders + the 17-facet config', () => {
    expect(typeof R.renderFacet).toBe('function');
    expect(typeof R.tableFor).toBe('function');
    expect(typeof R.partOfHtml).toBe('function');
    expect(typeof R.childTreeHtml).toBe('function');
    expect(typeof R.resolveV2).toBe('function');
    expect(Array.isArray(R.FACETS)).toBe(true);
    expect(R.FACETS.length).toBe(17);
    R.FACETS.forEach((f: any) => {
      expect(typeof f.key).toBe('string');
      expect(typeof f.title).toBe('string');
      expect(typeof f.fetch).toBe('function');
    });
  });

  test('statCard renders the #3415 stat molecule', () => {
    const h = R.statCard('<span class="role role--wren">Wren</span>', 'Owner');
    expect(h).toContain('class="stat"');
    expect(h).toContain('class="stat-value"');
    expect(h).toContain('class="stat-label"');
    expect(h).toContain('Owner');
    expect(h).toContain('role--wren');
  });

  test('tableFor builds a #3415 .table with code-wrapped paths', () => {
    const h = R.tableFor([{ path: 'src/x.ts', type: 'unit' }], ['path', 'type']);
    expect(h).toContain('<table class="table">');
    expect(h).toContain('<code>src/x.ts</code>');
    expect(h).toContain('unit');
    expect(h).toContain('<th>Path</th>');
  });

  test('renderFacet table-facet shows count + source badge + .card + .table', () => {
    const code = R.FACETS.find((f: any) => f.key === 'code');
    const h = R.renderFacet(code, { data: { files: [{ path: 'a.ts', type: 'unit' }] } }, {});
    expect(h).toContain('class="card"');
    expect(h).toContain('Code (1)');
    expect(h).toContain('derived');
    expect(h).toContain('<table class="table">');
    expect(h).toContain('a.ts');
  });

  test('renderFacet renders an HONEST empty state, not a hidden one', () => {
    const code = R.FACETS.find((f: any) => f.key === 'code');
    const h = R.renderFacet(code, { data: { files: [] } }, {});
    expect(h).toContain('Code (0)');
    expect(h).toContain('class="muted"');
    expect(h.toLowerCase()).toContain('no code');
  });

  test('renderFacet gaps-facet uses the gap callout', () => {
    const gaps = R.FACETS.find((f: any) => f.key === 'gaps');
    const h = R.renderFacet(gaps, { data: { gaps: [{ type: 'gap', description: 'absent coverage' }] } }, {});
    expect(h).toContain('callout--gap');
    expect(h).toContain('GAP:');
    expect(h).toContain('absent coverage');
  });

  test('renderFacet dependencies-facet shows up + down with nav links', () => {
    const deps = R.FACETS.find((f: any) => f.key === 'dependencies');
    const h = R.renderFacet(deps, { data: { direct: { consumes: [{ id: 'cards-service', label: 'Cards' }], consumedBy: [] }, shared: [] } }, {});
    expect(h).toContain('depends on');
    expect(h).toContain('consumed by');
    expect(h).toContain('?id=cards-service');
  });

  test('partOfHtml renders the upward parent chain as nav chips (AC2)', () => {
    const h = R.partOfHtml(['build-product', 'athena']);
    expect(h).toContain('Part of (upward)');
    expect(h).toContain('class="badge"');
    expect(h).toContain('?id=build-product');
    expect(h).toContain('athena');
  });

  test('partOfHtml is honestly empty when there is no parent', () => {
    expect(R.partOfHtml([])).toBe('');
    expect(R.partOfHtml(undefined)).toBe('');
  });

  test('childTreeHtml renders a recursive nested tree d1>d2>d3 (3351)', () => {
    const tree = { name: 'd1', children: [{ name: 'd2', children: [{ name: 'd3', children: [] }] }] };
    const h = R.childTreeHtml(tree);
    expect(h).toContain('?id=d1-domain');
    expect(h).toContain('?id=d2-domain');
    expect(h).toContain('?id=d3-domain');
    // the recursion shows as nesting: d2 inside d1, d3 inside d2 -> at least 2 nested lists
    expect((h.match(/<ul/g) || []).length).toBeGreaterThanOrEqual(2);
  });

  test('childTreeHtml leaf renders no nested list', () => {
    const h = R.childTreeHtml({ name: 'leaf', children: [] });
    expect(h).toContain('?id=leaf-domain');
    expect(h).not.toContain('<ul');
  });

  test('resolveV2 maps v1 ids onto v2 homes, null for v1-only (3373)', () => {
    const V2 = ['cards', 'code', 'version-control'];
    expect(R.resolveV2('cards', V2)).toBe('cards');
    expect(R.resolveV2('code-domain', V2)).toBe('code');
    expect(R.resolveV2('cards-service', V2)).toBe('cards');
    expect(R.resolveV2('gates-service', V2)).toBeNull();
    expect(R.resolveV2('version-control-domain', V2)).toBe('version-control');
  });

  test('builders emit system.css vocabulary, never hardcoded colors', () => {
    const code = R.FACETS.find((f: any) => f.key === 'code');
    const h =
      R.renderFacet(code, { data: { files: [{ path: 'a.ts', type: 'unit' }] } }, {}) +
      R.tableFor([{ path: 'a.ts', type: 'unit' }], ['path', 'type']) +
      R.statCard('x', 'Owner') +
      R.partOfHtml(['athena']) +
      R.childTreeHtml({ name: 'heralds', children: [] });
    expect(h).toMatch(/class="(card|table|badge|stat|muted)/);
    expect(h).not.toMatch(/#[0-9a-fA-F]{6}/);
  });
});
