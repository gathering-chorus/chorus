// @test-type: integration — hits service/remote/sibling, skip-if-absent in CI
// @domain: services — placed by chorus-crawl's domain rules (#4538)
/**
 * #2474 — resolveShimPath unit tests.
 *
 * The MCP server (and any other TS caller that needs to spawn
 * chorus-hook-shim) goes through resolveShimPath so the binary
 * location is one source of truth, env-overridable, with a sensible
 * fallback that works on Bedroom + CI + post-relocation.
 */
import { resolveShimPath } from '../src/shim-path';
import * as path from 'path';

describe('#2474 resolveShimPath', () => {
  test('uses CHORUS_SHIM_BIN env var when set', () => {
    expect(resolveShimPath({ CHORUS_SHIM_BIN: '/custom/path/to/shim' })).toBe('/custom/path/to/shim');
  });

  test('falls back to CHORUS_ROOT-derived path', () => {
    expect(resolveShimPath({ CHORUS_ROOT: '/opt/chorus', USER: 'chorus-test' })).toBe(
      '/opt/chorus/target/chorus-test/release/chorus-hook-shim',
    );
  });

  test('CHORUS_SHIM_BIN takes precedence over CHORUS_ROOT', () => {
    expect(resolveShimPath({ CHORUS_SHIM_BIN: '/a', CHORUS_ROOT: '/b' })).toBe('/a');
  });

  test('uses __dirname-relative absolute path when neither env is set', () => {
    const result = resolveShimPath({ USER: 'chorus-test' });
    expect(path.isAbsolute(result)).toBe(true);
    // #4555: the checkout root's workspace folder for the account, never a crate's
    expect(result).toBe(path.resolve(__dirname, '..', '..', '..', 'target/chorus-test/release/chorus-hook-shim'));
  });
});
