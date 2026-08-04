// @test-type: unit — signal is fixture-data: both resolvers run as pure functions
// of an env we construct (no live services; the "deployed binary" is a tempdir fixture)
/**
 * #2478 — Bash CLI ↔ resolveShimPath() parity, reordered by #3750.
 *
 * The TS resolver (#2474) and the bash scripts resolved the shim independently
 * with nothing proving they agree — a relocation fixed in one language would
 * silently strand the other. This pins them: the shared bash resolver
 * (platform/scripts/lib/resolve-shim.sh) and resolveShimPath() must produce
 * IDENTICAL paths for the env-driven scenarios, and both must produce an
 * absolute path in the no-env fallback.
 *
 * #3750 contract change: the DEPLOYED binary ($HOME/.chorus/bin) now beats the
 * CHORUS_ROOT build path on both sides — probes and suites must measure what
 * runs, not what was last compiled. HOME is threaded through env on both sides
 * so the deployed leg is test-constructible (and the negative proof — no
 * deployed binary → build-path fallback — is scenario 3).
 */
import { execFileSync } from 'child_process';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';
import { resolveShimPath } from '../src/shim-path';

const LIB = path.resolve(__dirname, '..', '..', 'scripts', 'lib', 'resolve-shim.sh');
// Exclude ~/.chorus/bin from PATH so `command -v` can't leak the host's real
// deployed shim into the hermetic scenarios (test brings its own world, #3528).
const CLEAN_PATH = '/usr/bin:/bin';

function bashResolve(env: Record<string, string>): string {
  return execFileSync('bash', ['-c', `source "${LIB}" && resolve_shim_path`], {
    encoding: 'utf8',
    env: { PATH: CLEAN_PATH, ...env },
  }).trim();
}

describe('#2478/#3750 bash ↔ TS shim-path parity', () => {
  let fakeHome: string;
  let deployedShim: string;

  beforeAll(() => {
    fakeHome = fs.mkdtempSync(path.join(os.tmpdir(), 'shim-parity-'));
    deployedShim = path.join(fakeHome, '.chorus', 'bin', 'chorus-hook-shim');
    fs.mkdirSync(path.dirname(deployedShim), { recursive: true });
    fs.writeFileSync(deployedShim, '#!/bin/sh\n', { mode: 0o755 });
  });

  afterAll(() => {
    fs.rmSync(fakeHome, { recursive: true, force: true });
  });

  test('scenario 1 — CHORUS_SHIM_BIN override beats everything: identical', () => {
    const env = { CHORUS_SHIM_BIN: '/custom/override/shim', HOME: fakeHome, CHORUS_ROOT: '/b' };
    expect(bashResolve(env)).toBe('/custom/override/shim');
    expect(resolveShimPath(env)).toBe('/custom/override/shim');
  });

  test('scenario 2 — deployed binary beats CHORUS_ROOT build path (#3750): identical', () => {
    const env = { HOME: fakeHome, CHORUS_ROOT: '/opt/chorus' };
    expect(bashResolve(env)).toBe(deployedShim);
    expect(resolveShimPath(env)).toBe(deployedShim);
  });

  test('scenario 3 — NEGATIVE PROOF: no deployed binary → CHORUS_ROOT fallback, identical', () => {
    // The state the old order could not distinguish: nothing deployed. Both
    // sides must fall back to the build path — and ONLY then.
    const env = { HOME: '/nonexistent-home-3750', CHORUS_ROOT: '/opt/chorus' };
    const expected = path.join('/opt/chorus', 'platform/services/chorus-hooks/target/release/chorus-hook-shim');
    expect(bashResolve(env)).toBe(expected);
    expect(resolveShimPath(env)).toBe(expected);
  });

  test('scenario 4 — no env at all: both yield a non-empty absolute path (host-relative fallbacks)', () => {
    const ts = resolveShimPath({});
    const sh = bashResolve({ HOME: '/nonexistent-home-3750' });
    expect(path.isAbsolute(ts)).toBe(true);
    expect(sh.length).toBeGreaterThan(0);
    expect(path.isAbsolute(sh)).toBe(true);
  });
});
