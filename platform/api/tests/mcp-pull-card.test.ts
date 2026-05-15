/**
 * #2751 / #2913 — MCP chorus_pull_card atomic transaction (ephemeral worktree).
 *
 * /pull is one MCP call: validate the card, move it to WIP, create the card's
 * ephemeral worktree via `chorus-werk add`, declare role-state, emit
 * card.pulled. The skill collapses to invoking this and nothing else.
 *
 * #2913 changed the werk model. The old persistent-per-role werk had a
 * werk-preflight step (refuse werk-dirty / werk-wrong-branch / werk-not-
 * initialized) because a single stable chorus-werk/<role>/ dir carried state
 * across cards. The ephemeral model has no carry-over: each card gets a fresh
 * worktree, created here by `chorus-werk add`, which is idempotent and refuses
 * a clobber itself. The preflight is gone — pre-flighting a werk that does not
 * exist yet was checking the wrong thing.
 *
 * Per DEC-1674 (TDD): specs describe Jeff's experience — he says /pull <id>
 * once and the card lands in WIP with its own worktree on a fresh branch.
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

function buildHappyExec(opts: { cardOverride?: Record<string, unknown>; werkAddFails?: boolean } = {}) {
  const calls: Array<{ file: string; args: string[]; cwd?: string }> = [];
  const fn = jest.fn(async (file: string, args: string[], options: { cwd?: string } = {}) => {
    calls.push({ file, args, cwd: options.cwd });
    if (file.endsWith('cards') && args[0] === 'view') {
      const card = { ...baseCardJson, ...(opts.cardOverride ?? {}) };
      return { stdout: JSON.stringify(card), stderr: '' };
    }
    if (file.endsWith('cards') && args[0] === 'move') {
      return { stdout: 'Moved #2751 to WIP\n  Blast radius: 116 files, 1 domains\n', stderr: '' };
    }
    if (file.endsWith('chorus-werk') && args[0] === 'add') {
      if (opts.werkAddFails) {
        const err = new Error('add failed') as Error & { code?: number; stderr?: string };
        err.code = 4;
        err.stderr = 'chorus-werk: git worktree add -b kade/2751 failed';
        throw err;
      }
      return { stdout: 'chorus-werk: added kade-2751 at /fake/chorus-werk/kade-2751 (branch kade/2751)\n', stderr: '' };
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

describe('#2751 / #2913 — chorus_pull_card MCP atomic transaction', () => {
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
    test('validates → moves to WIP → chorus-werk add → declares state → emits card.pulled', async () => {
      const { fn: exec, calls } = buildHappyExec();
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        resolveWorkingTree: ((_role: string) => '/fake/canonical') as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      const result = await handler(
        { method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } },
        {},
      );

      const fileNames = calls.map((c) => `${c.file.split('/').pop()}:${c.args[0]}`);
      expect(fileNames).toContain('cards:view');
      expect(fileNames).toContain('cards:move');
      expect(fileNames).toContain('chorus-werk:add');
      expect(fileNames).toContain('role-state:kade');
      // No git status / rev-parse — the werk-preflight step is gone (#2913).
      expect(fileNames).not.toContain('git:status');

      expect(events.find((e) => e.event === 'card.pulled')).toBeDefined();
      expect(events.find((e) => e.event === 'card.pulled')?.fields.card_id).toBe(2751);

      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.role).toBe('kade');
      expect(parsed.card_id).toBe(2751);
      expect(parsed.branch).toBe('kade/2751');
    });

    test('#2931 — every chorus_pull_card.*.completed event carries duration_ms', async () => {
      const { fn: exec } = buildHappyExec();
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        resolveWorkingTree: ((_role: string) => '/fake/canonical') as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await handler(
        { method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } },
        {},
      );

      const completed = events.filter((e) => /^chorus_pull_card\.[a-z-]+\.completed$/.test(e.event));
      expect(completed.length).toBeGreaterThan(2);
      for (const e of completed) {
        expect(typeof e.fields.duration_ms).toBe('number');
        expect(e.fields.duration_ms as number).toBeGreaterThanOrEqual(0);
      }
      const stepNames = completed.map((e) => e.event.replace(/^chorus_pull_card\.|\.completed$/g, ''));
      for (const step of ['validate', 'werk-add']) {
        expect(stepNames).toContain(step);
      }
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
        resolveWorkingTree: ((_role: string) => '/fake/canonical') as never,
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
        resolveWorkingTree: ((_role: string) => '/fake/canonical') as never,
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
        resolveWorkingTree: ((_role: string) => '/fake/canonical') as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } }, {}),
      ).rejects.toThrow(/experience-missing/);
      expect(events.find((e) => e.event === 'chorus_pull_card.refused')?.fields.reason).toBe('experience-missing');
    });

    test('refuses branch-fail when chorus-werk add fails', async () => {
      const { fn: exec } = buildHappyExec({ werkAddFails: true });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        resolveWorkingTree: ((_role: string) => '/fake/canonical') as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } }, {}),
      ).rejects.toThrow(/branch-fail/);
      expect(events.find((e) => e.event === 'chorus_pull_card.refused')?.fields.reason).toBe('branch-fail');
    });
  });

  describe('AC4 — werk-aware', () => {
    test('resolves the chorus-werk + role-state script paths from resolveWorkingTree root', async () => {
      const { fn: exec, calls } = buildHappyExec();
      const server = buildMcpServer(() => 'kade', {
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        resolveWorkingTree: ((_role: string) => '/fake/canonical') as never,
      } as never);
      const handler = (server as unknown as Handlers)._requestHandlers.get('tools/call') as CallHandler;
      await handler(
        { method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } },
        {},
      );
      const werkCall = calls.find((c) => c.file.endsWith('chorus-werk'));
      const roleStateCall = calls.find((c) => c.file.endsWith('role-state'));
      expect(werkCall!.file).toBe('/fake/canonical/platform/scripts/chorus-werk');
      expect(roleStateCall!.file).toBe('/fake/canonical/platform/scripts/role-state');
    });
  });
});
