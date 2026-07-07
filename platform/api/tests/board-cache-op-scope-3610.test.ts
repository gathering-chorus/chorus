// @test-type: unit
// #3610 — scheduled-job op attribution must cover ONLY the sync slice.
// The old server.ts wiring set currentOp='boardCache' and cleared it in
// .finally() around the whole async refresh — so the label spanned the
// cards-list shell-out await (seconds), and ANY block by ANY code during an
// in-flight refresh was reported as op=boardCache. That mislabel is why the
// nightly "boardCache" captures can't be trusted as call-site evidence.
// The only loop-blocking work in refresh is the sync parse; the op label
// must be pinned to exactly that.
import { createBoardCache } from '../src/board-cache';

describe('#3610 boardCache op label scoped to the sync parse slice', () => {
  const STDOUT = `WIP (1):\n  1  A [Wren|P1|type:fix]\n`;

  it('setOp is NOT set while the shell-out await is pending', async () => {
    const ops: Array<string | null> = [];
    let resolveRun: (s: string) => void = () => {};
    const pending = new Promise<string>((r) => { resolveRun = r; });

    const cache = createBoardCache({
      run: () => pending,
      setOp: (op) => ops.push(op),
    });

    const refreshing = cache.refresh();
    await Promise.resolve(); // let refresh() reach its await
    expect(ops).toEqual([]); // await in flight → no op claimed

    resolveRun(STDOUT);
    await refreshing;
    expect(ops).toEqual(['boardCache', null]); // set + cleared around the parse only
    expect(cache.getCards()).toHaveLength(1);
  });

  it('setOp is cleared even when the parse input is garbage (finally contract)', async () => {
    const ops: Array<string | null> = [];
    const cache = createBoardCache({
      run: () => Promise.resolve('not a board at all'),
      setOp: (op) => ops.push(op),
    });
    await cache.refresh();
    expect(ops).toEqual(['boardCache', null]);
  });

  it('run() rejection never claims the op (nothing sync ran)', async () => {
    const ops: Array<string | null> = [];
    const cache = createBoardCache({
      run: () => Promise.reject(new Error('cards CLI down')),
      setOp: (op) => ops.push(op),
    });
    await cache.refresh(); // swallows — last-good snapshot contract
    expect(ops).toEqual([]);
  });

  it('setOp is optional — existing callers unaffected', async () => {
    const cache = createBoardCache({ run: () => Promise.resolve(STDOUT) });
    await cache.refresh();
    expect(cache.getCards()).toHaveLength(1);
  });
});
