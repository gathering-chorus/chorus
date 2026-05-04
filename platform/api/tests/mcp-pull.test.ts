/**
 * #2688 — MCP chorus_pull (read+rebase) contract tests.
 *
 * Per DEC-1674 (TDD): tests describe Jeff's experience through the MCP
 * surface, RED first, then handler.
 *
 *   - Agent calls chorus_pull(role, branch?, remote?) and gets back fetched
 *     status — service runs `git pull --rebase` via the existing v2.5
 *     substrate (git-queue.sh do_pull) under the lock.
 *   - Refusal taxonomy (expanded from AC2 narrow per #2689 lesson):
 *     rebase-conflict | flock-timeout | dirty-tree | pull-fail.
 *   - Each refusal throws + emits chorus_pull.refused with reason.
 *   - Success emits chorus_pull.fetched + chorus_pull.rebase.attempted.
 *   - Abort path emits chorus_pull.rebase.aborted with pre-rebase ref.
 *   - No agent-visible flags/envs/bypasses on the input schema.
 */
import { buildMcpServer } from '../src/mcp/server';

type BoardCard = { id: number; owner: string; title: string };

describe('#2688 chorus_pull MCP tool — contract', () => {
  describe('AC1 + AC6 — registration + input schema', () => {
    test('exposes chorus_pull in tools/list', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access for unit test
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const names = result.tools.map((t: { name: string }) => t.name);
      expect(names).toContain('chorus_pull');
    });

    test('input schema accepts only role/branch?/remote? — strict, no smuggling', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const tool = result.tools.find((t: { name: string }) => t.name === 'chorus_pull');
      expect(tool).toBeDefined();
      expect(tool.inputSchema.required).toEqual(['role']);
      const propKeys = Object.keys(tool.inputSchema.properties).sort();
      expect(propKeys).toEqual(['branch', 'remote', 'role']);
      expect(tool.inputSchema.additionalProperties).toBe(false);
      expect(tool.inputSchema.properties.role.enum.sort()).toEqual(['kade', 'silas', 'wren']);
    });
  });

  describe('AC3 + AC8 — git-queue delegation, success path, spine events', () => {
    const oneCard: BoardCard[] = [{ id: 2688, owner: 'Kade', title: 'chorus_pull MCP' }];

    test('success path: spawns git-queue pull, emits chorus_pull.fetched + rebase.attempted', async () => {
      const calls: Array<{ args: string[]; env?: Record<string, string | undefined> }> = [];
      const exec = jest.fn(async (_file: string, args: string[], opts: { env?: Record<string, string | undefined> }) => {
        calls.push({ args, env: opts.env });
        if (args[0] === 'pull') return { stdout: 'Already up to date.\n', stderr: '' };
        throw new Error(`unexpected: ${args.join(' ')}`);
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');

      await handler(
        { method: 'tools/call', params: { name: 'chorus_pull', arguments: { role: 'kade' } } },
        {},
      );
      expect(calls).toHaveLength(1);
      expect(calls[0].args[0]).toBe('pull');
      // Same --force-branch escape hatch as commit/push (mirrors #2689 lesson).
      expect(calls[0].args).toContain('--force-branch');
      expect(calls[0].env?.DEPLOY_ROLE).toBe('kade');
      const eventNames = events.map((e) => e.event);
      expect(eventNames).toContain('chorus_pull.fetched');
      expect(eventNames).toContain('chorus_pull.rebase.attempted');
    });

    test('AC2 — rebase-conflict label on real merge conflict during pull-rebase', async () => {
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'pull') {
          const err = new Error('pull failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr =
            'CONFLICT (content): Merge conflict in platform/api/src/foo.ts\nerror: could not apply abc1234... wip';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull', arguments: { role: 'kade' } } }, {}),
      ).rejects.toThrow(/rebase-conflict/);
      expect(events.find((e) => e.event === 'chorus_pull.refused')?.fields.reason).toBe('rebase-conflict');
    });

    test('AC2 — flock-timeout label when git-queue lock held by peer', async () => {
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'pull') {
          const err = new Error('pull failed');
          (err as unknown as { code: number; stderr: string }).code = 75;
          (err as unknown as { code: number; stderr: string }).stderr =
            'git-queue: timeout after 30s — another commit is holding the lock';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull', arguments: { role: 'kade' } } }, {}),
      ).rejects.toThrow(/flock-timeout/);
      expect(events.find((e) => e.event === 'chorus_pull.refused')?.fields.reason).toBe('flock-timeout');
    });

    test('AC2 — dirty-tree label when uncommitted changes block pull-rebase', async () => {
      // The Mode-A common case: peer's checkout left uncommitted edits, pull
      // --rebase refuses. Without this label, pull-fail collapses two distinct
      // recovery paths (resolve conflict vs commit/stash dirty edits).
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'pull') {
          const err = new Error('pull failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr =
            'error: cannot pull with rebase: You have unstaged changes.\nerror: Please commit or stash them.';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull', arguments: { role: 'kade' } } }, {}),
      ).rejects.toThrow(/dirty-tree/);
      expect(events.find((e) => e.event === 'chorus_pull.refused')?.fields.reason).toBe('dirty-tree');
    });

    test('AC2 — pull-fail label as fallback for unmatched stderr (network, auth, etc.)', async () => {
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'pull') {
          const err = new Error('pull failed');
          (err as unknown as { code: number; stderr: string }).code = 128;
          (err as unknown as { code: number; stderr: string }).stderr =
            'fatal: unable to access https://github.com/...: Could not resolve host';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull', arguments: { role: 'kade' } } }, {}),
      ).rejects.toThrow(/pull-fail/);
      expect(events.find((e) => e.event === 'chorus_pull.refused')?.fields.reason).toBe('pull-fail');
    });

    test('AC4 + AC8 — rebase-conflict path emits chorus_pull.rebase.aborted', async () => {
      // git-queue.sh do_pull aborts the rebase cleanly on conflict (preserves
      // pre-rebase HEAD), so the user can retry without manual cleanup. The
      // abort emits chorus_pull.rebase.aborted at the MCP layer.
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'pull') {
          const err = new Error('pull failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr =
            'CONFLICT (content): Merge conflict in foo.ts\nrebase aborted, returned to pre-rebase state';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_pull', arguments: { role: 'kade' } } }, {}),
      ).rejects.toThrow(/rebase-conflict/);
      const eventNames = events.map((e) => e.event);
      expect(eventNames).toContain('chorus_pull.rebase.aborted');
    });
  });
});
