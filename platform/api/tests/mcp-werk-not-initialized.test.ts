/**
 * #2760 / #2913 — chorus_unpull_card refuses cleanly when the card's werk
 * isn't present.
 *
 * History: Wren hit a mis-classification 2026-05-06 — the werk path was
 * missing and the MCP refused with `werk-dirty — git status failed`
 * (ENOENT mis-read as dirty), with a doubled "refused:" message from a
 * recursive try/catch. #2760 added a typed `werk-not-initialized` refusal
 * and a single-pass message.
 *
 * #2913 (ephemeral worktrees): chorus_pull_card no longer has a werk
 * pre-flight — there is no werk to flight-check at pull time; `chorus-werk
 * add` creates it. So the pull-side of this test is gone. chorus_unpull_card
 * still pre-flights — at unpull time the card's worktree DOES exist (it was
 * created at pull), so a missing path is a real, typed refusal.
 */
import { buildMcpServer } from '../src/mcp/server';

type Handler = (req: unknown, ctx: unknown) => Promise<unknown>;
type Handlers = { _requestHandlers: Map<string, Handler> };

const cardWip = {
  index: 1,
  status: 'WIP',
  owner: 'Kade',
  title: 'fixture',
  description: '## Experience\nx\n\n## AC\n- [ ] x\n',
};

function execEnoent(viewStdout: string) {
  return jest.fn(async (file: string, args: string[]) => {
    if (file.endsWith('cards') && args[0] === 'view') {
      return { stdout: viewStdout, stderr: '' };
    }
    if (file === 'git' && args[0] === 'status') {
      const err = new Error('spawn ENOENT') as Error & { code?: string; stderr?: string };
      err.code = 'ENOENT';
      err.stderr = `chdir /fake/missing/werk-2760: ENOENT`;
      throw err;
    }
    return { stdout: '', stderr: '' };
  });
}

describe('#2760 / #2913 — werk-not-initialized refusal in chorus_unpull_card', () => {
  test('chorus_unpull_card refuses werk-not-initialized when the card werk path is missing', async () => {
    const exec = execEnoent(JSON.stringify(cardWip));
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const server = buildMcpServer(() => 'kade', {
      emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
      execFileAsync: exec as never,
      resolveWorkingTree: ((_role: string) => '/fake/missing/werk-2760') as never,
    } as never);
    const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call')!;
    let caught: Error | null = null;
    try {
      await handler({ method: 'tools/call', params: { name: 'chorus_unpull_card', arguments: { role: 'kade', card_id: 1 } } }, {});
    } catch (err) {
      caught = err as Error;
    }
    expect(caught).not.toBeNull();
    expect(caught!.message).toMatch(/werk-not-initialized/);
    const refusedCount = (caught!.message.match(/chorus_unpull_card refused:/g) || []).length;
    expect(refusedCount).toBe(1);
    const refusal = events.find((e) => e.event === 'chorus_unpull_card.refused');
    expect(refusal?.fields.reason).toBe('werk-not-initialized');
    expect(String(refusal?.fields.detail)).toContain('werk path does not exist');
  });
});
