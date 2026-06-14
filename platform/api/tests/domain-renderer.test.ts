/**
 * #3420 — page-level unit coverage for the GENERATED Athena domain page's renderer
 * (public/js/domain-renderer.js). The renderer is the bulk of the page's logic
 * (facet rendering, tables, empty states) — it previously had no unit coverage
 * (Jeff caught the gap). The renderer exports its PURE builders for node; we assert
 * they produce the right #3415 system.css structure from mock endpoint data, with
 * honest empty states and NO bespoke styling.
 */
// eslint-disable-next-line @typescript-eslint/no-var-requires
const R = require('../public/js/domain-renderer.js');

describe('#3420 domain-renderer — pure builders', () => {
  test('exports the builders + the 17-facet config', () => {
    expect(typeof R.renderFacet).toBe('function');
    expect(typeof R.tableFor).toBe('function');
    expect(typeof R.partOfHtml).toBe('function');
    expect(Array.isArray(R.FACETS)).toBe(true);
    expect(R.FACETS.length).toBe(17);
    // every facet declares a key + title + a fetch
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
    expect(h).toContain('derived');          // the source label
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
    expect(h).toContain('?id=cards-service');   // cross-entity nav link
  });

  test('partOfHtml renders the upward parent chain as nav chips (AC2)', () => {
    const h = R.partOfHtml(['build-product', 'athena']);
    expect(h).toContain('Part of (upward)');
    expect(h).toContain('class="badge"');
    expect(h).toContain('?id=build-product');   // navigable upward
    expect(h).toContain('athena');
  });

  test('partOfHtml is honestly empty when there is no parent', () => {
    // a top-level entity has no upward edge — render nothing, not an empty box
    expect(R.partOfHtml([])).toBe('');
    expect(R.partOfHtml(undefined)).toBe('');
  });

  test('builders emit system.css vocabulary, never hardcoded colors', () => {
    const code = R.FACETS.find((f: any) => f.key === 'code');
    const h =
      R.renderFacet(code, { data: { files: [{ path: 'a.ts', type: 'unit' }] } }, {}) +
      R.tableFor([{ path: 'a.ts', type: 'unit' }], ['path', 'type']) +
      R.statCard('x', 'Owner') +
      R.partOfHtml(['athena']);
    // uses the #3415 classes
    expect(h).toMatch(/class="(card|table|badge|stat|muted)/);
    // and emits no raw hex colors (those would be off-token / bespoke)
    expect(h).not.toMatch(/#[0-9a-fA-F]{6}/);
  });
});
