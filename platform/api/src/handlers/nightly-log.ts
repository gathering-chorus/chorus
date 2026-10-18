// #4579 reopen — the nightly runs, read from the run log the runner writes.
//
// Jeff 10-18 19:10 ("yes"): stop carrying per-run test results in the graph;
// /nightly reads the log. #4156 had moved the page onto TestSuiteRun rows, one
// per suite per run (21,097 rows on 10-18). On 10-18 the graph still showed the
// 03:00 run unfinished at 287 suites while the log had ended it at 06:08:56
// with suites=504: the log is the one place the runner writes the whole run.
//
// The log is append-only and large (100 MB since 08-03), so only its tail is
// read: enough bytes to hold the last `limit` runs. A log that cannot be read
// is null and the route refuses; it never reports an empty green.
import { promises as fsp } from 'fs';
import { parseAllRuns, type NightlyRunRecord } from './nightly-readout';
import type { InFlightUnit } from './nightly-report';

export const NIGHTLY_LOG = (): string =>
  process.env.NIGHTLY_LOG || `${process.env.HOME}/Library/Logs/Chorus/nightly-suites.log`;

/** The last `limit` runs in `text`, oldest first. Pure. */
export function tailRuns(text: string, limit: number): NightlyRunRecord[] {
  const runs = parseAllRuns(text);
  return runs.slice(Math.max(0, runs.length - limit));
}

/** Units a run started and has not ended (SUITESTART with no SUITEEND), as of
 *  `nowMs`. The log stamps both in epoch ms; a start carries the unit's timeout
 *  (`timeout=<s>s`) so a hung unit reads WEDGED (#4007). Pure. */
export function inFlightFromLog(block: string[], nowMs: number): InFlightUnit[] {
  const open = new Map<string, { ms: number; timeoutMs?: number }>();
  for (const l of block) {
    const [tag, unit, ms, extra] = l.split('|');
    if (tag === 'SUITESTART' && unit) {
      const t = /^timeout=(\d+)s$/.exec(extra ?? '');
      open.set(unit, { ms: Number(ms), ...(t ? { timeoutMs: Number(t[1]) * 1000 } : {}) });
    } else if (tag === 'SUITEEND' && unit) open.delete(unit);
  }
  return [...open].map(([unit, { ms, timeoutMs }]) => ({
    unit,
    elapsedMs: Math.max(0, nowMs - (Number.isFinite(ms) ? ms : nowMs)),
    ...(timeoutMs !== undefined ? { timeoutMs } : {}),
  }));
}

/** Read from the end until the text holds `limit` run starts or the whole file. */
async function readTail(file: string, limit: number): Promise<string> {
  const fh = await fsp.open(file, 'r');
  try {
    const size = (await fh.stat()).size;
    let want = Math.min(size, 4 * 1024 * 1024);
    for (;;) {
      const buf = Buffer.alloc(want);
      await fh.read(buf, 0, want, size - want);
      let text = buf.toString('utf8');
      if (want < size) text = text.slice(text.indexOf('\n') + 1); // drop the cut first line
      const starts = text.split('\n').filter((l) => l.startsWith('RUN|start|')).length;
      if (starts > limit || want === size) return text;
      want = Math.min(size, want * 2);
    }
  } finally {
    await fh.close();
  }
}

/** The last `limit` runs from the log, oldest first; null when the log cannot be
 *  read. The newest unfinished run carries what is in flight and how long the
 *  log has been quiet (its own last write, #4009). */
export async function loadRunsFromLog(file = NIGHTLY_LOG(), limit = 14, nowMs = Date.now()): Promise<NightlyRunRecord[] | null> {
  let text: string;
  let mtimeMs: number;
  try {
    mtimeMs = (await fsp.stat(file)).mtimeMs;
    text = await readTail(file, limit);
  } catch {
    return null;
  }
  const runs = tailRuns(text, limit);
  const newest = runs[runs.length - 1];
  if (newest && !newest.completed && !newest.stoppedAt) {
    const lines = text.split('\n');
    const at = lines.findIndex((l) => l === `RUN|start|${newest.startedAt}` || l.startsWith(`RUN|start|${newest.startedAt}|`));
    newest.inFlight = inFlightFromLog(at === -1 ? [] : lines.slice(at), nowMs);
    newest.quietForMs = Math.max(0, nowMs - mtimeMs);
    newest.lastRowAt = new Date(mtimeMs).toISOString();
  }
  return runs;
}
