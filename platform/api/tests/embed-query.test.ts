import { createEmbedder } from '../src/embed-query';

function okResponse(vec: number[]): Response {
  return {
    ok: true,
    status: 200,
    json: async () => ({ embedding: vec }),
  } as unknown as Response;
}

function badResponse(status: number): Response {
  return {
    ok: false,
    status,
    json: async () => ({}),
  } as unknown as Response;
}

describe('createEmbedder', () => {
  it('returns the embedding vector from a successful Ollama call', async () => {
    const fetchFn = jest.fn(async () => okResponse([0.1, 0.2, 0.3]));
    const embed = createEmbedder({ ollamaUrl: 'http://x', model: 'm', fetchFn });
    const vec = await embed('hello');
    expect(vec).toEqual([0.1, 0.2, 0.3]);
    expect(fetchFn).toHaveBeenCalledTimes(1);
  });

  it('POSTs to the expected Ollama path with model + prompt', async () => {
    const fetchFn = jest.fn(async () => okResponse([0.5]));
    const embed = createEmbedder({ ollamaUrl: 'http://host:9', model: 'mod', fetchFn });
    await embed('query');
    const [url, init] = fetchFn.mock.calls[0];
    expect(url).toBe('http://host:9/api/embeddings');
    expect(init!.method).toBe('POST');
    const body = JSON.parse(init!.body as string);
    expect(body).toEqual({ model: 'mod', prompt: 'query' });
  });

  it('retries on failure and succeeds on a later attempt', async () => {
    let call = 0;
    const fetchFn = jest.fn(async () => {
      call++;
      if (call < 3) throw new Error('network');
      return okResponse([1, 2]);
    });
    const embed = createEmbedder({
      ollamaUrl: 'http://x', model: 'm', fetchFn,
      maxRetries: 3, backoffMs: [0, 0, 0],
    });
    const vec = await embed('retry-me');
    expect(vec).toEqual([1, 2]);
    expect(fetchFn).toHaveBeenCalledTimes(3);
  });

  it('throws when all retries are exhausted', async () => {
    const fetchFn = jest.fn(async () => { throw new Error('perma-fail'); });
    const embed = createEmbedder({
      ollamaUrl: 'http://x', model: 'm', fetchFn,
      maxRetries: 2, backoffMs: [0, 0],
    });
    await expect(embed('no-luck')).rejects.toThrow();
    expect(fetchFn).toHaveBeenCalledTimes(2);
  });

  it('throws when Ollama returns a non-ok response after retries', async () => {
    const fetchFn = jest.fn(async () => badResponse(500));
    const embed = createEmbedder({
      ollamaUrl: 'http://x', model: 'm', fetchFn,
      maxRetries: 2, backoffMs: [0, 0],
    });
    await expect(embed('bad')).rejects.toThrow(/500/);
  });

  it('caches repeat calls with the same normalized text', async () => {
    const fetchFn = jest.fn(async () => okResponse([7]));
    const embed = createEmbedder({ ollamaUrl: 'http://x', model: 'm', fetchFn });
    await embed('Same');
    await embed('same');    // lowercased key — cache hit
    await embed('  same  '); // trimmed key — cache hit
    expect(fetchFn).toHaveBeenCalledTimes(1);
  });

  it('honors cache TTL — entry expires and a second fetch fires', async () => {
    const now = { t: 1000 };
    const fetchFn = jest.fn(async () => okResponse([9]));
    const embed = createEmbedder({
      ollamaUrl: 'http://x', model: 'm', fetchFn,
      cacheTtlMs: 100,
      now: () => now.t,
    });
    await embed('x');
    now.t += 50;
    await embed('x'); // within TTL — cache hit
    now.t += 100;
    await embed('x'); // past TTL — cache miss
    expect(fetchFn).toHaveBeenCalledTimes(2);
  });

  it('evicts oldest entry when cache exceeds max size', async () => {
    const fetchFn = jest.fn(async (url, init) => {
      const body = JSON.parse((init as any).body);
      return okResponse([body.prompt.charCodeAt(0)]);
    });
    const embed = createEmbedder({
      ollamaUrl: 'http://x', model: 'm', fetchFn,
      cacheMax: 2,
    });
    await embed('a'); // cache: [a]
    await embed('b'); // cache: [a, b]
    await embed('c'); // cache: [b, c] — evicts a
    // Re-querying 'a' must re-fetch because it was evicted.
    await embed('a');
    // Total distinct fetches: a, b, c, a again = 4.
    expect(fetchFn).toHaveBeenCalledTimes(4);
  });

  it('LRU touch on cache hit keeps the entry from being evicted', async () => {
    const fetchFn = jest.fn(async (url, init) => {
      const body = JSON.parse((init as any).body);
      return okResponse([body.prompt.length]);
    });
    const embed = createEmbedder({
      ollamaUrl: 'http://x', model: 'm', fetchFn,
      cacheMax: 2,
    });
    await embed('aa'); // cache: [aa]
    await embed('bb'); // cache: [aa, bb]
    await embed('aa'); // cache hit — touches aa, moving it to newest. order: [bb, aa]
    await embed('cc'); // evicts bb (oldest). order: [aa, cc]
    await embed('aa'); // should be cache hit — aa survived.
    expect(fetchFn).toHaveBeenCalledTimes(3); // aa, bb, cc — no second aa fetch.
  });
});
