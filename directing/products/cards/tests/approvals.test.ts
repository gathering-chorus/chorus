// @test-type: unit — postApprovalRow with the door's fetch stubbed and a stand-in identity; no live door.
// @domain: decisions
// @card: #4505
/**
 * #4505 — an agent's card ask becomes ONE Approval row, written as the asking role.
 * Each refusal has a negative proof: a disallowed flag, a replay-unsafe argv, a retry.
 */
import { buildApprovalArgv, checkApprovalArgv, postApprovalRow, headsUp, approvalName } from '../src/approvals';

type Call = { url: string; method?: string; headers?: Record<string, string>; body?: string };
function door(opts: { existing?: string[]; postStatus?: number } = {}) {
  const calls: Call[] = [];
  const f = async (url: string, init?: { method?: string; headers?: Record<string, string>; body?: string }) => {
    calls.push({ url, ...init });
    if (!init?.method) return { ok: true, status: 200, json: async () => ({ data: (opts.existing || []).map((name) => ({ name })) }) };
    const st = opts.postStatus ?? 201;
    return { ok: st < 300, status: st, text: async () => 'refused' };
  };
  return { calls, f };
}
const identity = (role: string) => `stand-in-identity-for-${role}`;

describe('#4505 approvals: the ask is one row', () => {
  test('R1-L1: the ask posts one Approval row with the cards add argv, as the asking role', async () => {
    const d = door();
    const argv = buildApprovalArgv('Retire the X', { owner: 'kade', priority: 'P3', description: '## Experience\nx' });
    const r = await postApprovalRow({ role: 'wren', title: 'Retire the X', argv, fetchImpl: d.f, token: identity, door: 'http://door' });
    expect(r.outcome).toBe('asked');
    const post = d.calls.find((c) => c.method === 'POST')!;
    expect(post.url).toBe('http://door/decisions/approvals');
    expect(post.headers!.Authorization).toContain(identity('wren'));
    const body = JSON.parse(post.body!);
    expect(body).not.toHaveProperty('ownedBy'); // the door stamps the owner from the verified writer, never the body
    expect(body.approvalKind).toBe('card');
    expect(JSON.parse(body.approvalAction)).toEqual(['add', 'Retire the X', '--owner', 'kade', '--priority', 'P3', '--desc', '## Experience\nx']);
  });

  test('R1-HEADS-UP: the first ask links the list; a second while others wait sends no second link', async () => {
    const first = await postApprovalRow({ role: 'wren', title: 'A', argv: ['add', 'A'], fetchImpl: door().f, token: identity, door: 'http://door' });
    expect(headsUp(first, 'wren', 'A')).toContain('/go/approvals');
    const second = await postApprovalRow({ role: 'kade', title: 'B', argv: ['add', 'B'], fetchImpl: door({ existing: ['approval-wren-a'] }).f, token: identity, door: 'http://door' });
    expect(headsUp(second, 'kade', 'B')).not.toContain('/go/approvals');
    expect(headsUp(second, 'kade', 'B')).toContain('2 waiting');
  });

  test('NEG: a retry of the same role+title is not a second ask (the door refuses the name)', async () => {
    const r = await postApprovalRow({ role: 'wren', title: 'A', argv: ['add', 'A'], fetchImpl: door({ postStatus: 409 }).f, token: identity, door: 'http://door' });
    expect(r.outcome).toBe('already-waiting');
    expect(headsUp(r, 'wren', 'A')).toContain('Not asked again');
    expect(approvalName('wren', 'Retire the X!')).toBe('approval-wren-retire-the-x');
  });

  test('NEG: an argv that is not a cards add argv is refused at post', async () => {
    for (const bad of [['rm', '-rf', '/'], ['add', 'T', '--desc-file', '/etc/hosts'], ['add', 'T', '--desc', '-'], ['add'], ['add', '--owner', 'x']]) {
      expect(() => checkApprovalArgv(bad)).toThrow(/not a cards add argv/);
      await expect(postApprovalRow({ role: 'wren', title: 'T', argv: bad, fetchImpl: door().f, token: identity, door: 'http://door' })).rejects.toThrow(/not a cards add argv/);
    }
    expect(() => buildApprovalArgv('T', { parent: '4505' })).toThrow(/cannot carry --parent/);
  });

  test('NEG: a door refusal is an error, not a silent ask', async () => {
    await expect(postApprovalRow({ role: 'wren', title: 'T', argv: ['add', 'T'], fetchImpl: door({ postStatus: 403 }).f, token: identity, door: 'http://door' }))
      .rejects.toThrow(/door refused the ask \(403\)/);
  });
});
