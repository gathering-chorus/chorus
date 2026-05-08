/**
 * #2682 — MCP chorus_commit (write) contract tests.
 *
 * Per DEC-1674 (TDD): tests describe Jeff's experience through the MCP
 * surface, RED first, then handler.
 *
 *   - Agent calls chorus_commit(role, paths, message) and gets back a SHA
 *     and the derived branch — service runs the full commit+push under the
 *     existing v2.5 substrate (git-queue.sh) and reports the outcome.
 *   - Refusal taxonomy expands #2661's: no-wip-card | multi-wip |
 *     board-unreachable (inherited via boardReader reuse) PLUS
 *     branch-mismatch | hook-fail | push-conflict (new, classified from
 *     git-queue exit + stderr).
 *   - Each refusal throws + emits chorus_commit.refused with reason.
 *   - Success emits chorus_commit.invoked with role, card_id, paths_count, sha.
 *   - No agent-visible flags/envs/bypasses on the input schema.
 */
import { buildMcpServer } from '../src/mcp/server';

type BoardCard = { id: number; owner: string; title: string };

describe('#2682 chorus_commit (write) MCP tool — contract', () => {
  describe('AC1 — registration', () => {
    test('exposes chorus_commit in tools/list', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access for unit test
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const names = result.tools.map((t: { name: string }) => t.name);
      expect(names).toContain('chorus_commit');
    });

    test('input schema accepts role/paths/message + optional no_add — strict, no smuggling', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/list');
      const result = await handler({ method: 'tools/list', params: {} }, {});
      const tool = result.tools.find((t: { name: string }) => t.name === 'chorus_commit');
      expect(tool).toBeDefined();
      // role/paths/message remain required; no_add is optional (#2778).
      expect(tool.inputSchema.required.sort()).toEqual(['message', 'paths', 'role']);
      const propKeys = Object.keys(tool.inputSchema.properties).sort();
      expect(propKeys).toEqual(['message', 'no_add', 'paths', 'role']);
      expect(tool.inputSchema.additionalProperties).toBe(false);
      expect(tool.inputSchema.properties.role.enum.sort()).toEqual(['kade', 'silas', 'wren']);
      expect(tool.inputSchema.properties.no_add.type).toBe('boolean');
    });

    test('rejects empty paths array', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: [], message: 'm' } } }, {}),
      ).rejects.toThrow(/Invalid arguments/);
    });

    test('rejects empty message', async () => {
      const server = buildMcpServer(() => 'kade');
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: '' } } }, {}),
      ).rejects.toThrow(/Invalid arguments/);
    });
  });

  describe('AC2 — coordination state observed, never refuses (#2687 strip)', () => {
    function buildOkExec() {
      return jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'commit') return { stdout: '[kade/x abcd1234] kade: m\n', stderr: '' };
        if (args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        throw new Error('unexpected ' + args.join(' '));
      });
    }

    test('no-wip-card: commit lands, emits coordination_observed event, no refusal', async () => {
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: [] })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: buildOkExec() as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      const res = await handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {});
      expect(res.content[0].text).toContain('abcd1234');
      const obs = events.find((e) => e.event === 'chorus_commit.coordination_observed');
      expect(obs?.fields.reason).toBe('no-wip-card');
      expect(events.find((e) => e.event === 'chorus_commit.refused')).toBeUndefined();
    });

    test('multi-wip: commit lands, emits coordination_observed with card_ids', async () => {
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({
          ok: true,
          cards: [
            { id: 2682, owner: 'Kade', title: 'a' },
            { id: 2683, owner: 'Kade', title: 'b' },
          ],
        })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: buildOkExec() as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      const res = await handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {});
      expect(res.content[0].text).toContain('abcd1234');
      const obs = events.find((e) => e.event === 'chorus_commit.coordination_observed');
      expect(obs?.fields.reason).toBe('multi-wip');
      expect(events.find((e) => e.event === 'chorus_commit.refused')).toBeUndefined();
    });

    test('board-unreachable: commit lands, emits coordination_observed', async () => {
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: false, reason: 'board-unreachable', detail: 'cli failure' })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: buildOkExec() as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      const res = await handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {});
      expect(res.content[0].text).toContain('abcd1234');
      const obs = events.find((e) => e.event === 'chorus_commit.coordination_observed');
      expect(obs?.fields.reason).toBe('board-unreachable');
      expect(events.find((e) => e.event === 'chorus_commit.refused')).toBeUndefined();
    });
  });

  describe('AC3+AC4 — git-queue delegation + new refusals from exit codes', () => {
    const oneCard: BoardCard[] = [{ id: 2682, owner: 'Kade', title: 'v3-1b chorus_commit' }];

    test('success path: spawns git-queue commit then push, returns SHA, emits invoked', async () => {
      const calls: Array<{ args: string[]; env?: Record<string, string | undefined> }> = [];
      const exec = jest.fn(async (_file: string, args: string[], opts: { env?: Record<string, string | undefined> }) => {
        calls.push({ args, env: opts.env });
        if (args[0] === 'rev-parse' && args.includes('HEAD')) return { stdout: 'kade/2682\n', stderr: '' };
        if (args[0] === 'commit') return { stdout: '[kade/2682-x abcd1234] kade: msg\n', stderr: '' };
        if (args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        throw new Error(`unexpected args: ${args.join(' ')}`);
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');

      const res = await handler(
        { method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['a.ts', 'b.ts'], message: 'kade: #2682 wip' } } },
        {},
      );
      const text = res.content[0].text;
      const parsed = JSON.parse(text) as { sha: string; branch: string; card_id: number };
      expect(parsed.sha).toBe('abcd1234');
      expect(parsed.card_id).toBe(2682);
      expect(parsed.branch).toBe('kade/2682');
      // #2699: HEAD-pin captures branch via rev-parse between commit and push.
      // Calls now: commit → rev-parse → push.
      expect(calls).toHaveLength(3);
      const commitCall = calls.find((c) => c.args[0] === 'commit')!;
      const pushCall = calls.find((c) => c.args[0] === 'push')!;
      expect(commitCall).toBeDefined();
      expect(pushCall).toBeDefined();
      // #2687: --force-branch passed so git-queue's branch-check doesn't surface
      // as a refusal (branch naming is observed via spine, not enforced at write).
      expect(commitCall.args[1]).toBe('--force-branch');
      expect(commitCall.env?.DEPLOY_ROLE).toBe('kade');
      const invoked = events.find((e) => e.event === 'chorus_commit.invoked');
      expect(invoked?.fields.role).toBe('kade');
      expect(invoked?.fields.card_id).toBe(2682);
      expect(invoked?.fields.paths_count).toBe(2);
      expect(invoked?.fields.sha).toBe('abcd1234');
    });

    test('#2778 — no_add:true passes --no-add to git-queue commit (staged-deletes-of-now-ignored case)', async () => {
      // Reproducer for #2778: editing .gitignore + `git rm --cached` on the
      // newly-ignored paths leaves the index pre-staged. Without --no-add,
      // git-queue.sh runs `git add` and refuses the now-ignored paths,
      // making the staged deletion impossible to commit through the typed
      // surface. With no_add:true the MCP must thread --no-add through.
      const calls: Array<{ args: string[] }> = [];
      const exec = jest.fn(async (_file: string, args: string[]) => {
        calls.push({ args });
        if (args[0] === 'rev-parse' && args.includes('HEAD')) return { stdout: 'kade/2778\n', stderr: '' };
        if (args[0] === 'commit') return { stdout: '[kade/2778 deadbeef] kade: m\n', stderr: '' };
        if (args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        throw new Error(`unexpected args: ${args.join(' ')}`);
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: [{ id: 2778, owner: 'Kade', title: 'no_add' }] })) as never,
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');

      await handler(
        { method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['.gitignore', 'noise.log'], message: 'kade: #2778 cleanup', no_add: true } } },
        {},
      );
      const commitCall = calls.find((c) => c.args[0] === 'commit')!;
      // #2731 contract: --no-add follows --force-branch in git-queue.sh do_commit.
      expect(commitCall.args).toContain('--no-add');
      // --force-branch still first per #2687.
      expect(commitCall.args[1]).toBe('--force-branch');
      expect(commitCall.args[2]).toBe('--no-add');
      // Paths still passed through after the flags.
      expect(commitCall.args).toContain('.gitignore');
      expect(commitCall.args).toContain('noise.log');
    });

    test('#2778 — no_add omitted/false does NOT pass --no-add (preserves default git-add behavior)', async () => {
      const calls: Array<{ args: string[] }> = [];
      const exec = jest.fn(async (_file: string, args: string[]) => {
        calls.push({ args });
        if (args[0] === 'rev-parse' && args.includes('HEAD')) return { stdout: 'kade/2778\n', stderr: '' };
        if (args[0] === 'commit') return { stdout: '[kade/2778 deadbeef] kade: m\n', stderr: '' };
        if (args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        throw new Error(`unexpected args: ${args.join(' ')}`);
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: [{ id: 2778, owner: 'Kade', title: 'default' }] })) as never,
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');

      await handler(
        { method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['a.ts'], message: 'kade: m' } } },
        {},
      );
      const commitCall = calls.find((c) => c.args[0] === 'commit')!;
      expect(commitCall.args).not.toContain('--no-add');
    });

    test('refuses hook-fail when git-queue commit blocked by pre-commit', async () => {
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'commit') {
          const err = new Error('hook failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr = 'pre-commit: lint-ratchet failed — see baseline';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {}),
      ).rejects.toThrow(/hook-fail/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('hook-fail');
    });

    test('passes PATH with parent node bin dir prepended (env consistency)', async () => {
      // #2662 dogfood receipt: chorus-api's launchctl PATH puts Homebrew Node
      // first, so subprocess `npx jest` in pre-commit hits a different Node
      // than chorus-api's own runtime. The handler prepends the parent
      // node's bin dir to PATH so subprocesses use the same interpreter
      // (and same compiled native modules).
      const path = require('path') as typeof import('path');
      const expectedPrefix = path.dirname(process.execPath);
      const calls: Array<{ env?: Record<string, string | undefined> }> = [];
      const exec = jest.fn(async (_file: string, args: string[], opts: { env?: Record<string, string | undefined> }) => {
        calls.push({ env: opts.env });
        if (args[0] === 'commit') return { stdout: '[kade/2682-x abcd1234] kade: m', stderr: '' };
        if (args[0] === 'push') return { stdout: '', stderr: '' };
        throw new Error(`unexpected args: ${args.join(' ')}`);
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler(
        { method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } },
        {},
      );
      expect(calls.length).toBeGreaterThanOrEqual(1);
      for (const c of calls) {
        const pathEnv = c.env?.PATH ?? '';
        expect(pathEnv.startsWith(`${expectedPrefix}:`)).toBe(true);
      }
    });

    test('refuses push-conflict when commit succeeds but push rebase fails', async () => {
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'commit') return { stdout: '[kade/2682-x abcd1234] kade: msg\n', stderr: '' };
        if (args[0] === 'push') {
          const err = new Error('push failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr = 'rebase: conflict on platform/api/src/foo.ts';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {}),
      ).rejects.toThrow(/push-conflict/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('push-conflict');
    });
  });

  describe('#2689 + #2697 — classifier split + Mode-A push protection', () => {
    const oneCard: BoardCard[] = [{ id: 2689, owner: 'Wren', title: 'classifier split' }];

    test('#2689 — push subprocess receives --force-branch (mirrors commit step, defends Mode-A wrong-HEAD)', async () => {
      // Repro: server.ts:584 originally invoked git-queue with bare ['push'].
      // Mode-A puts HEAD on a non-role-prefix branch, do_push's check_branch
      // exits 1 BEFORE log_event "build.push.started", chorus_commit catches
      // the non-zero and reports false-positive push-conflict.
      // Fix: push call passes --force-branch, mirroring commit step (line 568).
      const calls: Array<{ args: string[] }> = [];
      const exec = jest.fn(async (_file: string, args: string[]) => {
        calls.push({ args });
        if (args[0] === 'commit') return { stdout: '[wren/2689 abcd1234] m\n', stderr: '' };
        if (args[0] === 'push') return { stdout: '', stderr: '' };
        throw new Error(`unexpected: ${args.join(' ')}`);
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'wren', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler(
        { method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'wren', paths: ['x.html'], message: 'wren: doc' } } },
        {},
      );
      const pushCall = calls.find((c) => c.args[0] === 'push');
      expect(pushCall).toBeDefined();
      expect(pushCall?.args).toContain('--force-branch');
    });

    // #2799 — push step must include --force-with-lease so the post-rebase
    // non-fast-forward case (do_push's pull --rebase moved local SHAs ahead
    // of origin's pre-rebase ref) lands cleanly via the typed surface
    // instead of falling back to /tmp/wren-N-push.sh raw-git scripts.
    // Force-with-lease is the safe variant: refuses on concurrent peer
    // push, pushes our rebased history otherwise.
    test('#2799 — push subprocess receives --force-with-lease for safe post-rebase push', async () => {
      const calls: Array<{ args: string[] }> = [];
      const exec = jest.fn(async (_file: string, args: string[]) => {
        calls.push({ args });
        if (args[0] === 'commit') return { stdout: '[wren/2799 abcd1234] m\n', stderr: '' };
        if (args[0] === 'push') return { stdout: '', stderr: '' };
        throw new Error(`unexpected: ${args.join(' ')}`);
      });
      const server = buildMcpServer(() => 'wren', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler(
        { method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'wren', paths: ['x.html'], message: 'wren: doc' } } },
        {},
      );
      const pushCall = calls.find((c) => c.args[0] === 'push');
      expect(pushCall).toBeDefined();
      expect(pushCall?.args).toContain('--force-with-lease');
      // Order requirement: --force-branch precedes --force-with-lease
      // per git-queue.sh do_push parser order.
      const fbIdx = pushCall!.args.indexOf('--force-branch');
      const fwlIdx = pushCall!.args.indexOf('--force-with-lease');
      expect(fbIdx).toBeLessThan(fwlIdx);
    });

    test('#2697 — commit-fail (not hook-fail) when commit stderr does not match pre-commit signature', async () => {
      // Classifier split: hook-fail reserved for stderr matching pre-commit
      // hook output. Other commit-phase failures (nothing-to-commit, malformed
      // args, unknown subprocess error) classify as commit-fail.
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'commit') {
          const err = new Error('nothing to commit');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr = 'nothing to commit, working tree clean';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'wren', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'wren', paths: ['x.html'], message: 'm' } } }, {}),
      ).rejects.toThrow(/commit-fail/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('commit-fail');
    });

    test('#2697 — hook-fail label preserved when stderr matches pre-commit signature (regression guard)', async () => {
      // Existing line 177 test asserts hook-fail. After classifier split, that
      // path must still classify pre-commit-emitted failures as hook-fail.
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'commit') {
          const err = new Error('cmd failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr = 'pre-commit: 🔴 blocked — lint-ratchet failed';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'wren', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'wren', paths: ['x.html'], message: 'm' } } }, {}),
      ).rejects.toThrow(/hook-fail/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('hook-fail');
    });

    test('#2689 — push-fail (not push-conflict) when push stderr does not match rebase/conflict signature', async () => {
      // Classifier split: push-conflict reserved for actual rebase conflicts.
      // Other push failures (auth, network, hook-block, branch-mismatch leak)
      // classify as push-fail.
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'commit') return { stdout: '[wren/2689 abcd1234] m\n', stderr: '' };
        if (args[0] === 'push') {
          const err = new Error('push failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr = 'remote: unauthorized';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'wren', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'wren', paths: ['x.html'], message: 'm' } } }, {}),
      ).rejects.toThrow(/push-fail/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('push-fail');
    });

    test('#2689 — push-conflict label preserved when stderr matches rebase signature (regression guard)', async () => {
      // Existing line 237 test asserts push-conflict on rebase failure. After
      // split, real rebase conflicts must still classify as push-conflict.
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'commit') return { stdout: '[wren/2689 abcd1234] m\n', stderr: '' };
        if (args[0] === 'push') {
          const err = new Error('push failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr = 'rebase: conflict on platform/api/src/foo.ts';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'wren', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'wren', paths: ['x.html'], message: 'm' } } }, {}),
      ).rejects.toThrow(/push-conflict/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('push-conflict');
    });
  });

  describe('#2699 — HEAD-pin defensive + classifier regex tighten', () => {
    const oneCard: BoardCard[] = [{ id: 2699, owner: 'Kade', title: 'HEAD-pin' }];

    test('captures HEAD ref between commit and push, passes to push via --branch arg (#2705)', async () => {
      // Mode-A defensive: even if HEAD gets bumped between commit and push by
      // a peer's checkout, we push the ref name we captured immediately after
      // commit landed. #2705: explicit --branch <ref> arg replaces the
      // _CHORUS_PUSH_REF env-carry shipped in #2699 (substrate-uniform with
      // --force-branch shape per silas gate-arch feedback).
      const calls: Array<{ args: string[]; env?: Record<string, string | undefined>; file: string }> = [];
      const exec = jest.fn(async (file: string, args: string[], opts: { env?: Record<string, string | undefined> }) => {
        calls.push({ file, args, env: opts.env });
        if (args[0] === 'rev-parse' && args.includes('HEAD')) {
          return { stdout: 'kade/2699\n', stderr: '' };
        }
        if (args[0] === 'commit') return { stdout: '[kade/2699 abcd1234] m\n', stderr: '' };
        if (args[0] === 'push') return { stdout: '', stderr: '' };
        throw new Error(`unexpected: ${args.join(' ')}`);
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler(
        { method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } },
        {},
      );
      // rev-parse fires between commit and push
      const revParseIdx = calls.findIndex((c) => c.args[0] === 'rev-parse');
      const commitIdx = calls.findIndex((c) => c.args[0] === 'commit');
      const pushIdx = calls.findIndex((c) => c.args[0] === 'push');
      expect(revParseIdx).toBeGreaterThan(commitIdx);
      expect(pushIdx).toBeGreaterThan(revParseIdx);
      // #2705: push call carries the captured ref via explicit --branch arg
      // (env-carry retired). AC5: assert --branch arg appears in push call args.
      const pushCall = calls[pushIdx];
      expect(pushCall.args).toContain('--branch');
      const branchIdx = pushCall.args.indexOf('--branch');
      expect(pushCall.args[branchIdx + 1]).toBe('kade/2699');
      // AC3: env-carry removed
      expect(pushCall.env?._CHORUS_PUSH_REF).toBeUndefined();
    });

    test('classifier regex requires failure marker after pre-commit prefix (#2699 tighten)', async () => {
      // Pre-#2699: regex was /^pre-commit:|^.. blocked|hook failed/i — bare
      // 'hook failed' substring matched anywhere; any pre-commit-prefixed
      // stderr line (incl. warnings, non-failure context) classified as
      // hook-fail. Post-#2699: regex requires pre-commit:.*<failure-marker>
      // on same line. Non-failure pre-commit lines now classify as commit-fail.
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'rev-parse' && args.includes('HEAD')) {
          return { stdout: 'kade/2699\n', stderr: '' };
        }
        if (args[0] === 'commit') {
          const err = new Error('cmd failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          // pre-commit warning line WITHOUT a failure marker (no 🔴/❌/failed/blocked).
          // Pre-fix would mis-classify as hook-fail; post-fix is commit-fail.
          (err as unknown as { code: number; stderr: string }).stderr =
            'pre-commit: ⚠ known-fails entries reference closed cards: #1234';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {}),
      ).rejects.toThrow(/commit-fail/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('commit-fail');
    });

    test('#2750 — werk-aware: when resolveWorkingTree returns role werk, all subprocs use that cwd', async () => {
      // Captures silas/2333 first werk-acp gap: chorus_commit ran git-queue.sh
      // in canonical, where the role's files didn't exist (they were in
      // /chorus-werk/<role>/). Handler must route cwd to whatever
      // resolveWorkingTree returns. Production default reads role's
      // settings.json env to detect CHORUS_WERK_ENABLE=1.
      const cwds: string[] = [];
      const exec = jest.fn(async (_file: string, args: string[], opts: { cwd: string }) => {
        cwds.push(opts.cwd);
        if (args[0] === 'rev-parse' && args.includes('HEAD')) {
          return { stdout: 'kade/2750\n', stderr: '' };
        }
        if (args[0] === 'commit') return { stdout: '[kade/2750 abcd5678] kade: m\n', stderr: '' };
        if (args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        throw new Error('unexpected ' + args.join(' '));
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
      await handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['platform/scripts/x.sh'], message: 'm' } } }, {});

      expect(cwds.length).toBeGreaterThan(0);
      cwds.forEach((cwd) => expect(cwd).toBe('/fake/chorus-werk/kade'));
    });

    test('#2750 — flag-off (default): cwd is canonical repo root (regression guard)', async () => {
      // Pre-#2750 callers omit resolveWorkingTree entirely. Default behavior
      // must derive repoRoot from gitQueuePath as today (#2662 contract).
      const cwds: string[] = [];
      const exec = jest.fn(async (_file: string, args: string[], opts: { cwd: string }) => {
        cwds.push(opts.cwd);
        if (args[0] === 'rev-parse' && args.includes('HEAD')) {
          return { stdout: 'kade/2750\n', stderr: '' };
        }
        if (args[0] === 'commit') return { stdout: '[kade/2750 abcd5678] kade: m\n', stderr: '' };
        if (args[0] === 'push') return { stdout: 'pushed\n', stderr: '' };
        throw new Error('unexpected ' + args.join(' '));
      });
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: (() => {}) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/platform/scripts/git-queue.sh',
        // resolveWorkingTree omitted — default falls back to canonical via gitQueuePath derivation
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {});

      cwds.forEach((cwd) => expect(cwd).toBe('/fake'));
    });

    test('classifier regex still labels real pre-commit failure as hook-fail (regression)', async () => {
      // Tightened regex still matches real failure markers on a pre-commit line.
      const exec = jest.fn(async (_file: string, args: string[]) => {
        if (args[0] === 'rev-parse' && args.includes('HEAD')) {
          return { stdout: 'kade/2699\n', stderr: '' };
        }
        if (args[0] === 'commit') {
          const err = new Error('cmd failed');
          (err as unknown as { code: number; stderr: string }).code = 1;
          (err as unknown as { code: number; stderr: string }).stderr =
            'pre-commit: 🔴 1/5 checks failed\n  - clippy-ratchet: 1 hits';
          throw err;
        }
        return { stdout: '', stderr: '' };
      });
      const events: Array<{ event: string; fields: Record<string, unknown> }> = [];
      const server = buildMcpServer(() => 'kade', {
        boardReader: (async () => ({ ok: true, cards: oneCard })) as never,
        emitSpineEvent: ((event: string, fields: Record<string, unknown>) => events.push({ event, fields })) as never,
        execFileAsync: exec as never,
        gitQueuePath: '/fake/git-queue.sh',
      } as never);
      // @ts-expect-error - private handler access
      const handler = (server as any)._requestHandlers.get('tools/call');
      await expect(
        handler({ method: 'tools/call', params: { name: 'chorus_commit', arguments: { role: 'kade', paths: ['x.ts'], message: 'm' } } }, {}),
      ).rejects.toThrow(/hook-fail/);
      expect(events.find((e) => e.event === 'chorus_commit.refused')?.fields.reason).toBe('hook-fail');
    });
  });
});
