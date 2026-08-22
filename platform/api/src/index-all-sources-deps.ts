/* eslint-disable security/detect-non-literal-fs-filename -- internal indexer: reads source files from controlled repo paths, never untrusted input (#3429) */
// Shared dependency wiring for createIndexAllSources (#3085).
//
// Both the chorus-api server and the standalone reindex worker
// (index-worker.ts) need to construct indexAllSources with the SAME real deps —
// in particular the perf-tuned positioned reads readTail (#3067) and readSince
// (#3077). Defining them once here keeps the two callers from drifting
// (chorus:principle-no-competing-implementations).
import fs from 'fs';
import path from 'path';
import os from 'os';
import Database from 'better-sqlite3';
import { createIndexAllSources, type IndexAllSourcesDeps } from './index-all-sources';
import { createAthenaSparqlClient, createSparqlLoader } from './athena-sparql';
import { fusekiWriteAuthFromEnv } from './icd-sparql';
import { fetchLoomPrinciples } from './handlers/loom-principles';
import { fetchLoomPolicies } from './handlers/loom-policies';
import { collectAllDocs } from './handlers/doc-catalog';

export interface IndexAllSourcesEnv {
  dbPath: string;
  repoRoot: string;
}

// #3136 REFINE — graph knowledge fetch (principles/policies/practices from Fuseki).
// principles + policies reuse the existing in-process loom fetch fns (no new query);
// practices has no handler, so a small inline SELECT against the same client — the
// least-surface way to ingest its 40 existing instances (no new ontology, no new
// handler file). Wired here so both the server and the worker get the same path.
const FUSEKI_SPARQL = process.env.ATHENA_SPARQL || 'http://localhost:3030/pods/sparql';
const FUSEKI_UPDATE = process.env.ATHENA_UPDATE || 'http://localhost:3030/pods/update';

const PRACTICES_QUERY = `PREFIX chorus: <https://jeffbridwell.com/chorus#>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT DISTINCT ?practice ?label ?comment WHERE {
  GRAPH <urn:chorus:instances> {
    ?practice a chorus:Practice .
    OPTIONAL { ?practice rdfs:label ?label }
    OPTIONAL { ?practice rdfs:comment ?comment }
  }
}`;

type GraphRow = { source: string; id: string; content: string };

function lines(...parts: Array<string | undefined | false>): string {
  return parts.filter((p): p is string => Boolean(p)).join('\n');
}

// eslint-disable-next-line complexity -- cohesive multi-source SPARQL load (principles/policies/practices each loaded + mapped in one place); splitting scatters the wiring (#3429)
async function fetchGraphKnowledge(repoRoot: string): Promise<GraphRow[]> {
  const athena = createAthenaSparqlClient({ sparqlUrl: FUSEKI_SPARQL, updateUrl: FUSEKI_UPDATE, auth: fusekiWriteAuthFromEnv() });
  const loadQuery = createSparqlLoader({ fs, sparqlDir: path.join(repoRoot, 'platform/api/src/sparql') });
  const rows: GraphRow[] = [];

  // #3749 — principles now come from the generated athena-make surface (one source)
  const pr = (await fetchLoomPrinciples()).body as
    { data?: { principles?: Array<{ id: string; label: string; comment: string; techReading?: string; jeffReading?: string }> } };
  for (const p of pr.data?.principles ?? []) {
    rows.push({ source: 'principle', id: `principle:${p.id}`, content: lines(p.label, p.comment, p.techReading && `tech: ${p.techReading}`, p.jeffReading && `jeff: ${p.jeffReading}`) });
  }

  const po = (await fetchLoomPolicies({ sparql: athena.query, loadQuery })).body as
    { data?: { policies?: Array<{ id: string; label: string; comment: string; surface: string; enforces: Array<{ label: string }> }> } };
  for (const p of po.data?.policies ?? []) {
    rows.push({ source: 'policy', id: `policy:${p.id}`, content: lines(p.label, p.comment, p.surface && `surface: ${p.surface}`, p.enforces.length > 0 && `enforces: ${p.enforces.map((e) => e.label).join(', ')}`) });
  }

  const prac = (await athena.query(PRACTICES_QUERY)) as
    { results?: { bindings?: Array<{ practice?: { value: string }; label?: { value: string }; comment?: { value: string } }> } };
  for (const b of prac.results?.bindings ?? []) {
    const uri = b.practice?.value;
    if (!uri) continue;
    rows.push({ source: 'practice', id: `practice:${uri.split('#').pop()}`, content: lines(b.label?.value, b.comment?.value) });
  }
  return rows;
}

