/**
 * #2751 SMOKE — invoke chorus_pull_card via the real MCP server with real
 * execFileAsync (no mocks) against the real cards CLI / chorus-werk / git.
 * Proves the typed refusal taxonomy fires correctly against real data
 * shapes (the bug found in this session was `cards view --json` returns
 * `description`, not `desc` — a unit test with mocks could never catch it).
 */
import { buildMcpServer } from '../src/mcp/server';

type Handler = (req: unknown, ctx: unknown) => Promise<unknown>;

describe('#2751 smoke — chorus_pull_card against real chorus-api', () => {
  test('card-not-found: buildMcpServer rejects nonexistent card 99999', async () => {
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const server = buildMcpServer(() => 'kade', {
      emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
    } as never);
    const handler = (server as unknown as { _requestHandlers: Map<string, Handler> })._requestHandlers.get('tools/call')!;
    let caught: Error | null = null;
    try {
      await handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 99999 } } }, {});
    } catch (err) {
      caught = err as Error;
    }
    console.log('SMOKE-1 caught:', caught?.message);
    console.log('SMOKE-1 refusal:', JSON.stringify(events.find((e) => e.event === 'chorus_pull_card.refused')));
    expect(caught).not.toBeNull();
    expect(caught!.message).toMatch(/card-not-found|wrong-status/);
  }, 30_000);

  test('wrong-status: buildMcpServer rejects WIP card 2751', async () => {
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const server = buildMcpServer(() => 'kade', {
      emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
    } as never);
    const handler = (server as unknown as { _requestHandlers: Map<string, Handler> })._requestHandlers.get('tools/call')!;
    let caught: Error | null = null;
    try {
      await handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 2751 } } }, {});
    } catch (err) {
      caught = err as Error;
    }
    console.log('SMOKE-2 caught:', caught?.message);
    console.log('SMOKE-2 refusal:', JSON.stringify(events.find((e) => e.event === 'chorus_pull_card.refused')));
    expect(caught).not.toBeNull();
    expect(caught!.message).toMatch(/wrong-status|werk-dirty/);
  }, 30_000);

  test('werk-dirty or ac-missing: buildMcpServer rejects pull of card 1320 against dirty werk', async () => {
    const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
    const server = buildMcpServer(() => 'kade', {
      emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
    } as never);
    const handler = (server as unknown as { _requestHandlers: Map<string, Handler> })._requestHandlers.get('tools/call')!;
    let caught: Error | null = null;
    let success: unknown = null;
    try {
      success = await handler({ method: 'tools/call', params: { name: 'chorus_pull_card', arguments: { role: 'kade', card_id: 1320 } } }, {});
    } catch (err) {
      caught = err as Error;
    }
    console.log('SMOKE-3 caught:', caught?.message ?? '(no throw)');
    console.log('SMOKE-3 refusal:', JSON.stringify(events.find((e) => e.event === 'chorus_pull_card.refused')));
    console.log('SMOKE-3 success:', success ? JSON.stringify(success) : '(none)');
    expect(caught).not.toBeNull();
  }, 30_000);
});
