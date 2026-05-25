import { runReindexWorker } from '../src/index-worker';

describe('runReindexWorker (#3085 standalone reindex worker core)', () => {
  it('runs indexAllSources, logs the summary, and exits 0 on success', async () => {
    const indexAllSources = jest.fn(async () => ({
      indexed: { memory: 'indexed 3', spine: 'indexed 5' },
      elapsed_ms: 1234,
    }));
    const log = jest.fn();
    const error = jest.fn();
    const code = await runReindexWorker({ indexAllSources, log, error });
    expect(indexAllSources).toHaveBeenCalledTimes(1);
    expect(code).toBe(0);
    expect(log).toHaveBeenCalledWith(expect.stringContaining('2 sources indexed'));
    expect(log).toHaveBeenCalledWith(expect.stringContaining('1234ms'));
    expect(error).not.toHaveBeenCalled();
  });

  it('exits 1 and logs the error (does not throw) when indexAllSources rejects', async () => {
    const indexAllSources = jest.fn(async () => { throw new Error('sqlite locked'); });
    const log = jest.fn();
    const error = jest.fn();
    const code = await runReindexWorker({ indexAllSources, log, error });
    expect(code).toBe(1);
    expect(error).toHaveBeenCalledWith(expect.stringContaining('sqlite locked'));
    expect(log).not.toHaveBeenCalled();
  });
});
