// @test-type: unit
/**
 * #4006 — the retirement gate for the shim's `nudge` subcommand.
 *
 * WHAT WAS HERE BEFORE. `tests/nudge.integration.test.ts` — 20 cases asserting
 * `chorus-hook-shim nudge <role> "<msg>"` prints `DRY-RUN: would inject to
 * <role>`. That path was retired in #2804: the bash `nudge` became a fail-loud
 * stub and agents send nudges through the `chorus_nudge_message` MCP tool.
 * Nobody noticed the suite had stopped testing anything real, because the file
 * carried an unused-variable compile error (TS6133) and jest reported the whole
 * file as ONE failure — "suite failed to run" — for however long that took.
 * Fixing the compile error surfaced 18 failures underneath, all of them
 * describing a mechanism that no longer exists.
 *
 * WHY A GATE AND NOT JUST A DELETE. Deleting the suite silently would leave the
 * next reader to rediscover the retirement from scratch; a retirement without a
 * gate is structural amnesia. This file is the memory: it records what was
 * retired, when, and by which card, and it asserts the ONE property that must
 * hold about the retired path from this side of the seam.
 *
 * WHAT THIS DOES NOT ASSERT. The shim's own behaviour on an unknown subcommand
 * — it currently exits 0 silently, which Silas rooted on 2026-08-25 to a
 * fail-open hook proxy (shim.rs:357/378) and is fixing on #4004 with its own
 * negative proof. That gate belongs with that code, not here; a gate written
 * from across a seam grades a binary this package does not build.
 */
import * as fs from 'fs';
import * as path from 'path';

describe('#4006 the shim nudge path is retired', () => {
  test('no test in this package drives the retired shim nudge subcommand', () => {
    // The concrete regression: a suite that re-appears asserting DRY-RUN output
    // from `chorus-hook-shim nudge` is testing a mechanism retired in #2804.
    const dir = path.join(__dirname);
    const offenders: string[] = [];
    for (const f of fs.readdirSync(dir)) {
      if (!f.endsWith('.test.ts') || f === path.basename(__filename)) continue;
      const src = fs.readFileSync(path.join(dir, f), 'utf-8');
      if (/chorus-hook-shim['"`\s].*\bnudge\b|DRY-RUN: would inject/.test(src)) offenders.push(f);
    }
    expect(offenders).toEqual([]);
  });

  // NEGATIVE PROOF — the check above must RED on the thing it forbids. The
  // string tested is the exact shape the deleted suite used, so a check that
  // cannot see it would be a gate over an empty set.
  test('the check REDS on a file that drives the retired path', () => {
    const retiredShape = `const NUDGE_BINARY = '/x/chorus-hook-shim';\n`
      + `expect(stdout).toContain('DRY-RUN: would inject to silas');`;
    expect(/chorus-hook-shim['"`\s].*\bnudge\b|DRY-RUN: would inject/.test(retiredShape)).toBe(true);

    // And must NOT fire on ordinary nudge code that uses the live MCP path.
    const liveShape = `await chorus_nudge_message({ to: 'silas', message: 'hello' });`;
    expect(/chorus-hook-shim['"`\s].*\bnudge\b|DRY-RUN: would inject/.test(liveShape)).toBe(false);
  });
});
