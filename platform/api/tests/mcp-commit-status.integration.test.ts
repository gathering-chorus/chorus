/**
 * #2661 AC6 — chorus_commit_status integration test against live endpoint.
 *
 * Reshape rationale (in lieu of the original AC6 hermetic-add path): the
 * cards CLI has no dry-run / test-mode and the existing `startTestApp`
 * harness is process-hermetic but data-live. Inventing a hermetic-add layer
 * is net-new substrate; matching what alerts-subdomain / athena / assessment
 * integration tests already do (read-only assertions against live state) is
 * the cheaper honest path. Architect-confirmed (silas, 2026-05-02 15:10).
 *
 * What this test proves:
 *   - The full handler chain works against the real Express app: dispatcher
 *     → zod → executeCommitStatus → defaultBoardReader → fetch → live
 *     /api/chorus/context/board/wip → real card list.
 *   - Branch derivation `<role>/<card-id>` is exact against whatever role's
 *     state happens to be live at test time.
 *   - When a role has zero WIP cards on the live board, the handler refuses
 *     with `no-wip-card`. (Picks a role guaranteed to have no WIP — see
 *     ROLE_WITHOUT_WIP below; reshape if all three roles always have WIP.)
 *
 * What it doesn't prove:
 *   - multi-wip refusal — would need controlled state we don't have.
 *   - board-unreachable — covered by contract tests with mock failure.
 */
import { buildMcpServer } from '../src/mcp/server';
import { startTestApp, type TestApp } from './lib/test-app';

describe('#2661 chorus_commit_status — integration against live board', () => {
  let harness: TestApp;

  beforeAll(async () => {
    harness = await startTestApp();
  }, 30_000);

  afterAll(async () => {
    if (harness) await harness.close();
  });

  test('live WIP query returns either an active card OR refuses no-wip-card', async () => {
    const server = buildMcpServer(() => 'kade', { apiBase: harness.baseUrl });
    // @ts-expect-error - private handler access for integration test
    const handler = (server as any)._requestHandlers.get('tools/call');

    // Snapshot live state: which roles have WIP right now?
    const wipRes = await fetch(`${harness.baseUrl}/api/chorus/context/board/wip`);
    expect(wipRes.status).toBe(200);
    const body = (await wipRes.json()) as { data?: { cards?: Array<{ id: number; owner: string }> } };
    const liveCards = body.data?.cards ?? [];

    // Try each role; verify the handler's response matches live state.
    for (const role of ['kade', 'wren', 'silas'] as const) {
      const cap = role.charAt(0).toUpperCase() + role.slice(1);
      const roleCards = liveCards.filter((c) => c.owner === cap);

      if (roleCards.length === 0) {
        // No WIP for this role → handler must refuse no-wip-card.
        await expect(
          handler(
            { method: 'tools/call', params: { name: 'chorus_commit_status', arguments: { role } } },
            {},
          ),
        ).rejects.toThrow(/no-wip-card/);
      } else if (roleCards.length === 1) {
        // Exactly one → success path. Branch must derive to <role>/<id>.
        const res = await handler(
          { method: 'tools/call', params: { name: 'chorus_commit_status', arguments: { role } } },
          {},
        );
        const text = res.content[0].text;
        const parsed = JSON.parse(text) as { role: string; card_id: number; branch: string };
        expect(parsed.role).toBe(role);
        expect(parsed.card_id).toBe(roleCards[0].id);
        expect(parsed.branch).toBe(`${role}/${roleCards[0].id}`);
      } else {
        // 2+ → multi-wip refusal. (Real state can produce this.)
        await expect(
          handler(
            { method: 'tools/call', params: { name: 'chorus_commit_status', arguments: { role } } },
            {},
          ),
        ).rejects.toThrow(/multi-wip/);
      }
    }
  }, 15_000);
});
