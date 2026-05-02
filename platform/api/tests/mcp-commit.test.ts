/**
 * #2661 — MCP chorus_commit_status (read-only) contract tests.
 *
 * Per DEC-1674 (TDD): tests describe Jeff's experience through the MCP
 * surface, written FIRST and RED until the handler lands.
 *
 *   - Agent calls chorus_commit_status(role) and gets the role's active
 *     card and branch state — derived from the BOARD via `cards list`,
 *     NOT from role-state. (#2467/#2629: card lives on the board.)
 *   - Refusal taxonomy: no-wip-card | multi-wip | board-unreachable.
 *   - Each refusal throws a typed error AND emits a typed spine event.
 *   - No agent-visible flags/envs/bypasses on the input schema.
 *
 * Write surface (chorus_commit) is filed as a separate card after this
 * ships; tests for it land with that card.
 */
import { buildMcpServer } from '../src/mcp/server';

// Board reader DI seam: returns the cards matching the query
// (status=WIP, owner=role). Future write handler reuses this seam.
type BoardCard = { id: number; title: string; type?: string };
type BoardReaderResult =
  | { ok: true; cards: BoardCard[] }
  | { ok: false; reason: 'board-unreachable'; detail?: string };

function makeBoardReader(result: BoardReaderResult) {
  return jest.fn(async (_role: string) => result);
}

function makeEmitter() {
  const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
  const emit = (event: string, fields: Record<string, unknown>) => {
    events.push({ event, fields });
  };
  return { emit, events };
}

