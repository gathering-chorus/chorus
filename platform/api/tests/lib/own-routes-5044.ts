/**
 * #5044 — this tree's own answers for the two routes the card changes, for the browser suites.
 *
 * The browser suites (proving/flows/athena-domain-*-5034.spec.cjs) open this tree's pages against
 * the LIVE chorus-api, which runs main. A changed route is therefore not in front of them before
 * the land (the named limit in proving/flows/lib/athena-page-5034.cjs). #5041's rule for standing
 * in a server change: it must be the server's real answer, or not exist. So this is not a fixture:
 * it runs THIS tree's handlers, the same functions the routes call, and serves what they answer.
 *
 *   GET /pipeline?domain=<d>   fetchChorusDomainPipeline; its sibling reads go to the live API
 *                              (&fail=<text>: a sibling read whose address holds the text fails)
 *   GET /counts?domain=<d>     fetchDomainConformance; the lint is this tree's policy-validate
 *   GET /gaps?domain=<d>       #5045 fetchDomainGaps; &cards=<n,n> states the numbers the row holds,
 *                              &store=down fails the store (see the route below)
 *   GET /athena/api-docs/<domain>/<collection>/…  #5042 AC5: the API page and its viewer files, by
 *                              this tree's handler (the document is the live door's)
 *   GET /settled?domain=<d>    { settled: true } when the check running for the domain (if any) has
 *                              ended; { settled: false } after 45 s if it has not (ask again)
 *   GET /control?…             a test's hands on the clock and the lint (below); answers the state
 *
 * /control (each key optional):
 *   reset=1      forget every kept result and failure, clock back to now, lint back to real
 *   clock=<ms>   move the handlers' clock forward by this much from the real time
 *   lint=real    run policy-validate (one real run is shared by every worker for ten minutes)
 *   lint=hold    a check starts and does not end until release=1
 *   lint=empty   a check ends having measured nothing
 *   lint=down    the policies door does not answer
 *   release=1    end every held check with the lint's output
 *   quick=1      until reset, the lint is not run: every policy handed to it is answered PASS at
 *                once. For the tests about waiting, staleness and failure, where what the lint
 *                says does not matter and the real one (minutes, beside the other suites) would
 *                only be waited on. The suites that read verdicts leave this off.
 *
 * Started by the suites with tsx, on a port the system picks; it prints "READY <port>".
 * Wiring kept the same as platform/api/src/server.ts policyCountsDeps (the rows from the door, the
 * plans from this tree, the environment policy-validate is given); a difference there is a fault here.
 */
import fs from 'fs';
import http from 'http';
import os from 'os';
import path from 'path';
import { execFile } from 'child_process';
import { fetchChorusDomainPipeline } from '../../src/handlers/chorus-domain-pipeline';
import {
  fetchDomainConformance, domainCheckSettled, clearPolicyCountsCache, type PolicyRow, type PolicyCountsDeps,
} from '../../src/handlers/policy-counts';
import { fetchDomainGaps, type GapsDeps } from '../../src/handlers/domain-gaps';
import express from 'express';
import { API_DOCS_MOUNT, apiDocsViewer } from '../../src/handlers/api-docs-viewer';
import type { BoardCard } from '../../src/handlers/domain-cards';

const LIVE = (process.env.ATHENA_URL || 'http://localhost:3340').replace(/\/+$/, '');
// #5042 — before the land the live principles door serves no OpenAPI document. The pipeline's sibling
// reads meet the same stand-in the page meets: the one in proving/flows/lib/athena-page-5034.cjs
// (asLanded for the discovery, landedDocument for the document; both off in the nightly).
const landed = require(path.resolve(__dirname, '..', '..', '..', '..', 'proving/flows/lib/athena-page-5034.cjs')) as {
  asLanded: (body: unknown) => unknown; landedDocument: (pathname: string) => string | null;
};
async function liveRead(rel: string): Promise<unknown> {
  const doc = landed.landedDocument(rel);
  if (doc !== null) return JSON.parse(doc);
  const resp = await fetch(`${LIVE}${rel}`, { signal: AbortSignal.timeout(30_000) });
  if (!resp.ok) return null;
  const body: unknown = await resp.json();
  return rel === '/owl/' ? landed.asLanded(body) : body;
}
const ROOT = path.resolve(__dirname, '..', '..', '..', '..');
// one real lint for every worker OF THIS ACCOUNT (#5042: a role's run read EACCES on the file another
// role's account had written in a /tmp both share; the lint is per account, so its file is too)
const SHARED = path.join(os.tmpdir(), `own-routes-5044-lint-${os.userInfo().username}`);

