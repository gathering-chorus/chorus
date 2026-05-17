/**
 * #2969 — chorus_doc_catalog_add MCP tool unit tests.
 *
 * Covers tool registration, successful POST, and the three typed refusals
 * (invalid-input 400, file-not-found 404, already-registered 409) that
 * mirror registerDoc()'s HTTP status codes in handlers/doc-catalog.ts.
 */
import { buildMcpServer } from '../src/mcp/server';

describe('#2969 buildMcpServer chorus_doc_catalog_add', () => {
  test('tools/list exposes chorus_doc_catalog_add', async () => {
    const server = buildMcpServer(() => 'silas');
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/list');
    const result = await handler({ method: 'tools/list', params: {} }, {});
    const names = result.tools.map((t: any) => t.name);
    expect(names).toContain('chorus_doc_catalog_add');
  });

  test('chorus_doc_catalog_add POSTs filePath + href to /api/doc-catalog/add', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => ({
        registered: { filePath: '/abs/path/foo.html', href: '/gathering-docs/foo.html' },
      }),
    } as any);
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    const result = await handler(
      {
        method: 'tools/call',
        params: {
          name: 'chorus_doc_catalog_add',
          arguments: { filePath: '/abs/path/foo.html', href: '/gathering-docs/foo.html' },
        },
      },
      {},
    );
    expect(fetchMock).toHaveBeenCalledTimes(1);
    const [url, init] = fetchMock.mock.calls[0];
    expect(url).toContain('/api/doc-catalog/add');
    expect(init.method).toBe('POST');
    expect(JSON.parse(init.body)).toEqual({ filePath: '/abs/path/foo.html', href: '/gathering-docs/foo.html' });
    expect(result.content[0].text).toContain('/gathering-docs/foo.html');
  });

  test('chorus_doc_catalog_add forwards optional group field when provided', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => ({
        registered: { filePath: '/abs/path/foo.html', href: '/gathering-docs/foo.html', group: 'chorus' },
      }),
    } as any);
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    await handler(
      {
        method: 'tools/call',
        params: {
          name: 'chorus_doc_catalog_add',
          arguments: { filePath: '/abs/path/foo.html', href: '/gathering-docs/foo.html', group: 'chorus' },
        },
      },
      {},
    );
    const sentBody = JSON.parse(fetchMock.mock.calls[0][1].body);
    expect(sentBody.group).toBe('chorus');
  });

  test('404 from registerDoc surfaces as file-not-found refusal', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: false,
      status: 404,
      json: async () => ({ error: 'File not found: /abs/missing.html' }),
    } as any);
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    await expect(
      handler(
        {
          method: 'tools/call',
          params: {
            name: 'chorus_doc_catalog_add',
            arguments: { filePath: '/abs/missing.html', href: '/gathering-docs/missing.html' },
          },
        },
        {},
      ),
    ).rejects.toThrow(/file-not-found/);
  });

  test('409 from registerDoc surfaces as already-registered refusal', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: false,
      status: 409,
      json: async () => ({ error: 'Already registered: /gathering-docs/foo.html' }),
    } as any);
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    await expect(
      handler(
        {
          method: 'tools/call',
          params: {
            name: 'chorus_doc_catalog_add',
            arguments: { filePath: '/abs/path/foo.html', href: '/gathering-docs/foo.html' },
          },
        },
        {},
      ),
    ).rejects.toThrow(/already-registered/);
  });

  test('400 from registerDoc surfaces as invalid-input refusal', async () => {
    const fetchMock = jest.fn().mockResolvedValue({
      ok: false,
      status: 400,
      json: async () => ({ error: 'Only .html and .md files can be registered' }),
    } as any);
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    await expect(
      handler(
        {
          method: 'tools/call',
          params: {
            name: 'chorus_doc_catalog_add',
            arguments: { filePath: '/abs/path/foo.txt', href: '/gathering-docs/foo.txt' },
          },
        },
        {},
      ),
    ).rejects.toThrow(/invalid-input/);
  });

  test('zod schema rejects missing required fields before fetch fires', async () => {
    const fetchMock = jest.fn();
    const server = buildMcpServer(() => 'silas', { fetchImpl: fetchMock });
    // @ts-expect-error - private handler access for unit test
    const handler = (server as any)._requestHandlers.get('tools/call');
    await expect(
      handler(
        {
          method: 'tools/call',
          params: {
            name: 'chorus_doc_catalog_add',
            arguments: { href: '/gathering-docs/foo.html' },
          },
        },
        {},
      ),
    ).rejects.toThrow(/Invalid arguments/);
    expect(fetchMock).not.toHaveBeenCalled();
  });
});
