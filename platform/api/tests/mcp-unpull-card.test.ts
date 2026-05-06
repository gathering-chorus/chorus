/**
 * #2759 — MCP chorus_unpull_card atomic teardown.
 *
 * /pull's natural inverse. When a role pulls a card and changes their mind,
 * today's pattern is `cards move <id> Next` which leaves stale branches +
 * role-state still building + no spine signal. This MCP closes the loop:
 * card → Next, werk → detached at origin/main, local + remote branch
 * deleted, role-state idle, card.unpulled spine event.
 */
import { buildMcpServer } from '../src/mcp/server';

type Handler = (req: unknown, ctx: unknown) => Promise<unknown>;
type ListHandler = (req: unknown, ctx: unknown) => Promise<{
  tools: Array<{
    name: string;
    inputSchema: { required: string[]; additionalProperties: boolean; properties: Record<string, { enum?: string[] }> };
  }>;
}>;

const wipCard = {
  index: 2759,
  status: 'WIP',
  owner: 'Kade',
  title: 'chorus_unpull_card',
  description: '## Experience\nClean teardown.\n\n## AC\n- [ ] thing\n',
  domains: 'chunk:ops, domain:chorus, type:new',
};

function buildHappyExec(opts: { dirty?: string; cardOverride?: Record<string, unknown> } = {}) {
  const calls: Array<{ file: string; args: string[]; cwd?: string }> = [];
  const fn = jest.fn(async (file: string, args: string[], options: { cwd?: string } = {}) => {
    calls.push({ file, args, cwd: options.cwd });
    if (file === 'git' && args[0] === 'status' && args[1] === '--porcelain') {
      return { stdout: opts.dirty ?? '', stderr: '' };
    }
    if (file === 'git' && args[0] === 'rev-parse' && args[1] === '--abbrev-ref') {
      return { stdout: 'kade/2759\n', stderr: '' };
    }
    if (file.endsWith('cards') && args[0] === 'view') {
      const card = { ...wipCard, ...(opts.cardOverride ?? {}) };
      return { stdout: JSON.stringify(card), stderr: '' };
    }
    if (file.endsWith('cards') && args[0] === 'move') {
      return { stdout: `Moved #${args[1]} to ${args[2]}\n`, stderr: '' };
    }
    if (file.endsWith('chorus-werk') && args[0] === 'close') {
      return { stdout: 'chorus-werk: closed kade/2759 (werk detached, local branch deleted, remote cleanup attempted)\n', stderr: '' };
    }
    if (file.endsWith('role-state')) {
      return { stdout: 'role.state.changed\n', stderr: '' };
    }
    if (file.endsWith('chorus-log')) {
      return { stdout: '', stderr: '' };
    }
    throw new Error(`unexpected: ${file} ${args.join(' ')}`);
  });
  return { fn, calls };
}

describe('#2759 — chorus_unpull_card MCP atomic teardown', () => {
  describe('AC1 — registration', () => {
    test('exposes chorus_unpull_card in tools/list with role + card_id schema', async () => {
      const server = buildMcpServer(() => 'kade');
      const handler = (server as unknown as { _requestHandlers: Map<string, ListHandler> })._requestHandlers.get('tools/list')!;
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const names = result.tools.map((t) => t.name);
      expect(names).toContain('chorus_unpull_card');
      const tool = result.tools.find((t) => t.name === 'chorus_unpull_card');
      expect(tool).toBeDefined();
      expect(tool!.inputSchema.required.sort()).toEqual(['card_id', 'role']);
      expect(tool!.inputSchema.additionalProperties).toBe(false);
      expect(tool!.inputSchema.properties.role.enum!.sort()).toEqual(['kade', 'silas', 'wren']);
    });
  });

  describe('AC2 — happy path tears down', () => {
    test('moves card WIP→Next, runs chorus-werk close, sets role-state idle, emits card.unpulled', async () => {
      const { fn: exec, calls } = buildHappyExec();
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as { _requestHandlers: Map<string, Handler> })._requestHandlers.get('tools/call')!;
      const result = await handler(
        { method: 'tools/call', params: { name: 'chorus_unpull_card', arguments: { role: 'kade', card_id: 2759 } } },
        {},
      ) as { content: Array<{ text: string }> };

      const fileNames = calls.map((c) => `${c.file.split('/').pop()}:${c.args[0]}`);
      expect(fileNames).toContain('cards:view');
      expect(fileNames).toContain('cards:move');
      expect(fileNames).toContain('chorus-werk:close');
      expect(fileNames).toContain('role-state:kade');
      const moveCall = calls.find((c) => c.file.endsWith('cards') && c.args[0] === 'move');
      expect(moveCall?.args).toContain('Next');

      expect(events.find((e) => e.event === 'card.unpulled')).toBeDefined();
      expect(events.find((e) => e.event === 'card.unpulled')?.fields.card_id).toBe(2759);

      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.role).toBe('kade');
      expect(parsed.card_id).toBe(2759);
      expect(parsed.prior_branch).toBe('kade/2759');
      expect(parsed.branch_closed).toBe(true);
    });
  });

  describe('AC3 — refusal taxonomy', () => {
    test('refuses card-not-found when cards view fails', async () => {
      const exec = jest.fn(async (file: string, args: string[]) => {
        if (file.endsWith('cards') && args[0] === 'view') {
          const err = new Error('not found') as Error & { code?: number; stderr?: string };
          err.stderr = 'Card #9999 does not exist';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
      } as never);
      const handler = (server as unknown as { _requestHandlers: Map<string, Handler> })._requestHandlers.get('tools/call')!;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_unpull_card', arguments: { role: 'kade', card_id: 9999 } } }, {}),
      ).rejects.toThrow(/card-not-found/);
      expect(events.find((e) => e.event === 'chorus_unpull_card.refused')?.fields.reason).toBe('card-not-found');
    });

    test('refuses wrong-status when card is not WIP', async () => {
      const { fn: exec } = buildHappyExec({ cardOverride: { status: 'Next' } });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as { _requestHandlers: Map<string, Handler> })._requestHandlers.get('tools/call')!;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_unpull_card', arguments: { role: 'kade', card_id: 2759 } } }, {}),
      ).rejects.toThrow(/wrong-status/);
      expect(events.find((e) => e.event === 'chorus_unpull_card.refused')?.fields.reason).toBe('wrong-status');
    });

    test('refuses wrong-owner when card belongs to another role', async () => {
      const { fn: exec } = buildHappyExec({ cardOverride: { owner: 'Silas' } });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as { _requestHandlers: Map<string, Handler> })._requestHandlers.get('tools/call')!;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_unpull_card', arguments: { role: 'kade', card_id: 2759 } } }, {}),
      ).rejects.toThrow(/wrong-owner/);
      expect(events.find((e) => e.event === 'chorus_unpull_card.refused')?.fields.reason).toBe('wrong-owner');
    });

    test('refuses werk-dirty when werk has uncommitted changes', async () => {
      const { fn: exec } = buildHappyExec({ dirty: ' M platform/api/src/server.ts\n' });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as { _requestHandlers: Map<string, Handler> })._requestHandlers.get('tools/call')!;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_unpull_card', arguments: { role: 'kade', card_id: 2759 } } }, {}),
      ).rejects.toThrow(/werk-dirty/);
      const refusal = events.find((e) => e.event === 'chorus_unpull_card.refused');
      expect(refusal?.fields.reason).toBe('werk-dirty');
      expect(String(refusal?.fields.detail)).toContain('server.ts');
    });
  });
});