let clockAdd = 0;
let lint: 'real' | 'hold' | 'empty' | 'down' = 'real';
let checks = 0;
let quick = false;
let held: Array<() => void> = [];

const sleep = (ms: number): Promise<void> => new Promise((r) => setTimeout(r, ms));

function runValidate(rows: PolicyRow[], args: string[]): Promise<string> {
  return new Promise((resolve) => {
    const tmp = path.join(os.tmpdir(), `own-routes-5044-${process.pid}-${Date.now()}.json`);
    fs.writeFileSync(tmp, JSON.stringify({ data: rows }));
    execFile(path.join(ROOT, 'platform', 'scripts', 'policy-validate'), args, {
      cwd: ROOT, timeout: 300_000, maxBuffer: 4 * 1024 * 1024,
      env: { ...process.env, PATH: `${path.dirname(process.execPath)}:${process.env.PATH ?? ''}`, POLICIES_JSON: tmp, CHORUS_HOME: ROOT, CHORUS_API: LIVE },
    }, (_err, stdout) => { fs.rmSync(tmp, { force: true }); resolve(stdout); });
  });
}

/** The real lint's output for a domain: run once, shared across workers through a file, ten minutes. */
async function realLint(domain: string, rows: PolicyRow[], args: string[]): Promise<string> {
  const file = `${SHARED}-${domain}.txt`;
  const lock = `${file}.lock`;
  const fresh = (): string | null => {
    try { return Date.now() - fs.statSync(file).mtimeMs < 10 * 60 * 1000 ? fs.readFileSync(file, 'utf8') : null; } catch { return null; }
  };
  // The lock is a file holding its owner's pid. A test process that ends takes this server down
  // with it, mid-lint or not, so a lock can outlive its owner; an owner that is no longer running
  // is not waited on. (2026-11-25 18:50: a test's wait on a check passed 300 s; the first cut
  // waited six minutes on a left-over lock. A slow lint alone is not ruled out.)
  const alive = (pid: number): boolean => { try { process.kill(pid, 0); return true; } catch { return false; } };
  let mine = false;
  for (let i = 0; i < 200 && !mine; i++) {
    const kept = fresh();
    if (kept) return kept;
    try { fs.writeFileSync(lock, String(process.pid), { flag: 'wx' }); mine = true; } catch {
      let owner = NaN;
      try { owner = Number(fs.readFileSync(lock, 'utf8')); } catch { /* released between the two looks */ }
      if (Number.isInteger(owner) && !alive(owner)) { try { fs.unlinkSync(lock); } catch { /* another waiter cleared it */ } } else await sleep(2000);
    }
  }
  // still not ours after the wait: run anyway, without touching the other owner's lock
  try {
    const out = await runValidate(rows, args);
    if (out.trim()) fs.writeFileSync(file, out);
    return out;
  } finally { if (mine) { try { fs.unlinkSync(lock); } catch { /* cleared by a waiter that judged us gone */ } } }
}

function deps(): PolicyCountsDeps {
  return {
    now: () => Date.now() + clockAdd,
    fetchPolicies: async () => {
      if (lint === 'down') throw new Error('ECONNREFUSED (own-routes: lint=down)');
      const resp = await fetch(`${LIVE}/owl/policies/policies?limit=1000`, { signal: AbortSignal.timeout(15_000) });
      if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
      return ((await resp.json()) as { data?: PolicyRow[] }).data ?? [];
    },
    loadPlans: () => {
      const dir = path.join(ROOT, 'designing', 'domains', 'policies');
      try {
        return fs.readdirSync(dir).filter((f) => /^policy-checks-\d+\.json$/.test(f))
          .flatMap((f) => JSON.parse(fs.readFileSync(path.join(dir, f), 'utf8')) as PolicyRow[]);
      } catch { return []; }
    },
    runnable: (row) => {
      const m = /platform\/scripts\/([\w.-]+)/.exec(row.checkSpec ?? '');
      return !m || fs.existsSync(path.join(ROOT, 'platform', 'scripts', m[1]));
    },
    runValidate: async (domain, rows, args = []) => {
      checks++;
      if (lint === 'empty') return '';
      if (lint === 'hold') await new Promise<void>((r) => { held.push(r); });
      if (quick) return rows.map((r) => `PASS ${r.name} 0 <= ${r.baseline ?? 0}`).join('\n');
      return realLint(domain, rows, args);
    },
  };
}

const json = (res: http.ServerResponse, status: number, body: unknown): void => {
  res.writeHead(status, { 'content-type': 'application/json', 'cache-control': 'no-store' });
  res.end(JSON.stringify(body));
};

