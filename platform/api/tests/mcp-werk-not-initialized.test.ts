/**
 * #2760 — chorus_pull_card and chorus_unpull_card refuse cleanly when the
 * role's werk isn't initialized.
 *
 * Wren hit this 2026-05-06: flag flipped last session but `chorus-werk init
 * wren` never ran, so /CascadeProjects/chorus-werk/wren/ doesn't exist. The
 * MCP refused with `werk-dirty — git status failed` (mis-classified ENOENT
 * as dirty), and the recursive try/catch around refuse() doubled the error
 * message ("chorus_pull_card refused: ... — chorus_pull_card refused: ...").
 *
 * This card adds:
 * 1. `werk-not-initialized` typed refusal in BOTH chorus_pull_card and
 *    chorus_unpull_card when the werk path is missing.
 * 2. Refusal message single-pass (no "refused:" repeated twice).
 */
import { buildMcpServer } from '../src/mcp/server';

type Handler = (req: unknown, ctx: unknown) => Promise<unknown>;
type Handlers = { _requestHandlers: Map<string, Handler> };

const cardNext = {
  index: 1,
  status: 'Next',
  owner: 'Kade',
  title: 'fixture',
  description: '## Experience\nx\n\n## AC\n- [ ] x\n',
};

const cardWip = { ...cardNext, status: 'WIP' };

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

describe('#2760 — werk-not-initialized refusal in chorus_pull_card', () => {
  test('chorus_pull_card refuses werk-not-initialized when werk path missing', async () => {
    const exec = execEnoent(JSON.stringify(cardNext));
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const server = buildMcpServer(() => 'kade', {
      emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
      execFileAsync: exec as never,
      gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      resolveWorkingTree: ((_role: string) => '/fake/missing/werk-2760') as never,
    } as never);
    const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call')!;
    let caught: Error | null = null;
    try {
      await handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 1 } } }, {});
    } catch (err) {
      caught = err as Error;
    }
    expect(caught).not.toBeNull();
    expect(caught!.message).toMatch(/werk-not-initialized/);
    const refusedCount = (caught!.message.match(/chorus_pull_card refused:/g) || []).length;
    expect(refusedCount).toBe(1);
    const refusal = events.find((e) => e.event === 'chorus_pull_card.refused');
    expect(refusal?.fields.reason).toBe('werk-not-initialized');
    expect(String(refusal?.fields.detail)).toContain('chorus-werk init kade');
  });
});

describe('#2760 — werk-not-initialized refusal in chorus_unpull_card', () => {
  test('chorus_unpull_card refuses werk-not-initialized when werk path missing', async () => {
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
    expect(String(refusal?.fields.detail)).toContain('chorus-werk init kade');
  });
});
