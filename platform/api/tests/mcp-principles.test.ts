/**
 * #2476 — MCP principles tool unit tests (initial 3 cases).
 *
 * Covers tool registration shape and basic delegation. End-to-end live
 * round-trips covered by platform/tests/mcp-principles.test.sh.
 */
import { buildMcpServer } from '../src/mcp/server';

describe('#2476 buildMcpServer principles tools', () => {
  test('tools/list exposes nudge + three principles tools', async () => {
    const server = buildMcpServer(() => 'silas');
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/list');
    const result = await handler({ method: 'tools/list', params: {} }, {});
    const names = result.tools.map((t: any) => t.name).sort();
    expect(names).toEqual([
      'chorus_decisions_get',
      'chorus_decisions_list',
      'chorus_nudge_message',
      'chorus_principles_create',
      'chorus_principles_get',
      'chorus_principles_list',
      'chorus_subdomains_get',
      'chorus_subdomains_list',
    ]);
  });

  test('chorus_principles_list delegates fetch to /api/loom/principles', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ data: { principles: [{ id: 'p1', label: 'P1' }] } }),
    } as any);
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    const result = await handler({ method: 'tools/call', params: { name: 'chorus_principles_list', arguments: {} } }, {});
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toContain('/api/loom/principles');
    expect(result.content[0].text).toContain('P1');
  });

  test('chorus_principles_create POSTs label to subdomain endpoint', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => ({ data: { id: 'principle-test' } }),
    } as any);
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    const result = await handler({ method: 'tools/call', params: { name: 'chorus_principles_create', arguments: { label: 'Test' } } }, {});
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const url = fetchMock.mock.calls[0][0];
    expect(url).toContain('/api/athena/subdomains/loom-principles/principles');
    expect(result.content[0].text).toMatch(/principle-test|created/i);
  });
});
