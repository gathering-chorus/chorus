/**
 * #2682 — MCP chorus_commit (write) contract tests.
 *
 * Per DEC-1674 (TDD): tests describe Jeff's experience through the MCP
 * surface, RED first, then handler.
 *
 *   - Agent calls chorus_commit(role, paths, message) and gets back a SHA
 *     and the derived branch — service runs the full commit+push under the
 *     existing v2.5 substrate (git-queue.sh) and reports the outcome.
 *   - Refusal taxonomy expands #2661's: no-wip-card | multi-wip |
 *     board-unreachable (inherited via boardReader reuse) PLUS
 *     branch-mismatch | hook-fail | push-conflict (new, classified from
 *     git-queue exit + stderr).
 *   - Each refusal throws + emits chorus_commit.refused with reason.
 *   - Success emits chorus_commit.invoked with role, card_id, paths_count, sha.
 *   - No agent-visible flags/envs/bypasses on the input schema.
 */
import { buildMcpServer } from '../src/mcp/server';

type BoardCard = { id: number; owner: string; title: string };

describe('#2682 chorus_commit (write) MCP tool — contract', () => {
  describe('AC1 — registration', () => {
    test('exposes chorus_commit in tools/list', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access for unit test
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const names = result.tools.map((t: { name: string }) => t.name);
      expect(names).toContain('chorus_commit');
    });

    test('input schema accepts only role/paths/message — strict, no smuggling', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const tool = result.tools.find((t: { name: string }) => t.name === 'chorus_commit');
      expect(tool).toBeDefined();
      expect(tool.inputSchema.required.sort()).toEqual(['message', 'paths', 'role']);
      const propKeys = Object.keys(tool.inputSchema.properties).sort();
      expect(propKeys).toEqual(['message', 'paths', 'role']);
      expect(tool.inputSchema.additionalProperties).toBe(false);
      expect(tool.inputSchema.properties.role.enum.sort()).toEqual(['kade', 'silas', 'wren']);
    });

    test('rejects empty paths array', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: [], message: 'm' } } }, {}),
      ).rejects.toThrow(/Invalid arguments/);
    });

    test('rejects empty message', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: '' } } }, {}),
      ).rejects.toThrow(/Invalid arguments/);
    });
  });

  describe('AC2 — board-derived refusals (inherited from #2661 boardReader)', () => {
    test('refuses no-wip-card when role has no WIP', async () => {
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: [] })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {}),
      ).rejects.toThrow(/no-wip-card/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('no-wip-card');
    });

    test('refuses multi-wip when role has 2+ WIP cards', async () => {
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({
          ok: true,
          cards: [
            { id: 2682, owner: 'Kade', title: 'a' },
            { id: 2683, owner: 'Kade', title: 'b' },
          ],
        })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {}),
      ).rejects.toThrow(/multi-wip/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('multi-wip');
    });

    test('refuses board-unreachable when boardReader fails', async () => {
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: false, reason: 'board-unreachable', detail: 'cli failure' })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {}),
      ).rejects.toThrow(/board-unreachable/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('board-unreachable');
    });
  });

  describe('AC3+AC4 — git-queue delegation + new refusals from exit codes', () => {
    const oneCard: BoardCard[] = [{ id: 2682, owner: 'Kade', title: 'v3-1b chorus_commit' }];

    test('success path: spawns git-queue commit then push, returns SHA, emits invoked', async () => {
      const calls: Array<{ args: string[]; env?: Record<string, string | undefined> }> = [];
      const exec = jest.fn(async (_file: string, args: string[], opts: { env?: Record<string, string | undefined> }) => {
        calls.push({ args, env: opts.env });
        if (args[0] === 'commit') return { stdout: '[kade/2682-x abcd1234] kade: msg\n', stderr: '' };
        if (args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        throw new Error(`unexpected args: ${args.join(' ')}`);
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

      const res = await handler(
        { method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['a.ts', 'b.ts'], message: 'kade: #2682 wip' } } },
        {},
      );
      const text = res.content[0].text;
      const parsed = JSON.parse(text) as { sha: string; branch: string; card_id: number };
      expect(parsed.sha).toBe('abcd1234');
      expect(parsed.card_id).toBe(2682);
      expect(parsed.branch).toBe('kade/2682');
      expect(calls).toHaveLength(2);
      expect(calls[0].args[0]).toBe('commit');
      expect(calls[0].env?.DEPLOY_ROLE).toBe('kade');
      expect(calls[1].args[0]).toBe('push');
      const invoked = events.find((e) => e.event === 'chorus_commit.invoked');
      expect(invoked?.fields.role).toBe('kade');
      expect(invoked?.fields.card_id).toBe(2682);
      expect(invoked?.fields.paths_count).toBe(2);
      expect(invoked?.fields.sha).toBe('abcd1234');
    });

    test('refuses branch-mismatch when git-queue rejects HEAD branch', async () => {
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'commit') {
          const err = new Error('branch-check refused');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr = 'git-queue: branch-check refused — HEAD is wren/foo, expected kade/*';
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
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {}),
      ).rejects.toThrow(/branch-mismatch/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('branch-mismatch');
    });

    test('refuses hook-fail when git-queue commit blocked by pre-commit', async () => {
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'commit') {
          const err = new Error('hook failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr = 'pre-commit: lint-ratchet failed — see baseline';
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
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {}),
      ).rejects.toThrow(/hook-fail/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('hook-fail');
    });

    test('refuses push-conflict when commit succeeds but push rebase fails', async () => {
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'commit') return { stdout: '[kade/2682-x abcd1234] kade: msg\n', stderr: '' };
        if (args[0] === 'push') {
          const err = new Error('push failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr = 'rebase: conflict on platform/api/src/foo.ts';
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
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {}),
      ).rejects.toThrow(/push-conflict/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('push-conflict');
    });
  });
});
