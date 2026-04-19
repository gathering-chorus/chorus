import { createScheduledReindex } from '../src/scheduled-reindex';

describe('createScheduledReindex', () => {
  it('invokes indexAllSources and logs a count of indexed sources', async () => {
    const index = jest.fn(async () => ({ a: 'indexed 3', b: 'indexed 5', c: 'skipped' }));
    const log = jest.fn();
    const error = jest.fn();
    const run = createScheduledReindex({ indexAllSources: index, log, error });
    await run();
    expect(index).toHaveBeenCalledTimes(1);
    expect(log).toHaveBeenCalledWith(expect.stringContaining('2 sources indexed'));
    expect(error).not.toHaveBeenCalled();
  });

  it('counts only values that start with "indexed "', async () => {
    const index = jest.fn(async () => ({
      ok: 'indexed 1',
      more: 'indexed 2',
      skip: 'skipped',
      bad: 'failed',
      n: 42,
    }));
    const log = jest.fn();
    const run = createScheduledReindex({ indexAllSources: index, log, error: jest.fn() });
    await run();
    expect(log.mock.calls[0][0]).toContain('2 sources indexed');
  });

  it('logs an error and does not throw when indexAllSources rejects', async () => {
    const index = jest.fn(async () => { throw new Error('boom'); });
    const log = jest.fn();
    const error = jest.fn();
    const run = createScheduledReindex({ indexAllSources: index, log, error });
    await expect(run()).resolves.toBeUndefined();
    expect(error).toHaveBeenCalledWith(expect.stringContaining('boom'));
    expect(log).not.toHaveBeenCalled();
  });

  it('serializes overlapping invocations — the second run short-circuits while the first is in flight', async () => {
    let resolve: () => void = () => {};
    const firstCall = new Promise<void>(r => { resolve = r; });
    const index = jest.fn(async () => {
      await firstCall;
      return { a: 'indexed 1' };
    });
    const log = jest.fn();
    const run = createScheduledReindex({ indexAllSources: index, log, error: jest.fn() });
    const p1 = run();
    const p2 = run();
    expect(index).toHaveBeenCalledTimes(1);
    resolve();
    await Promise.all([p1, p2]);
  });

  it('allows a follow-up run after the first run completes', async () => {
    const index = jest.fn(async () => ({ a: 'indexed 1' }));
    const run = createScheduledReindex({ indexAllSources: index, log: jest.fn(), error: jest.fn() });
    await run();
    await run();
    expect(index).toHaveBeenCalledTimes(2);
  });

  it('releases the reindexRunning flag even on error', async () => {
    const index = jest.fn()
      .mockImplementationOnce(async () => { throw new Error('first fails'); })
      .mockImplementationOnce(async () => ({ a: 'indexed 1' }));
    const run = createScheduledReindex({ indexAllSources: index as any, log: jest.fn(), error: jest.fn() });
    await run();
    await run();
    expect(index).toHaveBeenCalledTimes(2);
  });
});