export function buildIndexAllSourcesDeps(env: IndexAllSourcesEnv): IndexAllSourcesDeps {
  return {
    dbPath: env.dbPath,
    DatabaseCtor: Database as unknown as IndexAllSourcesDeps['DatabaseCtor'],
    fs,
    path,
    repoRoot: env.repoRoot,
    homedir: () => os.homedir(),
    // #3067: positioned tail read so indexSpine bounds the 170MB log to its recent
    // tail instead of re-reading the whole file every reindex cycle (4.9s sync block).
    readTail: (p, maxBytes) => {
      const size = fs.statSync(p).size;
      if (size <= maxBytes) return fs.readFileSync(p, 'utf-8');
      const fd = fs.openSync(p, 'r');
      try {
        const buf = Buffer.alloc(maxBytes);
        fs.readSync(fd, buf, 0, maxBytes, size - maxBytes);
        return buf.toString('utf-8');
      } finally {
        fs.closeSync(fd);
      }
    },
    // #3077 AC2: positioned read of only the bytes appended since `offset`. Resets to 0
    // if the log was rotated/truncated (offset > size). O(new bytes), not O(16MB tail).
    readSince: (p, offset) => {
      const size = fs.statSync(p).size;
      const start = offset > size ? 0 : offset;
      if (start >= size) return { content: '', startOffset: start, size };
      const fd = fs.openSync(p, 'r');
      try {
        const len = size - start;
        const buf = Buffer.alloc(len);
        fs.readSync(fd, buf, 0, len, start);
        return { content: buf.toString('utf-8'), startOffset: start, size };
      } finally {
        fs.closeSync(fd);
      }
    },
    // #3136 REFINE — graph knowledge, prefetched once per reindex cycle.
    fetchGraph: () => fetchGraphKnowledge(env.repoRoot),
    // #3695 AC3 — only when a relay is configured for this deployment.
    ...(process.env.BUZZ_RELAY_HOST
      ? { fetchBuzz: () => fetchBuzzEvents(process.env.BUZZ_RELAY_HOST!, process.env.BUZZ_COMPOSE_DIR ?? '~/CascadeProjects/buzz/deploy/compose') }
      : {}),
    // #3136 REFINE — the full doc catalog (all ~402 docs via the SOURCE_DIRS scan).
    listDocs: () => collectAllDocs().map((d) => ({ href: d.href, title: d.title, group: d.group, absPath: d.absPath })),
  };
}


// ── #3695 AC3 — Buzz relay events (our self-hosted Nostr relay on Bedroom) ────
// Transport: ssh → docker compose exec postgres (the cross-machine read pattern;
// read-is-free per ADR-012). Attribution: pubkey → owning chorus:Principal via
// the SAME security-graph projection buzz-allowlist-project uses — one truth.
// Env-gated: BUZZ_RELAY_HOST unset → dep omitted → source absent (no relay, no
// change). Any failure throws; the core marks source=error and the loop survives.
type BuzzRow = { id: string; pubkey: string; author: string; created_at: string; kind: number; channel: string; content: string };

const BUZZ_PRINCIPALS_QUERY = `PREFIX chorus: <https://jeffbridwell.com/chorus#>
SELECT ?p ?k WHERE { GRAPH <urn:chorus:domains:security> {
  ?c a chorus:KeyRegistryEntry ; chorus:keyType "nostr-secp256k1" ;
     chorus:forPrincipal ?p ; chorus:nostrPubkey ?k } }`;

async function fetchBuzzEvents(relayHost: string, composeDir: string): Promise<BuzzRow[]> {
  const { execFile } = await import('node:child_process');
  const { promisify } = await import('node:util');
  const run = promisify(execFile);

  // 1. pubkey → principal map (security graph — the allowlist projection's source)
  const athena = createAthenaSparqlClient({ sparqlUrl: FUSEKI_SPARQL, updateUrl: FUSEKI_UPDATE, auth: fusekiWriteAuthFromEnv() });
  const res = (await athena.query(BUZZ_PRINCIPALS_QUERY)) as
    { results?: { bindings?: Array<{ p?: { value: string }; k?: { value: string } }> } };
  const byPubkey = new Map<string, string>();
  for (const b of res.results?.bindings ?? []) {
    const iri = b.p?.value; const key = b.k?.value;
    if (iri && key) byPubkey.set(key, iri.replace(/.*#principal-/, ''));
  }

  // 2. events from the relay's own Postgres (tab-separated; content base64 so
  //    newlines/tabs inside a message can't break row framing). Schema verified
  //    LIVE 2026-07-25 (not assumed): id/pubkey are BYTEA → hex-encode;
  //    channel_id is a uuid column; deleted_at soft-deletes. base64 is
  //    newline-STRIPPED — pg encode() wraps at 76 chars, which would break
  //    one-row-per-line framing (caught live, not in review). Timestamp is
  //    CONCATENATED (no quoted literals in the to_char format): the ssh remote
  //    shell eats double quotes, and pg then reads DD+"TH" as an ordinal
  //    pattern, corrupting the hour — caught when deep-health flagged
  //    2026-07-25THH24:46:59Z as 495838h stale.
  const sql = "SELECT encode(id,'hex'), encode(pubkey,'hex'), to_char(created_at at time zone 'UTC', 'YYYY-MM-DD') || 'T' || to_char(created_at at time zone 'UTC', 'HH24:MI:SS') || 'Z', kind, coalesce(channel_id::text,'team'), replace(encode(convert_to(content,'UTF8'),'base64'), chr(10), '') FROM events WHERE kind = 9 AND deleted_at IS NULL ORDER BY created_at DESC LIMIT 5000";
  const { stdout } = await run('ssh', ['-o', 'ConnectTimeout=10', relayHost,
    `cd ${composeDir} && /usr/local/bin/docker compose exec -T postgres psql -U buzz -d buzz -tA -F '\t' -c "${sql}"`,
  ], { timeout: 30000, maxBuffer: 32 * 1024 * 1024 });

  const rows: BuzzRow[] = [];
  for (const line of stdout.split('\n')) {
    const parts = line.split('\t');
    if (parts.length < 6 || !/^[0-9a-f]{64}$/.test(parts[0])) continue;
    const [id, pubkey, created_at, kindStr, channel, b64] = parts;
    rows.push({
      id, pubkey,
      author: byPubkey.get(pubkey) ?? '',
      created_at, kind: Number(kindStr) || 0, channel,
      content: Buffer.from(b64, 'base64').toString('utf8'),
    });
  }
  return rows;
}

/** Convenience: a ready-to-run indexAllSources bound to the given env. */
export function makeIndexAllSources(env: IndexAllSourcesEnv): () => Promise<{ indexed: Record<string, string>; elapsed_ms: number }> {
  return createIndexAllSources(buildIndexAllSourcesDeps(env));
}
