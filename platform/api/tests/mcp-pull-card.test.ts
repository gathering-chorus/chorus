/**
 * #2751 — MCP chorus_pull_card atomic transaction.
 *
 * /pull skill today is 7 hard gates the model executes by reading markdown
 * (validate, preflight, wip-check, domain-context, design-gate, TDD-readiness,
 * move + branch + role-state + spine). Same model-compliance reliability gap
 * /acp had before #2750. Same fix shape: substrate primitive runs the whole
 * transaction; skill collapses to one MCP call.
 *
 * Plus the lived-experience addition from 2026-05-06 morning: when flag-on
 * and werk has uncommitted carry-over from a prior session, a /pull onto
 * dirty werk smuggled Wren's in-progress file into a kade commit. The MCP
 * refuses dirty werk and wrong-branch werk with typed reasons, so the
 * substrate can't be tricked into the same accident.
 *
 * Per DEC-1674 (TDD): RED first. Specs describe Jeff's experience: he says
 * /pull <id> once and the card lands in WIP with a clean branch in the role's
 * werk; if the werk has carry-over drift, he gets a typed refusal naming the
 * dirty files, not a half-baked pull.
 */
import { buildMcpServer } from '../src/mcp/server';

type ListHandler = (req: unknown, ctx: unknown) => Promise<{
  tools: Array<{
    name: string;
    inputSchema: {
      required: string[];
      additionalProperties: boolean;
      properties: Record<string, { enum?: string[] }>;
    };
  }>;
}>;
type CallHandler = (req: unknown, ctx: unknown) => Promise<{ content: Array<{ text: string }> }>;
type Handlers = { _requestHandlers: Map<string, ListHandler | CallHandler> };

const baseCardJson = {
  id: 2751,
  owner: 'Kade',
  status: 'Next',
  title: 'chorus_pull_card atomic',
  desc: '## Experience\nClean pull lands the card in WIP.\n\n## AC\n- [ ] thing\n',
  domains: 'chunk:ops, domain:chorus, type:fix',
};

