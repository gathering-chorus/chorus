// @test-type: unit — imports the module with child_process.exec recorded; no live service, no network
// @domain: code · @card: 4405 · owner: kade
/**
 * #4405 — importing server.ts must not start the board refresh. Every jest
 * file imports it; on 2026-10-12 12:40 each import launched `cards list`, and
 * in a werk with no built cards CLI each one compiled TypeScript: 63 at once,
 * load 144, the Clearing unreachable.
 *
 * The refresh calls exec synchronously inside the import when it starts, so
 * recording exec needs no wait (a 1.5 s wait timed this test out on a cold run).
 */
const calls: string[] = [];
jest.mock('child_process', () => {
  const actual = jest.requireActual('child_process');
  return {
    ...actual,
    exec: (cmd: string, ...rest: unknown[]) => {
      calls.push(cmd);
      return (actual.exec as (...a: unknown[]) => unknown)(cmd, ...rest);
    },
  };
});

describe('#4405 board refresh starts only when server.ts is the server', () => {
  it('NEGATIVE PROOF — importing the module runs no `cards list`', async () => {
    process.env.CHORUS_CARDS_BIN = '/usr/bin/true';
    await import('../src/server');
    expect(calls.filter((c) => /\blist\b/.test(c))).toEqual([]);
  });
});