describe('#2661 chorus_commit_status MCP tool — contract', () => {
  describe('AC1 — registration', () => {
    test('exposes chorus_commit_status in tools/list', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access for unit test
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const names = result.tools.map((t: { name: string }) => t.name);
      expect(names).toContain('chorus_commit_status');
    });

    test('input schema accepts only role: enum(kade|wren|silas) — no other fields', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const tool = result.tools.find((t: { name: string }) => t.name === 'chorus_commit_status');
      expect(tool).toBeDefined();
      expect(tool.inputSchema.required).toEqual(['role']);
      const propKeys = Object.keys(tool.inputSchema.properties);
      expect(propKeys).toEqual(['role']);
      expect(tool.inputSchema.properties.role.enum.sort()).toEqual(['kade', 'silas', 'wren']);
      // AC6 — no card_id / branch / force / bypass on the wire.
      expect(propKeys).not.toContain('card_id');
      expect(propKeys).not.toContain('branch');
      expect(propKeys).not.toContain('force');
      expect(propKeys).not.toContain('bypass');
    });

    test('rejects missing role', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit_status', arguments: {} } }, {}),
      ).rejects.toThrow(/Invalid arguments/);
    });

    test('rejects unknown role', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler(
          { method: 'tools/call', params: { name: 'chorus_commit_status', arguments: { role: 'jeff' } } },
          {},
        ),
      ).rejects.toThrow(/Invalid arguments/);
    });
  });

  describe('AC2 — board query for active card', () => {
    test('returns active card and derived branch when exactly one WIP card', async () => {
      const boardReader = makeBoardReader({
        ok: true,
        cards: [{ id: 2661, title: 'v3-1a chorus_commit_status', type: 'new' }],
      });
      const { emit } = makeEmitter();
      const server = buildMcpServer(() => 'kade', {
        boardReader: boardReader as never,
        emitSpineEvent: emit as never,
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      const res = await handler(
        { method: 'tools/call', params: { name: 'chorus_commit_status', arguments: { role: 'kade' } } },
        {},
      );
      const text = res.content[0].text;
      expect(text).toContain('2661');
      expect(text).toContain('kade/2661');
      expect(boardReader).toHaveBeenCalledWith('kade');
    });
  });

  describe('AC3 — refusal taxonomy (typed reasons)', () => {
    test('refuses no-wip-card when zero cards match', async () => {
      const boardReader = makeBoardReader({ ok: true, cards: [] });
      const { emit, events } = makeEmitter();
      const server = buildMcpServer(() => 'kade', {
        boardReader: boardReader as never,
        emitSpineEvent: emit as never,
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler(
          { method: 'tools/call', params: { name: 'chorus_commit_status', arguments: { role: 'kade' } } },
          {},
        ),
      ).rejects.toThrow(/no-wip-card/);

      const refusal = events.find((e) => e.event === 'chorus_commit.status_queried');
      expect(refusal).toBeDefined();
      expect(refusal?.fields.reason).toBe('no-wip-card');
      expect(refusal?.fields.role).toBe('kade');
    });

    test('refuses multi-wip when multiple cards match', async () => {
      const boardReader = makeBoardReader({
        ok: true,
        cards: [
          { id: 2661, title: 'a', type: 'new' },
          { id: 2677, title: 'b', type: 'chore' },
        ],
      });
      const { emit, events } = makeEmitter();
      const server = buildMcpServer(() => 'kade', {
        boardReader: boardReader as never,
        emitSpineEvent: emit as never,
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler(
          { method: 'tools/call', params: { name: 'chorus_commit_status', arguments: { role: 'kade' } } },
          {},
        ),
      ).rejects.toThrow(/multi-wip/);

      const refusal = events.find((e) => e.event === 'chorus_commit.status_queried');
      expect(refusal?.fields.reason).toBe('multi-wip');
    });

    test('refuses board-unreachable when board reader fails', async () => {
      const boardReader = makeBoardReader({
        ok: false,
        reason: 'board-unreachable',
        detail: 'cards CLI exited 1',
      });
      const { emit, events } = makeEmitter();
      const server = buildMcpServer(() => 'kade', {
        boardReader: boardReader as never,
        emitSpineEvent: emit as never,
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler(
          { method: 'tools/call', params: { name: 'chorus_commit_status', arguments: { role: 'kade' } } },
          {},
        ),
      ).rejects.toThrow(/board-unreachable/);

      const refusal = events.find((e) => e.event === 'chorus_commit.status_queried');
      expect(refusal?.fields.reason).toBe('board-unreachable');
    });

    test('default board reader surfaces fetch AbortError as board-unreachable timeout', async () => {
      // Defensive contract: the production fetch carries a 5s AbortController
      // timeout. If the board hangs, the fetch aborts and the handler refuses
      // with board-unreachable + "timeout" detail rather than hanging the
      // caller. Observed 2026-05-02: 9 stuck jest processes from a hung
      // integration run before this defensive timeout was added.
      const abortFetch = jest.fn(async () => {
        const err = new Error('The operation was aborted');
        err.name = 'AbortError';
        throw err;
      });
      const { emit, events } = makeEmitter();
      const server = buildMcpServer(() => 'kade', {
        fetchImpl: abortFetch as never,
        emitSpineEvent: emit as never,
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler(
          { method: 'tools/call', params: { name: 'chorus_commit_status', arguments: { role: 'kade' } } },
          {},
        ),
      ).rejects.toThrow(/board-unreachable.*timeout/);
      const refusal = events.find((e) => e.event === 'chorus_commit.status_queried');
      expect(refusal?.fields.reason).toBe('board-unreachable');
      expect(String(refusal?.fields.detail)).toMatch(/timeout/);
    });
  });

  describe('AC4 — spine event always fires', () => {
    test('success path emits chorus_commit.status_queried with role + card_id + no reason', async () => {
      const boardReader = makeBoardReader({
        ok: true,
        cards: [{ id: 2661, title: 'x', type: 'new' }],
      });
      const { emit, events } = makeEmitter();
      const server = buildMcpServer(() => 'kade', {
        boardReader: boardReader as never,
        emitSpineEvent: emit as never,
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler(
        { method: 'tools/call', params: { name: 'chorus_commit_status', arguments: { role: 'kade' } } },
        {},
      );
      const ev = events.find((e) => e.event === 'chorus_commit.status_queried');
      expect(ev).toBeDefined();
      expect(ev?.fields.role).toBe('kade');
      expect(ev?.fields.card_id).toBe(2661);
      expect(ev?.fields.reason).toBeUndefined();
    });
  });
});
