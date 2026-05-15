/**
 * #2750 slice 2 — MCP chorus_acp atomic transaction.
 *
 * /acp loop today is 7 steps the model executes (pull → commit → push → PR
 * open → PR merge → cards done → spine + branch close). Skills are markdown
 * — model compliance is unreliable; today's session has live receipts of
 * shortcut + improvised steps. This card moves execution to substrate:
 * one MCP call runs the whole transaction deterministically. The /acp
 * skill collapses to "call this one thing."
 *
 * Per DEC-1674 (TDD): RED first, then handler. The specs describe
 * Jeff's experience: he says /acp 2750 once and the work goes from werk
 * to merged-on-main with the card Done — no manual gh-merge, no manual
 * cleanup, no remembering which step is next.
 */
import { buildMcpServer } from '../src/mcp/server';

const oneCard = [{ id: 2750, owner: 'kade', title: 'chorus_acp atomic' }];

describe('#2750 slice 2 — chorus_acp MCP atomic transaction', () => {
  describe('AC1 — registration', () => {
    test('exposes chorus_acp in tools/list', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const names = result.tools.map((t: { name: string }) => t.name);
      expect(names).toContain('chorus_acp');
    });

    test('input schema requires role; card_id is optional intent-assertion (#2868)', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const tool = result.tools.find((t: { name: string }) => t.name === 'chorus_acp');
      expect(tool).toBeDefined();
      expect(tool.inputSchema.required).toEqual(['role']);
      expect(tool.inputSchema.additionalProperties).toBe(false);
      expect(tool.inputSchema.properties.role.enum.sort()).toEqual(['kade', 'silas', 'wren']);
      // #2868: card_id property exists on schema and is integer (not in required).
      expect(tool.inputSchema.properties.card_id).toBeDefined();
      expect(tool.inputSchema.properties.card_id.type).toBe('integer');
      expect(tool.inputSchema.required).not.toContain('card_id');
    });
  });

  describe('AC2 — atomic happy path', () => {
    function buildHappyExec() {
      const calls: Array<{ file: string; args: string[]; cwd?: string }> = [];
      const fn = jest.fn(async (file: string, args: string[], opts: { cwd?: string } = {}) => {
        calls.push({ file, args, cwd: opts.cwd });
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') {
          return { stdout: '[kade/2750 abcd5678] kade: m\n', stderr: '' };
        }
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'view') {
          // Pretend PR doesn't exist yet (first call) so handler creates it
          const err = new Error('no PR');
          (err as { code?: number }).code = 1;
          throw err;
        }
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'create') {
          return { stdout: 'https://github.com/x/y/pull/999\n', stderr: '' };
        }
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'merge') {
          return { stdout: 'merged\n', stderr: '' };
        }
        if (file.endsWith('cards') && args[0] === 'done') {
          return { stdout: 'Done: #2750\n', stderr: '' };
        }
        if (file.endsWith('chorus-log')) return { stdout: '', stderr: '' };
        if (file.endsWith('chorus-werk') && args[0] === 'remove') return { stdout: 'removed\n', stderr: '' };
        // #2863 — release trigger: kickstarts building-pipeline post-merge.
        if (file === 'launchctl' && args[0] === 'kickstart') return { stdout: '', stderr: '' };
        throw new Error(`unexpected: ${file} ${args.join(' ')}`);
      });
      return { fn, calls };
    }

    test('runs pull + commit + push + PR open + PR merge + cards done + spine + werk close in order', async () => {
      const { fn: exec, calls } = buildHappyExec();
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      const result = await handler(
        { method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } },
        {},
      );

      // Order: commit → push → pr view → pr create → pr merge → cards done → werk close
      const ordered = calls.map((c) => `${c.file.split('/').pop()}:${c.args[0]}${c.args[1] ? '/' + c.args[1] : ''}`);
      const has = (sub: string) => ordered.some((line) => line.includes(sub));
      expect(has('git-queue.sh:commit')).toBe(true);
      expect(has('git-queue.sh:push')).toBe(true);
      expect(has('gh:pr/merge')).toBe(true);
      expect(has('cards:done')).toBe(true);
      expect(has('chorus-werk:remove')).toBe(true);

      // Spine event emitted on success
      expect(events.find((e) => e.event === 'card.accepted')).toBeDefined();

      // #2863 release-trigger: post-merge step kickstarts building-pipeline.
      // Distinct from prior step hardening (#2782 verify-after, #2793
      // werk-on-main, #2799 force-with-lease) — this is a new step that
      // makes /acp the release event for the deploy chain.
      const launchctlCall = calls.find((c) => c.file === 'launchctl' && c.args[0] === 'kickstart');
      expect(launchctlCall).toBeDefined();
      expect(launchctlCall!.args[1]).toMatch(/^gui\/\d+\/com\.chorus\.building-pipeline$/);
      expect(events.find((e) => e.event === 'chorus_acp.release-trigger.started')).toBeDefined();
      expect(events.find((e) => e.event === 'chorus_acp.release-trigger.completed')).toBeDefined();

      // Result shape
      const text = result.content[0].text;
      const parsed = JSON.parse(text);
      expect(parsed.role).toBe('kade');
      expect(parsed.card_id).toBe(2750);
      expect(parsed.sha).toBe('abcd5678');
    });
  });

  describe('AC3 — refusal taxonomy', () => {
    test('refuses hook-fail when commit blocked by pre-commit', async () => {
      const exec = jest.fn(async (file: string, args: string[]) => {
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') {
          const err = new Error('cmd failed');
          (err as { code?: number; stderr?: string }).code = 1;
          (err as { stderr?: string }).stderr = 'pre-commit: 🔴 1/5 checks failed';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {}),
      ).rejects.toThrow(/hook-fail/);
      expect(events.find((e) => e.event === 'chorus_acp.refused')?.fields.reason).toBe('hook-fail');
    });

    test('refuses pr-merge-fail when gh pr merge fails', async () => {
      const exec = jest.fn(async (file: string, args: string[]) => {
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') return { stdout: '[kade/2750 abcd] kade: m\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        // #2911: cherry shows new commit (+ line) → unmerged → normal flow
        if (file === 'git' && args[0] === 'cherry') return { stdout: '+ abc1234 new work\n', stderr: '' };
        if (file === 'gh' && args[1] === 'view') { const e = new Error('no PR') as { code?: number }; e.code = 1; throw e; }
        if (file === 'gh' && args[1] === 'create') return { stdout: 'https://x/pr/1\n', stderr: '' };
        if (file === 'gh' && args[1] === 'merge') {
          const err = new Error('merge failed');
          (err as { code?: number; stderr?: string }).code = 1;
          (err as { stderr?: string }).stderr = 'mergeable status: BLOCKED';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {}),
      ).rejects.toThrow(/pr-merge-fail/);
      expect(events.find((e) => e.event === 'chorus_acp.refused')?.fields.reason).toBe('pr-merge-fail');
    });

    // #2793 werk-on-main guard retired by #2915 — the ephemeral model
    // (`chorus-werk add` always creates the werk on <role>/<card>) makes a
    // werk-on-main state structurally unreachable from a normal /pull, so the
    // guard and its test were removed rather than tag-and-kept.
  });

  // #2799 — push step must pass --force-with-lease so the post-rebase
  // non-fast-forward case (chorus_acp's internal pull --rebase moved local
  // SHAs ahead of origin's pre-rebase ref) lands cleanly via the typed
  // surface instead of falling back to /tmp/wren-N-push.sh raw-git scripts.
  // 3 receipts today (Wren #2727, Kade #2790, Wren #2763); each manually
  // recovered with `_GIT_QUEUE_PUSH=1 git push --force-with-lease` outside
  // chorus_acp's typed taxonomy. This card brings the recovery in-house.
  describe('#2799 — push step uses --force-with-lease', () => {
    test('chorus_acp push args include --force-with-lease', async () => {
      const calls: Array<{ args: string[] }> = [];
      const exec = jest.fn(async (file: string, args: string[]) => {
        calls.push({ args });
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') return { stdout: '[kade/2799 abcd] kade: m\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2799\n', stderr: '' };
        // #2911: cherry shows + line → unmerged → normal flow
        if (file === 'git' && args[0] === 'cherry') return { stdout: '+ abc1234 new work\n', stderr: '' };
        if (file === 'gh' && args[1] === 'view') return { stdout: JSON.stringify({ url: 'https://github.com/x/y/pull/1', state: 'OPEN' }), stderr: '' };
        if (file === 'gh' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done\n', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {});

      const pushCalls = calls.filter((c) => c.args[0] === 'push');
      expect(pushCalls.length).toBeGreaterThan(0);
      // Every push call from chorus_acp must include --force-with-lease.
      // Order requirement: --force-branch precedes --force-with-lease per
      // git-queue.sh do_push parser order. --branch <ref> may follow.
      for (const c of pushCalls) {
        expect(c.args).toContain('--force-with-lease');
        const fbIdx = c.args.indexOf('--force-branch');
        const fwlIdx = c.args.indexOf('--force-with-lease');
        expect(fbIdx).toBeLessThan(fwlIdx);
      }
    });
  });

  describe('AC4 — idempotent partial-failure recovery', () => {
    test('skips PR create when PR already exists (idempotent re-run)', async () => {
      const calls: Array<{ args: string[] }> = [];
      const exec = jest.fn(async (file: string, args: string[]) => {
        calls.push({ args });
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') return { stdout: '[kade/2750 abcd] kade: m\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        // #2911: cherry shows + line → unmerged → normal flow (idempotent path still creates+merges; "already merged" is a separate concept now)
        if (file === 'git' && args[0] === 'cherry') return { stdout: '+ abc1234 new work\n', stderr: '' };
        if (file === 'gh' && args[1] === 'view') {
          // PR already exists and is OPEN — view succeeds
          return { stdout: JSON.stringify({ url: 'https://github.com/x/y/pull/999', state: 'OPEN' }), stderr: '' };
        }
        if (file === 'gh' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done: #2750\n', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {});

      // PR create must NOT be in the call list (idempotent: detected existing)
      const createCalls = calls.filter((c) => c.args[0] === 'pr' && c.args[1] === 'create');
      expect(createCalls.length).toBe(0);
      // PR merge SHOULD be in the call list (existing PR can still merge)
      const mergeCalls = calls.filter((c) => c.args[0] === 'pr' && c.args[1] === 'merge');
      expect(mergeCalls.length).toBeGreaterThan(0);
    });
  });

  // #2782 / #2911 fast-path test blocks removed by #2923. They exercised the
  // alreadyMerged short-circuit (skip commit/push/PR → cards-done), which
  // #2923 deleted from executeAcp — it was a false-success path that marked
  // uncommitted werks Done with nothing shipped (`git cherry` empty is
  // identical for "already merged" and "never committed"). The normal
  // commit→push→pr→merge path is idempotent on re-runs, so the
  // already-merged case is handled without a separate branch; the AC4 and
  // #2868 blocks below cover the normal path.

  describe('AC5 — werk-aware', () => {
    test('routes git-queue cwd via resolveWorkingTree (slice 1 reuse)', async () => {
      const cwds: string[] = [];
      const exec = jest.fn(async (file: string, args: string[], opts: { cwd?: string } = {}) => {
        if (file.endsWith('git-queue.sh') || (file === 'git' && args[0] === 'rev-parse')) {
          if (opts.cwd) cwds.push(opts.cwd);
        }
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') return { stdout: '[kade/2750 abcd] kade: m\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        if (file === 'gh' && args[1] === 'view') { const e = new Error('no PR') as { code?: number }; e.code = 1; throw e; }
        if (file === 'gh' && args[1] === 'create') return { stdout: 'https://x/pr/1\n', stderr: '' };
        if (file === 'gh' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done: #2750\n', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        resolveWorkingTree: ((_role: 'kade') => '/fake/chorus-werk/kade') as never,
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {});

      cwds.forEach((cwd) => expect(cwd).toBe('/fake/chorus-werk/kade'));
    });
  });

  // #2868 — card_id intent-assertion. The skill UI accepts /acp <card-id>;
  // pre-#2868 the MCP signature dropped the arg and derived from branch
  // silently, so a werk on wren/2851 with a typed `/acp 2847` ran against
  // 2851 with no signal. This block asserts: when caller passes card_id,
  // MCP refuses with `card-mismatch` if branch-derived id differs; runs
  // normally if they match; preserves backward-compat when no card_id passed.
  describe('#2868 — card_id intent-assertion + mismatch refusal', () => {
    test('refuses card-mismatch when args.card_id differs from branch-derived', async () => {
      const exec = jest.fn(async (file: string, args: string[]) => {
        if (file === 'git' && args[0] === 'rev-parse' && args[1] === '--abbrev-ref') {
          return { stdout: 'wren/2851\n', stderr: '' };
        }
        throw new Error(`unexpected: ${file} ${args.join(' ')}`);
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'wren', {
        boardReader: (async () => ({ ok: true, cards: [{ id: 2851, owner: 'wren', title: 'x' }] })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler(
          { method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'wren', card_id: 2847 } } },
          {},
        ),
      ).rejects.toThrow(/card-mismatch/);
      // No commit / push / merge happened — refused at preflight.
      const commits = exec.mock.calls.filter((c: unknown[]) => {
        const file = c[0] as string;
        const a = c[1] as string[];
        return file.endsWith('git-queue.sh') && a[0] === 'commit';
      });
      expect(commits.length).toBe(0);
      // Refusal event names the mismatch.
      const refused = events.find((e) => e.event === 'chorus_acp.refused' && e.fields.reason === 'card-mismatch');
      expect(refused).toBeDefined();
      expect(refused!.fields.requested_card_id).toBe(2847);
      expect(refused!.fields.branch_card_id).toBe(2851);
    });

    test('runs normally when args.card_id matches branch-derived', async () => {
      const calls: Array<{ file: string; args: string[] }> = [];
      const exec = jest.fn(async (file: string, args: string[]) => {
        calls.push({ file, args });
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') {
          return { stdout: '[kade/2750 abcd5678] m\n', stderr: '' };
        }
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'view') {
          const err = new Error('no PR'); (err as { code?: number }).code = 1; throw err;
        }
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'create') {
          return { stdout: 'https://github.com/x/y/pull/999\n', stderr: '' };
        }
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done\n', stderr: '' };
        if (file.endsWith('chorus-log')) return { stdout: '', stderr: '' };
        if (file.endsWith('chorus-werk') && args[0] === 'remove') return { stdout: 'removed\n', stderr: '' };
        if (file === 'launchctl' && args[0] === 'kickstart') return { stdout: '', stderr: '' };
        throw new Error(`unexpected: ${file} ${args.join(' ')}`);
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: (() => undefined) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error
      const handler = (server as any)._requestHandlers.get('tools/call');
      const result = await handler(
        { method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade', card_id: 2750 } } },
        {},
      );
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.card_id).toBe(2750);
    });

    test('backward-compat: no card_id arg = derive from branch as today', async () => {
      const exec = jest.fn(async (file: string, args: string[]) => {
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') {
          return { stdout: '[kade/2750 abcd5678] m\n', stderr: '' };
        }
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'view') {
          const err = new Error('no PR'); (err as { code?: number }).code = 1; throw err;
        }
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'create') return { stdout: 'https://x/y/pull/9\n', stderr: '' };
        if (file === 'gh' && args[0] === 'pr' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done\n', stderr: '' };
        if (file.endsWith('chorus-log')) return { stdout: '', stderr: '' };
        if (file.endsWith('chorus-werk') && args[0] === 'remove') return { stdout: 'removed\n', stderr: '' };
        if (file === 'launchctl' && args[0] === 'kickstart') return { stdout: '', stderr: '' };
        throw new Error(`unexpected: ${file} ${args.join(' ')}`);
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: (() => undefined) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error
      const handler = (server as any)._requestHandlers.get('tools/call');
      const result = await handler(
        { method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } },
        {},
      );
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.card_id).toBe(2750);
    });
  });

  // #2913 — reused branch: a stale MERGED/CLOSED PR + new commits opens a
  // fresh PR. Self-acp receipt, 2026-05-14: kade/2913 shipped via merged PR
  // #241, then accrued a new commit. /acp ran — `git cherry` correctly showed
  // the new commit unmerged (no fast-path), but the PR step ran
  // `gh pr view kade/2913`, which returns the *most recent* PR for the branch
  // name: the MERGED #241. `gh pr merge` on #241 errored "already merged",
  // caught as idempotent success → new commit shipped nothing, cards-done ran
  // falsely, chorus-werk remove couldn't delete the unmerged branch
  // (branch_closed:false). The PR step must consult the found PR's state: a
  // MERGED/CLOSED PR on a branch with unmerged commits is stale — open a fresh
  // PR for the new work, don't try to re-merge the dead one.
  describe('#2913 — reused branch: stale merged PR + new commits opens a fresh PR', () => {
    test('opens a fresh PR when gh pr view returns a MERGED PR but commits are unmerged', async () => {
      const calls: Array<{ file: string; args: string[] }> = [];
      const exec = jest.fn(async (file: string, args: string[]) => {
        calls.push({ file, args });
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2913\n', stderr: '' };
        if (file === 'git' && args[0] === 'fetch') return { stdout: '', stderr: '' };
        // cherry shows '+' → new commit, unmerged → no fast-path
        if (file === 'git' && args[0] === 'cherry') return { stdout: '+ 1ecae41a doc fix on top of merged PR\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') return { stdout: '[kade/2913 1ecae41a] kade: m\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        // gh pr view returns the stale MERGED PR #241 for the reused branch name
        if (file === 'gh' && args[1] === 'view') {
          return { stdout: JSON.stringify({ url: 'https://github.com/x/y/pull/241', state: 'MERGED' }), stderr: '' };
        }
        if (file === 'gh' && args[1] === 'create') return { stdout: 'https://github.com/x/y/pull/242\n', stderr: '' };
        if (file === 'gh' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done\n', stderr: '' };
        if (file.endsWith('chorus-werk') && args[0] === 'remove') return { stdout: 'removed\n', stderr: '' };
        if (file === 'launchctl') return { stdout: '', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: [{ id: 2913, owner: 'kade', title: 'chorus-werk rewrite' }] })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      const result = await handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {});

      // A fresh PR MUST be created — the stale merged #241 is not the target.
      const createCalls = calls.filter((c) => c.file === 'gh' && c.args[1] === 'create');
      expect(createCalls.length).toBe(1);
      // pr-merge still runs (against the fresh PR).
      expect(calls.some((c) => c.file === 'gh' && c.args[1] === 'merge')).toBe(true);
      // Result carries the fresh PR url, not the stale #241.
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.pr_url).toContain('/pull/242');
      expect(parsed.pr_url).not.toContain('/pull/241');
    });

    test('reuses the existing PR when gh pr view returns an OPEN PR (no stale-PR churn)', async () => {
      const calls: Array<{ file: string; args: string[] }> = [];
      const exec = jest.fn(async (file: string, args: string[]) => {
        calls.push({ file, args });
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2913\n', stderr: '' };
        if (file === 'git' && args[0] === 'fetch') return { stdout: '', stderr: '' };
        if (file === 'git' && args[0] === 'cherry') return { stdout: '+ abc1234 new work\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') return { stdout: '[kade/2913 abc1234] kade: m\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'gh' && args[1] === 'view') {
          return { stdout: JSON.stringify({ url: 'https://github.com/x/y/pull/300', state: 'OPEN' }), stderr: '' };
        }
        if (file === 'gh' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done\n', stderr: '' };
        if (file.endsWith('chorus-werk') && args[0] === 'remove') return { stdout: 'removed\n', stderr: '' };
        if (file === 'launchctl') return { stdout: '', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: [{ id: 2913, owner: 'kade', title: 'x' }] })) as never,
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      const result = await handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {});

      // OPEN PR is reused — no fresh PR created.
      const createCalls = calls.filter((c) => c.file === 'gh' && c.args[1] === 'create');
      expect(createCalls.length).toBe(0);
      const parsed = JSON.parse(result.content[0].text);
      expect(parsed.pr_url).toContain('/pull/300');
    });
  });

  describe('#2931 — duration_ms on completed step events', () => {
    test('every chorus_acp.*.completed event carries duration_ms', async () => {
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const exec = jest.fn(async (file: string, args: string[]) => {
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') return { stdout: '[kade/2750 abcd5678] msg\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        if (file === 'gh' && args[1] === 'view') { const e = new Error('no PR') as { code?: number }; e.code = 1; throw e; }
        if (file === 'gh' && args[1] === 'create') return { stdout: 'https://x/pr/1\n', stderr: '' };
        if (file === 'gh' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done\n', stderr: '' };
        if (file.endsWith('chorus-log')) return { stdout: '', stderr: '' };
        if (file.endsWith('chorus-werk')) return { stdout: 'removed\n', stderr: '' };
        if (file === 'launchctl') return { stdout: '', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {});

      // Every .completed event must carry numeric duration_ms.
      const completed = events.filter((e) => /^chorus_acp\.[a-z-]+\.completed$/.test(e.event));
      expect(completed.length).toBeGreaterThan(3); // commit, push, pr-create, pr-merge, cards-done, ...
      for (const e of completed) {
        expect(typeof e.fields.duration_ms).toBe('number');
        expect(e.fields.duration_ms as number).toBeGreaterThanOrEqual(0);
      }
      // Specific contract: each AC2-named step appears.
      const stepNames = completed.map((e) => e.event.replace(/^chorus_acp\.|\.completed$/g, ''));
      for (const step of ['commit', 'push', 'pr-create', 'pr-merge', 'release-trigger']) {
        expect(stepNames).toContain(step);
      }
    });
  });

  describe('#2931 — Chorus-Trace-Id + Chorus-Card-Id commit trailers', () => {
    // Why: chorus_acp mints a trace_id at the top of the transaction and
    // emits every spine event with it. The build pipeline runs LATER in a
    // separate process tree (launchctl-kickstarted building-pipeline), with
    // no inherited env. Writing the trace_id as a `Chorus-Trace-Id:` git
    // trailer on the acp commit lets build-signed.sh extract it and export
    // CHORUS_TRACE_ID, so chorus-log's env-bridge (#2857) tags build.* and
    // deploy.* events with the same trace_id ACP used. Result: a single
    // chain in chorus_logs_for_trace from pull → acp → build → deploy.
    test('commit message carries Chorus-Trace-Id and Chorus-Card-Id trailers', async () => {
      const calls: Array<{ file: string; args: string[] }> = [];
      const exec = jest.fn(async (file: string, args: string[]) => {
        calls.push({ file, args });
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') {
          return { stdout: '[kade/2750 abcd5678] kade: acp #2750\n', stderr: '' };
        }
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'kade/2750\n', stderr: '' };
        if (file === 'gh' && args[1] === 'view') { const e = new Error('no PR') as { code?: number }; e.code = 1; throw e; }
        if (file === 'gh' && args[1] === 'create') return { stdout: 'https://x/pr/1\n', stderr: '' };
        if (file === 'gh' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file.endsWith('cards') && args[0] === 'done') return { stdout: 'Done: #2750\n', stderr: '' };
        if (file.endsWith('chorus-log')) return { stdout: '', stderr: '' };
        if (file.endsWith('chorus-werk') && args[0] === 'remove') return { stdout: 'removed\n', stderr: '' };
        if (file === 'launchctl' && args[0] === 'kickstart') return { stdout: '', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: (() => undefined) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {});

      const commitCall = calls.find((c) => c.file.endsWith('git-queue.sh') && c.args[0] === 'commit');
      expect(commitCall).toBeDefined();
      // -m is the second-to-last arg position pattern (see commitArgs construction).
      const mIdx = commitCall!.args.lastIndexOf('-m');
      expect(mIdx).toBeGreaterThan(-1);
      const msg = commitCall!.args[mIdx + 1];
      // Subject line preserved.
      expect(msg.split('\n')[0]).toBe('kade: acp #2750');
      // Blank line separates subject from trailer block (git-trailer convention).
      expect(msg).toMatch(/\n\nChorus-Trace-Id: [0-9a-f-]+/);
      // Card-Id trailer present when cardId resolved.
      expect(msg).toMatch(/\nChorus-Card-Id: 2750/);
      // Trace-id is a v7 UUID-shape minted by mintTraceIdV7 — non-empty hex+dashes.
      const traceMatch = msg.match(/Chorus-Trace-Id: ([0-9a-f-]+)/);
      expect(traceMatch).not.toBeNull();
      expect(traceMatch![1].length).toBeGreaterThan(8);
    });

    test('commit message omits Chorus-Card-Id when cardId cannot be derived', async () => {
      // Board returns no cards AND branch is not <role>/<id> → cardId stays null.
      const calls: Array<{ file: string; args: string[] }> = [];
      const exec = jest.fn(async (file: string, args: string[]) => {
        calls.push({ file, args });
        if (file === 'git' && args[0] === 'rev-parse') return { stdout: 'main\n', stderr: '' };
        if (file.endsWith('git-queue.sh') && args[0] === 'commit') {
          return { stdout: '[main abcd] kade: acp #unknown\n', stderr: '' };
        }
        if (file.endsWith('git-queue.sh') && args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        if (file === 'gh' && args[1] === 'view') { const e = new Error('no PR') as { code?: number }; e.code = 1; throw e; }
        if (file === 'gh' && args[1] === 'create') return { stdout: 'https://x/pr/1\n', stderr: '' };
        if (file === 'gh' && args[1] === 'merge') return { stdout: 'merged\n', stderr: '' };
        if (file === 'launchctl') return { stdout: '', stderr: '' };
        if (file.endsWith('chorus-log')) return { stdout: '', stderr: '' };
        if (file.endsWith('chorus-werk')) return { stdout: '', stderr: '' };
        if (file.endsWith('cards')) return { stdout: '', stderr: '' };
        return { stdout: '', stderr: '' };
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: [] })) as never,
        emitSpineEvent: (() => undefined) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      try {
        await handler({ method: 'tools/call', params: { name: 'chorus_acp', arguments: { role: 'kade' } } }, {});
      } catch {
        /* downstream steps may refuse; only the commit args matter */
      }

      const commitCall = calls.find((c) => c.file.endsWith('git-queue.sh') && c.args[0] === 'commit');
      expect(commitCall).toBeDefined();
      const mIdx = commitCall!.args.lastIndexOf('-m');
      const msg = commitCall!.args[mIdx + 1];
      expect(msg).toMatch(/Chorus-Trace-Id: /);
      expect(msg).not.toMatch(/Chorus-Card-Id: /);
    });
  });
});