function buildHappyExec(opts: { dirty?: string; branch?: string; cardOverride?: Record<string, unknown> } = {}) {
  const calls: Array<{ file: string; args: string[]; cwd?: string }> = [];
  const fn = jest.fn(async (file: string, args: string[], options: { cwd?: string } = {}) => {
    calls.push({ file, args, cwd: options.cwd });
    if (file === 'git' && args[0] === 'status' && args[1] === '--porcelain') {
      return { stdout: opts.dirty ?? '', stderr: '' };
    }
    if (file === 'git' && args[0] === 'rev-parse' && args[1] === '--abbrev-ref') {
      return { stdout: (opts.branch ?? 'main') + '\n', stderr: '' };
    }
    if (file.endsWith('cards') && args[0] === 'view') {
      const card = { ...baseCardJson, ...(opts.cardOverride ?? {}) };
      return { stdout: JSON.stringify(card), stderr: '' };
    }
    if (file.endsWith('cards') && args[0] === 'move') {
      return { stdout: 'Moved #2751 to WIP\n  Blast radius: 116 files, 1 domains\n', stderr: '' };
    }
    if (file.endsWith('chorus-werk') && args[0] === 'repoint') {
      return { stdout: 'chorus-werk: kade now on kade/2751\n', stderr: '' };
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

describe('#2751 — chorus_pull_card MCP atomic transaction', () => {
  describe('AC1 — registration', () => {
    test('exposes chorus_pull_card in tools/list', async () => {
      const server = buildMcpServer(() => 'kade');
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/list') as ListHandler;
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const names = result.tools.map((t) => t.name);
      expect(names).toContain('chorus_pull_card');
    });

    test('input schema requires role + card_id, strict, no bypass fields', async () => {
      const server = buildMcpServer(() => 'kade');
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/list') as ListHandler;
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const tool = result.tools.find((t) => t.name === 'chorus_pull_card');
      expect(tool).toBeDefined();
      expect(tool!.inputSchema.required.sort()).toEqual(['card_id', 'role']);
      expect(tool!.inputSchema.additionalProperties).toBe(false);
      expect(tool!.inputSchema.properties.role.enum!.sort()).toEqual(['kade', 'silas', 'wren']);
    });
  });

  describe('AC2 — happy path runs full transaction', () => {
    test('validates → moves to WIP → repoints werk → declares state → emits card.pulled', async () => {
      const { fn: exec, calls } = buildHappyExec();
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      const result = await handler(
        { method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } },
        {},
      );

      const fileNames = calls.map((c) => `${c.file.split('/').pop()}:${c.args[0]}`);
      expect(fileNames).toContain('cards:view');
      expect(fileNames).toContain('cards:move');
      expect(fileNames).toContain('chorus-werk:repoint');
      expect(fileNames).toContain('role-state:kade');

      expect(events.find((e) => e.event === 'card.pulled')).toBeDefined();
      expect(events.find((e) => e.event === 'card.pulled')?.fields.card_id).toBe(2751);

      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.role).toBe('kade');
      expect(parsed.card_id).toBe(2751);
      expect(parsed.branch).toBe('kade/2751');
    });
  });

  describe('AC3 — refusal taxonomy', () => {
    test('refuses card-not-found when cards view fails', async () => {
      const exec = jest.fn(async (file: string, args: string[]) => {
        if (file.endsWith('cards') && args[0] === 'view') {
          const err = new Error('not found') as Error & { code?: number; stderr?: string };
          err.code = 1;
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
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 9999 } } }, {}),
      ).rejects.toThrow(/card-not-found/);
      expect(events.find((e) => e.event === 'chorus_pull_card.refused')?.fields.reason).toBe('card-not-found');
    });

    test('refuses wrong-status when card is already Done', async () => {
      const { fn: exec } = buildHappyExec({ cardOverride: { status: 'Done' } });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } }, {}),
      ).rejects.toThrow(/wrong-status/);
      expect(events.find((e) => e.event === 'chorus_pull_card.refused')?.fields.reason).toBe('wrong-status');
    });

    test('refuses ac-missing when description has no AC checklist', async () => {
      const { fn: exec } = buildHappyExec({ cardOverride: { desc: '## Experience\nNo AC here.\n' } });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } }, {}),
      ).rejects.toThrow(/ac-missing/);
      expect(events.find((e) => e.event === 'chorus_pull_card.refused')?.fields.reason).toBe('ac-missing');
    });

    test('refuses experience-missing when description has no Experience section', async () => {
      const { fn: exec } = buildHappyExec({ cardOverride: { desc: '## AC\n- [ ] thing\n' } });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } }, {}),
      ).rejects.toThrow(/experience-missing/);
      expect(events.find((e) => e.event === 'chorus_pull_card.refused')?.fields.reason).toBe('experience-missing');
    });

    test('refuses werk-dirty when werk has uncommitted changes', async () => {
      const { fn: exec } = buildHappyExec({ dirty: ' M roles/silas/next-session.md\n M platform/api/src/server.ts\n' });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } }, {}),
      ).rejects.toThrow(/werk-dirty/);
      const refusal = events.find((e) => e.event === 'chorus_pull_card.refused');
      expect(refusal?.fields.reason).toBe('werk-dirty');
      expect(String(refusal?.fields.detail)).toContain('next-session.md');
    });

    test('refuses werk-wrong-branch when werk is on a card branch instead of main', async () => {
      const { fn: exec } = buildHappyExec({ branch: 'kade/2752' });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } }, {}),
      ).rejects.toThrow(/werk-wrong-branch/);
      const refusal = events.find((e) => e.event === 'chorus_pull_card.refused');
      expect(refusal?.fields.reason).toBe('werk-wrong-branch');
      expect(String(refusal?.fields.detail)).toContain('kade/2752');
    });

    test('detached HEAD (post-acp state) is acceptable, not a refusal', async () => {
      const { fn: exec } = buildHappyExec({ branch: 'HEAD' });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await handler(
        { method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } },
        {},
      );
      expect(events.find((e) => e.event === 'chorus_pull_card.refused')).toBeUndefined();
      expect(events.find((e) => e.event === 'card.pulled')).toBeDefined();
    });
  });

  describe('AC4 — werk-aware', () => {
    test('routes git status + chorus-werk + role-state cwd via resolveWorkingTree', async () => {
      const cwds: string[] = [];
      const exec = jest.fn(async (file: string, args: string[], options: { cwd?: string } = {}) => {
        if (options.cwd && (file === 'git' || file.endsWith('chorus-werk'))) cwds.push(options.cwd);
        if (file === 'git' && args[0] === 'status' && args[1] === '--porcelain') return { stdout: '', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'main\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'view') return { stdout: JSON.stringify(baseCardJson), stderr: '' };
        if (file.endsWith('cards') && args[0] === 'move') return { stdout: 'Moved\n', stderr: '' };
        if (file.endsWith('chorus-werk')) return { stdout: 'kade now on kade/2751\n', stderr: '' };
        if (file.endsWith('role-state')) return { stdout: 'changed\n', stderr: '' };
        if (file.endsWith('chorus-log')) return { stdout: '', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        resolveWorkingTree: ((_role: string) => '/fake/chorus-werk/kade') as never,
        fsExists: ((_p: string) => true) as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await handler(
        { method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } },
        {},
      );
      expect(cwds.length).toBeGreaterThan(0);
      cwds.forEach((cwd) => expect(cwd).toBe('/fake/chorus-werk/kade'));
    });
  });
});
