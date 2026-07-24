// @test-type: unit — signal is fixture-data: both resolvers run as pure functions
// of an env we construct (no live services, no real binary needed)
/**
 * #2478 — Bash CLI ↔ resolveShimPath() parity.
 *
 * The TS resolver (#2474) and the bash scripts resolved the shim independently
 * with nothing proving they agree — a relocation fixed in one language would
 * silently strand the other. This pins them: the shared bash resolver
 * (platform/scripts/lib/resolve-shim.sh) and resolveShimPath() must produce
 * IDENTICAL paths for the two env-driven scenarios, and both must produce an
 * absolute path in the no-env fallback (the fallbacks are host-relative by
 * nature — TS anchors on __dirname, bash on command -v — so scenario 3 pins
 * the contract, not the string).
 */
import { execFileSync } from 'child_process';
import * as path from 'path';
import { resolveShimPath } from '../src/shim-path';

const LIB = path.resolve(__dirname, '..', '..', 'scripts', 'lib', 'resolve-shim.sh');

function bashResolve(env: Record<string, string>): string {
  return execFileSync('bash', ['-c', `source "${LIB}" && resolve_shim_path`], {
    encoding: 'utf8',
    env: { PATH: process.env.PATH ?? '', ...env },
  }).trim();
}

describe('#2478 bash ↔ TS shim-path parity', () => {
  test('scenario 1 — CHORUS_SHIM_BIN override: identical', () => {
    const env = { CHORUS_SHIM_BIN: '/custom/override/shim' };
    expect(bashResolve(env)).toBe(resolveShimPath(env));
  });

  test('scenario 2 — CHORUS_ROOT-derived: identical', () => {
    const env = { CHORUS_ROOT: '/opt/chorus' };
    expect(bashResolve(env)).toBe(resolveShimPath(env));
  });

  test('scenario 2 beats scenario 3, scenario 1 beats both — same order both sides', () => {
    const env = { CHORUS_SHIM_BIN: '/a/shim', CHORUS_ROOT: '/b' };
    expect(bashResolve(env)).toBe('/a/shim');
    expect(resolveShimPath(env)).toBe('/a/shim');
  });

  test('scenario 3 — no env: both yield a non-empty absolute path (host-relative fallbacks)', () => {
    const ts = resolveShimPath({});
    const sh = bashResolve({});
    expect(path.isAbsolute(ts)).toBe(true);
    expect(sh.length).toBeGreaterThan(0);
    expect(path.isAbsolute(sh)).toBe(true);
  });
});
