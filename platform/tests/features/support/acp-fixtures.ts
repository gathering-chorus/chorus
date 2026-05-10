/**
 * acp-fixtures.ts — hermetic fixture helpers for #2879 BDD tests.
 *
 * Three tiers matching scenario weight:
 *   Tier 1 (minimal):  chorus.log marker capture + cleanup.
 *                      Used by card-mismatch + no-wip-card refusals.
 *   Tier 2 (+ git):    bare-repo origin + role/card-id branch setup.
 *                      Used by hook/commit/push refusals + regression guards.
 *   Tier 3 (full):     Tier 2 + gh stub state reset + role-state file.
 *                      Used by success path + idempotent + branch-close-non-fatal.
 *
 * The runner test-acp.sh exports ACP_BDD_RUN_DIR (per-run mktemp dir) and
 * GH_STUB_STATE (gh stub state file). Helpers read those env vars so all
 * scenarios share one hermetic root per test run.
 *
 * Pattern note: #2875 (test-demo.sh BDD for /demo) put everything in
 * step_definitions/ — no support/ dir. /acp's fixtures are heavier (bare-repo
 * setup, multi-step git operations) so a separate support module pays for
 * itself. Step defs import named helpers, not whole files.
 */

import { execFileSync } from 'node:child_process';
import * as fs from 'node:fs';
import * as path from 'node:path';
import * as os from 'node:os';

// ───────────────────────────────────────────────────────────────────────────
// Tier 1: minimal — log marker + cleanup
// ───────────────────────────────────────────────────────────────────────────

const CHORUS_LOG = path.join(os.homedir(), '.chorus', 'chorus.log');

/** Path to the per-run hermetic root (from test-acp.sh runner). */
export function runDir(): string {
  const d = process.env.ACP_BDD_RUN_DIR;
  if (!d) throw new Error('ACP_BDD_RUN_DIR not set — must be invoked via test-acp.sh');
  return d;
}

/** Capture chorus.log size before a scenario; new lines after = scenario emissions. */
export function captureLogMarker(): number {
  try {
    return fs.statSync(CHORUS_LOG).size;
  } catch {
    return 0;
  }
}

/** Read chorus.log events emitted AFTER the marker. One JSON object per line. */
export function eventsSince(marker: number): Array<Record<string, unknown>> {
  if (!fs.existsSync(CHORUS_LOG)) return [];
  const stat = fs.statSync(CHORUS_LOG);
  if (stat.size <= marker) return [];
  const fd = fs.openSync(CHORUS_LOG, 'r');
  const buf = Buffer.alloc(stat.size - marker);
  fs.readSync(fd, buf, 0, buf.length, marker);
  fs.closeSync(fd);
  return buf
    .toString('utf8')
    .split('\n')
    .filter((l) => l.trim().length > 0)
    .map((l) => {
      try {
        return JSON.parse(l) as Record<string, unknown>;
      } catch {
        return { _raw: l };
      }
    });
}

/** True if any event since `marker` matches name + card id.
 *  chorus-log CLI writes `card=<id>` which serializes as `"card":"<id>"` —
 *  not `card_id`. Production emitters use both depending on path
 *  (createSpineEmitter wraps with `card_id`; manual chorus-log uses `card`).
 *  Accept either to match real spine shape. */
export function eventLanded(marker: number, event: string, cardId: number): boolean {
  return eventsSince(marker).some(
    (e) =>
      e.event === event &&
      (Number(e.card_id) === cardId || Number(e.card) === cardId),
  );
}

/** All events for the given trace_id since the marker (verifies trace propagation). */
export function eventsForTrace(marker: number, traceId: string): Array<Record<string, unknown>> {
  return eventsSince(marker).filter((e) => e.trace_id === traceId);
}

// ───────────────────────────────────────────────────────────────────────────
// Tier 2: + git — bare-repo origin + role/card branch
// ───────────────────────────────────────────────────────────────────────────

export interface GitFixture {
  /** Path to the bare-repo origin (passed as `origin` remote). */
  originDir: string;
  /** Path to the working clone where chorus_acp will run. */
  cloneDir: string;
  /** Resolved branch name (e.g. "wren/99209"). */
  branch: string;
}

/**
 * Create a hermetic bare-repo origin + working clone, branch off main at
 * <role>/<cardId>. Upstream tracks origin/main (matches chorus-werk repoint
 * convention — important for the fresh-branch regression guard).
 *
 * Each call uses a unique sub-dir under runDir() so scenarios don't collide.
 */
