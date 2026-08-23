// @test-type: unit — injected fake verifier + role lookup; fixture log; no live world.
/**
 * #2725 AC1 — the /api/nudge/:role/pending handler: declared + ENFORCED,
 * role-lane scoped (Silas ruling 2026-08-23). Pure decision function tested
 * with injected fakes (#3528: no live verifier, no live graph, no ~/.chorus).
 *
 * Refusals name their state (memory: a gate that can't tell "not you" from
 * "not public" is the auth defect class): 401 authn-missing vs 403 not-your-lane.
 */
import { decideNudgePending } from '../src/nudge-pending-route';
import { mkdtempSync, writeFileSync } from 'fs';
import { tmpdir } from 'os';
import { join } from 'path';

const TS = '2026-08-23T09:25:47.916-04:00';
const emittedLine = (trace: string, to: string) =>
  JSON.stringify({
    timestamp: TS, event: 'nudge.emitted', role: 'chorus-mcp',
    payload: `from=testerA,to=${to},chars=2,trace=${trace},origin=mcp,content=hi`,
  });

function fixtureLog(lines: string[]): string {
  const dir = mkdtempSync(join(tmpdir(), 'spine-2725r-'));
  const p = join(dir, 'chorus.log');
  writeFileSync(p, lines.join('\n') + '\n');
  return p;
}

const okVerify = (webId: string) => async (_t: string) => ({ ok: true as const, webId, scope: ['nudge-read'] });
const badVerify = async (_t: string) => ({ ok: false as const, reason: 'bad-token' });
// holdsRole lookup fake — the roles-domain projection (webId → role slug)
const roleOf = (map: Record<string, string>) => async (webId: string) => map[webId] ?? null;

describe('#2725 pending route decision', () => {
  const log = fixtureLog([emittedLine('t1', 'testerB'), emittedLine('t2', 'testerC')]);
  const deps = (webId: string, map: Record<string, string>) => ({
    verify: okVerify(webId),
    roleForWebId: roleOf(map),
    logPath: log,
  });

  test('no bearer → 401 authn-missing', async () => {
    const d = await decideNudgePending({ role: 'testerB', authorization: '' },
      { verify: badVerify, roleForWebId: roleOf({}), logPath: log });
    expect(d.status).toBe(401);
    expect(d.body).toMatchObject({ error: 'authn-missing' });
  });

  test('own lane → 200 with own pending only', async () => {
    const d = await decideNudgePending(
      { role: 'testerB', authorization: 'Bearer x' },
      deps('https://id.test/b#me', { 'https://id.test/b#me': 'testerB' }),
    );
    expect(d.status).toBe(200);
    expect((d.body as any[]).map((n) => n.trace)).toEqual(['t1']);
  });

  test("another role's lane → 403 not-your-lane (names the state)", async () => {
    const d = await decideNudgePending(
      { role: 'testerC', authorization: 'Bearer x' },
      deps('https://id.test/b#me', { 'https://id.test/b#me': 'testerB' }),
    );
    expect(d.status).toBe(403);
    expect(d.body).toMatchObject({ error: 'not-your-lane' });
  });

  test('jeff reads all lanes', async () => {
    const d = await decideNudgePending(
      { role: 'all', authorization: 'Bearer x' },
      deps('https://id.test/jeff#me', { 'https://id.test/jeff#me': 'jeff' }),
    );
    expect(d.status).toBe(200);
    expect((d.body as any[]).map((n) => n.trace).sort()).toEqual(['t1', 't2']);
  });

  test('unknown webId → 403 no-role-held (not 401 — authn succeeded)', async () => {
    const d = await decideNudgePending(
      { role: 'testerB', authorization: 'Bearer x' },
      deps('https://id.test/x#me', {}),
    );
    expect(d.status).toBe(403);
    expect(d.body).toMatchObject({ error: 'no-role-held' });
  });
});
