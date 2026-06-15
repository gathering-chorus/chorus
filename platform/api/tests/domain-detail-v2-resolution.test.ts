/**
 * #3373 — the domain page's v1-id → v2-name resolution.
 *
 * Jeff's experience under test: every existing bookmark/link keeps working.
 * Ids with a v2 home read the generated API; ids without one render from v1
 * exactly as before — no page breaks because the model migrated underneath.
 *
 * The page is plain browser JS (no modules), so the function under test is
 * extracted by evaluating the script's source in a vm sandbox.
 */

// #3373 moved into the GENERATED renderer (#3351 retired the hand-built domain-detail.js).
// The function lives in the shared renderer now; load it from its exports.
function loadResolve(): (id: string, names: string[]) => string | null {
  const R = require('../public/js/domain-renderer.js');
  return R.resolveV2;
}

const V2 = ['cards', 'code', 'tests', 'builds', 'version-control', 'alerts-monitors'];

describe('resolveV2DomainName (#3373)', () => {
  const resolve = loadResolve();

  it('exact v2 name resolves to itself', () => {
    expect(resolve('cards', V2)).toBe('cards');
  });

  it('v1 -domain suffix strips onto its v2 home', () => {
    expect(resolve('code-domain', V2)).toBe('code');
    expect(resolve('tests-domain', V2)).toBe('tests');
  });

  it('v1 -service suffix strips onto its v2 home', () => {
    expect(resolve('cards-service', V2)).toBe('cards');
  });

  it('v1-only ids stay v1 — null, never a wrong match', () => {
    expect(resolve('gates-service', V2)).toBeNull(); // 'gates' not in v2 here
    expect(resolve('athena-domain', V2)).toBeNull();
    expect(resolve('chorus-domain', V2)).toBeNull();
  });

  it('a name that merely contains a v2 name never false-matches', () => {
    expect(resolve('version-control-domain', V2)).toBe('version-control');
    expect(resolve('controls', V2)).toBeNull();
  });
});