export function newGitFixture(role: 'kade' | 'wren' | 'silas', cardId: number): GitFixture {
  const root = runDir();
  const slug = `${role}-${cardId}-${Date.now()}-${Math.floor(Math.random() * 10_000)}`;
  const originDir = path.join(root, `origin-${slug}`);
  const cloneDir = path.join(root, `clone-${slug}`);
  const branch = `${role}/${cardId}`;

  execFileSync('git', ['init', '-q', '--bare', originDir]);
  execFileSync('git', ['clone', '-q', originDir, cloneDir]);
  execFileSync('git', ['-C', cloneDir, 'config', 'user.email', `${role}@fixture.test`]);
  execFileSync('git', ['-C', cloneDir, 'config', 'user.name', role]);

  // Seed main with one commit.
  fs.writeFileSync(path.join(cloneDir, 'README.md'), `# fixture for ${branch}\n`);
  execFileSync('git', ['-C', cloneDir, 'add', 'README.md']);
  execFileSync('git', ['-C', cloneDir, 'commit', '-q', '-m', 'seed']);

  // Normalize default branch to "main".
  const defaultBranch = execFileSync('git', ['-C', cloneDir, 'symbolic-ref', '--short', 'HEAD'])
    .toString()
    .trim();
  if (defaultBranch !== 'main') {
    execFileSync('git', ['-C', cloneDir, 'branch', '-m', defaultBranch, 'main']);
  }
  execFileSync('git', ['-C', cloneDir, 'push', '-q', '-u', 'origin', 'main']);

  // Create role/card branch, set upstream to origin/main (chorus-werk repoint shape).
  execFileSync('git', ['-C', cloneDir, 'checkout', '-q', '-b', branch]);
  execFileSync('git', ['-C', cloneDir, 'branch', '--set-upstream-to=origin/main', branch]);

  // Stub branch-check.sh so git-queue.sh accepts the role/card branch in fixture.
  const platformScripts = path.join(cloneDir, 'platform', 'scripts');
  fs.mkdirSync(platformScripts, { recursive: true });
  fs.writeFileSync(
    path.join(platformScripts, 'branch-check.sh'),
    'branch_check_match() { return 0; }\nbranch_check_card_match() { return 0; }\n',
  );

  return { originDir, cloneDir, branch };
}

/** Add a commit on the current branch; returns the new SHA. */
export function addCommit(fix: GitFixture, file: string, content: string, message: string): string {
  const fp = path.join(fix.cloneDir, file);
  fs.mkdirSync(path.dirname(fp), { recursive: true });
  fs.writeFileSync(fp, content);
  execFileSync('git', ['-C', fix.cloneDir, 'add', file]);
  execFileSync('git', ['-C', fix.cloneDir, 'commit', '-q', '-m', message]);
  return execFileSync('git', ['-C', fix.cloneDir, 'rev-parse', 'HEAD']).toString().trim();
}

/** Push the current branch to origin (sets up origin/<branch> ref). */
export function pushBranch(fix: GitFixture): void {
  execFileSync('git', ['-C', fix.cloneDir, 'push', '-q', 'origin', fix.branch]);
}

/** Resolve the origin's view of a branch (for assertion after push). */
export function originSha(fix: GitFixture): string {
  return execFileSync('git', ['-C', fix.originDir, 'rev-parse', `refs/heads/${fix.branch}`])
    .toString()
    .trim();
}

/** Resolve the local HEAD SHA on the working clone. */
export function localSha(fix: GitFixture): string {
  return execFileSync('git', ['-C', fix.cloneDir, 'rev-parse', 'HEAD']).toString().trim();
}

/**
 * Inject a pre-commit hook with the given exit code + optional stdout signature.
 * Used by hook-fail / commit-fail refusal scenarios.
 */
export function installPreCommitHook(fix: GitFixture, exitCode: number, signature: string): void {
  const hooksDir = path.join(fix.cloneDir, '.git', 'hooks');
  fs.mkdirSync(hooksDir, { recursive: true });
  const hook = path.join(hooksDir, 'pre-commit');
  fs.writeFileSync(
    hook,
    `#!/usr/bin/env bash\n${signature ? `echo "${signature}"\n` : ''}exit ${exitCode}\n`,
  );
  fs.chmodSync(hook, 0o755);
}

/**
 * Create a divergent commit on origin's copy of the branch that modifies the
 * SAME file as the local with DIFFERENT content. Forces pull-rebase conflict
 * → git emits "could not apply" → push-conflict classification.
 *
 * Used by push-conflict refusal scenario.
 */