// #5042 AC5 — this tree's API page (the handler the route calls), its document read from the live door
const viewer = express();
viewer.use(`${API_DOCS_MOUNT}/:domain/:collection`, ...apiDocsViewer(async (owlPath) => {
  const r = await fetch(`${LIVE}/owl${owlPath}`, { signal: AbortSignal.timeout(30_000) });
  return { status: r.status, text: await r.text() };
}));

const server = http.createServer(async (req, res) => {
  try {
    const url = new URL(req.url ?? '/', 'http://own');
    if (url.pathname.startsWith(API_DOCS_MOUNT + '/')) return viewer(req, res);
    const domain = url.searchParams.get('domain') ?? '';
    if (url.pathname === '/pipeline') {
      // fail=<text>: a sibling read whose address holds the text does not answer (a test's negative proof)
      const fail = url.searchParams.get('fail');
      const r = await fetchChorusDomainPipeline({
        fetcher: async (rel) => {
          if (fail && rel.includes(fail)) return null;
          try { return await liveRead(rel); } catch { return null; }
        },
        envelope: (queryName, data, durationMs, extra) => ({ _meta: { source: 'athena', query_name: queryName, duration_ms: durationMs, ...extra }, data }),
      }, domain);
      return json(res, r.status, r.body);
    }
    if (url.pathname === '/gaps') {
      // #5045 — fetchDomainGaps, this tree's. The board is the live one, asked card by card. The
      // store is the live one too, unless cards=<n,n> names the numbers the Domain row holds: the
      // rows that carry chorus:gapCard are written after the model deploy, so before the land no
      // domain has any, and a test states the row it means. store=down: the store does not answer.
      const planted = url.searchParams.get('cards');
      const down = url.searchParams.get('store') === 'down';
      const sparql: GapsDeps['sparql'] = async (query) => {
        if (down) throw new Error('ECONNREFUSED (own-routes: store=down)');
        if (planted !== null) return { results: { bindings: planted.split(',').filter(Boolean).map((n) => ({ n: { value: n } })) } };
        const resp = await fetch(`${LIVE}/sparql-read`, { method: 'POST', headers: { 'content-type': 'application/sparql-query', accept: 'application/sparql-results+json' }, body: query, signal: AbortSignal.timeout(20_000) });
        if (!resp.ok) throw new Error(`HTTP ${resp.status}`);
        return (await resp.json()) as Awaited<ReturnType<GapsDeps['sparql']>>;
      };
      const numbers = (await sparql('').catch(() => ({ results: { bindings: [] } }))).results.bindings.map((b) => b.n.value);
      const board: BoardCard[] = [];
      for (const n of planted !== null ? numbers : []) {
        const resp = await fetch(`${LIVE}/api/chorus/card-story/${encodeURIComponent(n)}`, { signal: AbortSignal.timeout(20_000) });
        if (!resp.ok) continue;   // the board does not hold it: the handler says so
        const c = (await resp.json()) as { card?: number; title?: string; owner?: string; status?: string };
        if (c.title) board.push({ id: String(c.card ?? n), title: c.title, owner: c.owner ?? '', status: c.status ?? '', priority: '', tags: '' });
      }
      const r = await fetchDomainGaps({ sparql, getCards: () => board }, domain);
      return json(res, r.status, r.body);
    }
    if (url.pathname === '/counts') { const r = await fetchDomainConformance(domain, deps()); return json(res, r.status, r.body); }
    if (url.pathname === '/settled') {
      const settled = await Promise.race([domainCheckSettled(domain).then(() => true), sleep(45_000).then(() => false)]);
      return json(res, 200, { settled });
    }
    if (url.pathname === '/control') {
      const q = url.searchParams;
      if (q.get('reset')) { for (const r of held) r(); held = []; clearPolicyCountsCache(); clockAdd = 0; lint = 'real'; checks = 0; quick = false; }
      if (q.get('quick')) quick = true;
      if (q.get('clock') !== null) clockAdd = Number(q.get('clock'));
      const mode = q.get('lint');
      if (mode === 'real' || mode === 'hold' || mode === 'empty' || mode === 'down') lint = mode;
      if (q.get('release')) { for (const r of held) r(); held = []; }
      return json(res, 200, { clockAdd, lint, checks, quick, held: held.length });
    }
    return json(res, 404, { error: `own-routes-5044 has no ${url.pathname}` });
  } catch (e) { return json(res, 500, { error: String((e as Error).message ?? e) }); }
});

server.listen(0, '127.0.0.1', () => {
  process.stdout.write(`READY ${(server.address() as { port: number }).port}\n`);
});
