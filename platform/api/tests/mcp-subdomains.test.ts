/**
 * #2624 — MCP subdomains tool unit tests.
 *
 * Adds chorus_subdomains_list + chorus_subdomains_get so DEC-058's
 * "Query Athena via MCP for current subdomain owners" instruction
 * resolves to a real tool.
 *
 * Pattern mirrors mcp-principles.test.ts and mcp-decisions.test.ts.
 */
import { buildMcpServer } from '../src/mcp/server';

describe('#2624 buildMcpServer subdomains tools', () => {
  test('tools/list exposes chorus_subdomains_list + chorus_subdomains_get', async () => {
    const server = buildMcpServer(() => 'wren');
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/list');
    const result = await handler({ method: 'tools/list', params: {} }, {});
    const names = result.tools.map((t: any) => t.name).sort();
    expect(names).toContain('chorus_subdomains_list');
    expect(names).toContain('chorus_subdomains_get');
  });

  test('chorus_subdomains_list delegates fetch to /api/athena/subdomains and returns owner+step', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        data: [
          { id: 'commits-domain', label: 'Commits', owner: 'Kade', step: 'Building' },
          { id: 'code-domain', label: 'Code', owner: 'Kade', step: 'Building' },
        ],
      }),
    } as any);
    const server = buildMcpServer(() => 'wren', { fetchImpl: fetchMock });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    const result = await handler(
      { method: 'tools/call', params: { name: 'chorus_subdomains_list', arguments: {} } },
      {},
    );
    expect(fetchMock).toHaveBeenCalledTimes(1);
    expect(fetchMock.mock.calls[0][0]).toContain('/api/athena/subdomains');
    expect(result.content[0].text).toContain('Commits');
    expect(result.content[0].text).toContain('Kade');
  });

  test('chorus_subdomains_get returns a single subdomain by id', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({
        data: [
          { id: 'commits-domain', label: 'Commits', owner: 'Kade', step: 'Building' },
          { id: 'gates-service', label: 'Gates', owner: 'Silas', step: 'Building' },
        ],
      }),
    } as any);
    const server = buildMcpServer(() => 'wren', { fetchImpl: fetchMock });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    const result = await handler(
      {
        method: 'tools/call',
        params: { name: 'chorus_subdomains_get', arguments: { id: 'gates-service' } },
      },
      {},
    );
    expect(result.content[0].text).toContain('Gates');
    expect(result.content[0].text).toContain('Silas');
    expect(result.content[0].text).not.toContain('Commits');
  });

  test('chorus_subdomains_get throws on unknown id', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      json: async () => ({ data: [{ id: 'commits-domain', label: 'Commits', owner: 'Kade' }] }),
    } as any);
    const server = buildMcpServer(() => 'wren', { fetchImpl: fetchMock });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    await expect(
      handler(
        { method: 'tools/call', params: { name: 'chorus_subdomains_get', arguments: { id: 'nonexistent' } } },
        {},
      ),
    ).rejects.toThrow(/not found/i);
  });
});
