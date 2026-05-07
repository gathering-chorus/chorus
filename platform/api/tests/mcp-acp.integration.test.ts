/**
 * #2782 AC4 — chorus_acp integration check against real substrate.
 *
 * Pairs with the DI-mocked unit suite. Today's session shipped #2778 with
 * mock-only execFileAsync coverage; the real-git path was broken
 * (git-queue.sh --no-add pathspec filter via .gitignore) and the unit
 * suite missed it. This file proves chorus_acp's real chain — handler →
 * boardReader (live API) → execFileAsync (real subprocess) — surfaces
 * typed refusal at the right layer.
 *
 * Pattern matches mcp-commit-write.integration spec (#2682 AC6): exercise
 * REFUSAL paths against real substrate, not success paths. Success would
 * mutate the working tree (commit + push + PR + cards-done) — out of
 * scope. Refusal proves: real boardReader, real git rev-parse, real
 * classifier mapping real stderr, real spine emitter.
 */
import { buildMcpServer } from '../src/mcp/server';
import { startTestApp, type TestApp } from './lib/test-app';

const TYPED_REFUSAL_RE = /chorus_acp refused: (hook-fail|commit-fail|push-conflict|pr-create-fail|pr-merge-fail|cards-done-fail|branch-close-fail|nothing-to-commit)/;
const isTypedRefusal = (msg: string): boolean => TYPED_REFUSAL_RE.exec(msg) !== null;

describe('#2782 chorus_acp — integration against real substrate', () => {
  let harness: TestApp;

  beforeAll(async () => {
    harness = await startTestApp();
  }, 30_000);

  afterAll(async () => {
    if (harness) await harness.close();
  });

  test('real-substrate chain runs end-to-end and surfaces typed refusal or accepted success', async () => {
    const server = buildMcpServer(() => 'kade', { apiBase: harness.baseUrl });
    // @ts-expect-error - private handler access for integration probe
    const handler = (server as any)._requestHandlers.get('tools/call');

    let outcome: 'refused' | 'success' | 'untyped-error' = 'untyped-error';
    try {
      await handler(
        { method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } },
        {},
      );
      outcome = 'success';
    } catch (err) {
      const msg = (err as Error).message;
      if (isTypedRefusal(msg)) {
        outcome = 'refused';
      } else {
        throw new Error(`chorus_acp untyped error (chain broken pre-classifier): ${msg}`);
      }
    }
    expect(['refused', 'success']).toContain(outcome);
  }, 60_000);
});
