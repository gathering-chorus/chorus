/**
 * #4405 reopen 4 — the sensors on /crawler-validate. Jeff, 2026-10-12: "make
 * sure our runner is properly aligned integrated and visible as a sensor".
 *
 * Each Sensor row (heralds domain) and its SensorRun rows come from the door.
 * One line per sensor: its latest run, when, against what, what it changed.
 * A sensor that has not looked within its own window reads stale, and a run
 * left at "running" past that window is stale too (a killed pass never
 * closes its row). Nothing here is green unless a run said green in time.
 */

type Row = Record<string, unknown>;

export interface SensorState {
  sensor: string;
  owner: string;
  windowMinutes: number;
  state: 'green' | 'red' | 'running' | 'stale' | 'never';
  run: string;
  target: string;
  startedAt: string;
  endedAt: string;
  create: number;
  update: number;
  delete: number;
  unchanged: number;
}

export type SensorsPanel = { measured: true; rows: SensorState[] } | { measured: false; reason: string };

const str = (v: unknown): string => (typeof v === 'string' ? v : Array.isArray(v) ? str(v[0]) : '');
const num = (v: unknown): number => {
  const n = typeof v === 'number' ? v : Number.parseInt(str(v), 10);
  return Number.isFinite(n) ? n : 0;
};
const local = (v: unknown, prefix: string): string => { const t = str(v); return t.startsWith(prefix) ? t.slice(prefix.length) : t; };

/** The collection the door serves for a kind, from its own discovery list. */
export function collectionOf(primitives: Array<{ kind?: string; collection?: string }>, kind: string): string | null {
  return primitives.find((p) => p.kind === kind)?.collection ?? null;
}

export function sensorStates(sensors: Row[], runs: Row[], nowMs: number): SensorState[] {
  return sensors.map((s): SensorState => {
    const name = str(s.name);
    const windowMinutes = num(s.sensorWindowMinutes) || 60;
    const mine = runs.filter((r) => local(r.runOfSensor, 'sensor-') === name)
      .sort((a, b) => str(b.startedAt).localeCompare(str(a.startedAt)));
    const base = { sensor: name, owner: local(s.ownedBy, 'principal-'), windowMinutes };
    if (mine.length === 0) {
      return { ...base, state: 'never' as const, run: '', target: '', startedAt: '', endedAt: '', create: 0, update: 0, delete: 0, unchanged: 0 };
    }
    const last = mine[0];
    const outcome = str(last.sensorOutcome);
    const seen = Date.parse(str(last.sensorEndedAt) || str(last.startedAt));
    const overdue = !Number.isFinite(seen) || nowMs - seen > windowMinutes * 60_000;
    const state: SensorState['state'] = outcome === 'red' ? 'red'
      : overdue ? 'stale'
        : outcome === 'green' ? 'green'
          : outcome === 'running' ? 'running' : 'stale';
    return {
      ...base, state, run: str(last.name), target: str(last.runTarget),
      startedAt: str(last.startedAt), endedAt: str(last.sensorEndedAt),
      create: num(last.plannedCreate), update: num(last.plannedUpdate), delete: num(last.plannedDelete), unchanged: num(last.plannedUnchanged),
    };
  }).sort((a, b) => a.sensor.localeCompare(b.sensor));
}

function esc(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');
}

const PILL: Record<SensorState['state'], string> = {
  green: '<span class="pill green">green</span>',
  red: '<span class="pill red">red</span>',
  running: '<span class="pill amber">running</span>',
  stale: '<span class="pill red">stale</span>',
  never: '<span class="pill red">never ran</span>',
};

function every(minutes: number): string {
  return minutes % 1440 === 0 ? `${minutes / 1440} day${minutes === 1440 ? '' : 's'}` : minutes % 60 === 0 ? `${minutes / 60} h` : `${minutes} min`;
}

export function renderSensorsSection(panel: SensorsPanel): string {
  const head = '<h2>Sensors</h2><p class="sub">Each sensor is one pass of the herald core, run through the <code>herald_run</code> tool as its owner. Its latest run, against what, and what it changed. A sensor that has not looked within its window reads stale.</p>';
  if (!panel.measured) {
    return `${head}<p class="banner red">UNMEASURED — ${esc(panel.reason)}. No sensor is shown as green.</p>`;
  }
  if (panel.rows.length === 0) {
    return `${head}<p class="banner red">No Sensor rows yet. Each sensor writes its own row the first time it runs.</p>`;
  }
  const rows = panel.rows.map((r) =>
    `<tr><td>${esc(r.sensor)}</td><td>${esc(r.owner)}</td><td>${PILL[r.state]}</td>`
    + `<td>${esc(r.endedAt || r.startedAt) || '—'}</td><td>${esc(r.target) || '—'}</td>`
    + `<td class="n">${r.run ? `${r.create} / ${r.update} / ${r.delete} / ${r.unchanged}` : '—'}</td>`
    + `<td>${esc(every(r.windowMinutes))}</td></tr>`).join('');
  return `${head}<table class="sensors"><thead><tr><th>sensor</th><th>owner</th><th>last run</th><th>at (UTC)</th><th>target</th><th>create / update / delete / same</th><th>every</th></tr></thead><tbody>${rows}</tbody></table>`;
}

/** Read the sensors from the door. Any failure is UNMEASURED, never empty. */
export async function loadSensors(base: string, nowMs: number, fetchFn: typeof fetch = fetch): Promise<SensorsPanel> {
  try {
    const get = async (p: string): Promise<Row> => {
      const r = await fetchFn(`${base}${p}`, { signal: AbortSignal.timeout(8000) });
      if (!r.ok) throw new Error(`${p} answered ${r.status}`);
      return (await r.json()) as Row;
    };
    const prims = ((await get('/')).primitives ?? []) as Array<{ kind?: string; collection?: string }>;
    const sc = collectionOf(prims, 'Sensor');
    const rc = collectionOf(prims, 'SensorRun');
    if (!sc || !rc) return { measured: false, reason: 'the door does not serve Sensor and SensorRun yet' };
    const all = async (c: string): Promise<Row[]> => {
      const out: Row[] = [];
      let next: string | null = `${c}?limit=500`;
      for (let i = 0; next && i < 20; i++) {
        const page = await get(next);
        out.push(...(Array.isArray(page.data) ? (page.data as Row[]) : []));
        const n = (page.links as Row | undefined)?.next;
        next = typeof n === 'string' && n ? n : null;
      }
      return out;
    };
    const [listed, runs] = await Promise.all([all(sc), all(rc)]);
    // #4405 reopen 5 — the door serves a row's owner only on the row itself
    // (links.ownedBy, "chorus:principal-silas"); the list leaves it out.
    const sensors = await Promise.all(listed.map(async (s) => {
      const one = await get(`${sc}/${encodeURIComponent(str(s.name))}`);
      const owner = str((one.links as Row | undefined)?.ownedBy);
      return { ...s, ownedBy: owner.startsWith('chorus:') ? owner.slice('chorus:'.length) : owner };
    }));
    return { measured: true, rows: sensorStates(sensors, runs, nowMs) };
  } catch (e) {
    return { measured: false, reason: `the door did not answer (${(e as Error).message})` };
  }
}