export function createConflictingOriginCommit(
  fix: GitFixture,
  file: string,
  localContent: string,
  originContent: string,
): void {
  // Apply local first (becomes the local-only commit).
  fs.writeFileSync(path.join(fix.cloneDir, file), localContent);
  execFileSync('git', ['-C', fix.cloneDir, 'add', file]);
  execFileSync('git', ['-C', fix.cloneDir, 'commit', '-q', '-m', `local: ${file}`]);

  // Spawn a side-clone, push a conflicting version to origin.
  const sideDir = path.join(runDir(), `side-${Date.now()}-${Math.floor(Math.random() * 10_000)}`);
  execFileSync('git', ['clone', '-q', fix.originDir, sideDir]);
  execFileSync('git', ['-C', sideDir, 'config', 'user.email', 'side@fixture.test']);
  execFileSync('git', ['-C', sideDir, 'config', 'user.name', 'side']);
  execFileSync('git', ['-C', sideDir, 'checkout', '-q', '-b', fix.branch]);
  fs.writeFileSync(path.join(sideDir, file), originContent);
  execFileSync('git', ['-C', sideDir, 'add', file]);
  execFileSync('git', ['-C', sideDir, 'commit', '-q', '-m', `side: ${file}`]);
  execFileSync('git', ['-C', sideDir, 'push', '-q', 'origin', fix.branch]);
}

// ───────────────────────────────────────────────────────────────────────────
// Tier 3: + gh stub state + role-state
// ───────────────────────────────────────────────────────────────────────────

/** Reset gh stub state to empty (no PRs, next_pr=1). Called per scenario. */
export function resetGhStubState(): void {
  const stateFile = process.env.GH_STUB_STATE;
  if (!stateFile) throw new Error('GH_STUB_STATE not set — must be invoked via test-acp.sh');
  fs.writeFileSync(stateFile, JSON.stringify({ prs: {}, next_pr: 1 }));
}

/** Inject a gh stub state: PR exists for `head`, optionally already merged. */
export function seedGhStubPr(head: string, opts: { merged?: boolean; number?: number } = {}): number {
  const stateFile = process.env.GH_STUB_STATE;
  if (!stateFile) throw new Error('GH_STUB_STATE not set');
  const state = JSON.parse(fs.readFileSync(stateFile, 'utf8')) as {
    prs: Record<string, { head: string; base: string; title: string; state: string; merged_at: string | null }>;
    next_pr: number;
  };
  const num = opts.number ?? state.next_pr;
  state.prs[String(num)] = {
    head,
    base: 'main',
    title: `fixture PR for ${head}`,
    state: opts.merged ? 'MERGED' : 'OPEN',
    merged_at: opts.merged ? new Date().toISOString() : null,
  };
  if (num >= state.next_pr) state.next_pr = num + 1;
  fs.writeFileSync(stateFile, JSON.stringify(state));
  return num;
}

/** Configure GH_STUB_FAIL env for the next invocation (comma-separated subcmds). */
export function setGhStubFail(failures: Array<'create' | 'merge' | 'view' | 'list' | 'all'>): void {
  process.env.GH_STUB_FAIL = failures.join(',');
}

/** Clear GH_STUB_FAIL after a scenario. */
export function clearGhStubFail(): void {
  delete process.env.GH_STUB_FAIL;
}

/** Place a demo:preflight-pass comment proxy file the demo-evidence gate reads. */
export function seedDemoEvidence(cardId: number, role: 'kade' | 'wren' | 'silas'): void {
  // The actual gate reads card comments via `cards comments`. For fixture mode,
  // step defs override CARDS_BIN to a stub that returns canned comment text
  // including "demo:preflight-pass". The stub itself is owned by the cards
  // CLI fixture (separate from gh stub). For now, this helper records the
  // intent — step def wires the stub.
  const evidence = path.join(runDir(), `demo-evidence-${cardId}.txt`);
  fs.writeFileSync(evidence, `demo:preflight-pass ac=N/N — ${role}\n`);
}

// ───────────────────────────────────────────────────────────────────────────
// Cleanup
// ───────────────────────────────────────────────────────────────────────────

/**
 * Remove a single fixture's working tree. The runner's cleanup hook handles
 * the per-run root dir; this is for tighter scenario-level cleanup when a
 * test wants to reset between Background steps.
 */
export function teardownGitFixture(fix: GitFixture): void {
  for (const dir of [fix.originDir, fix.cloneDir]) {
    try {
      fs.rmSync(dir, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
  }
}
