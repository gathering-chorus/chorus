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

export interface IndexAllSourcesEnv {
  dbPath: string;
  repoRoot: string;
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
  };
}

/** Convenience: a ready-to-run indexAllSources bound to the given env. */
export function makeIndexAllSources(env: IndexAllSourcesEnv): () => Promise<{ indexed: Record<string, string>; elapsed_ms: number }> {
  return createIndexAllSources(buildIndexAllSourcesDeps(env));
}
