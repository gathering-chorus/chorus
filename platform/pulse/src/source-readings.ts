// @card: #4563
// @domain: awareness
// Pulse source readings derivation, cache service, and live fetcher.

export type ReadingStatus = 'ok' | 'failed' | 'unmeasured';

export interface SourceReading {
  source: string;
  observedAt: string;
  status: ReadingStatus;
  reason?: string;
  lastWrite?: string;
  asOf?: string;
  lagMinutes?: number;
}

export interface UnmeasuredSource {
  source: string;
  reason: string;
}

export interface ReadingsFactResult {
  derivedAt: string;
  readings: SourceReading[];
  unmeasured: UnmeasuredSource[];
}

export interface RawProbeResult {
  source: string;
  ok: boolean;
  timestamp?: string;
  error?: string;
  lagMinutes?: number;
}

export interface ReadingsFactInput {
  now?: Date;
  probes: RawProbeResult[];
}

const CANONICAL_SOURCES = ['board', 'rooms', 'decisions', 'roles', 'loki'];
const DECISIONS = 'decisions';
const NO_SOURCE_PART_2 = 'no source, part 2';
const CONN_REFUSED = 'connection refused';
const LIVE = 'live';
const FAILED = 'failed';

function formatHHMM(d: Date): string {
  const h = String(d.getHours()).padStart(2, '0');
  const m = String(d.getMinutes()).padStart(2, '0');
  return `${h}:${m}`;
}

function computeAsOf(probe: RawProbeResult, lastWriteDate: Date): { asOf: string; lagMinutes?: number } {
  if (probe.lagMinutes && probe.lagMinutes >= 30) {
    return {
      asOf: `as of ${formatHHMM(lastWriteDate)} (${probe.lagMinutes}m)`,
      lagMinutes: probe.lagMinutes,
    };
  }
  if (probe.timestamp) {
    return { asOf: `as of ${formatHHMM(new Date(probe.timestamp))}` };
  }
  return { asOf: LIVE };
}

function buildReadingForProbe(
  source: string,
  probe: RawProbeResult | undefined,
  nowIso: string
): { reading: SourceReading; unmeasured?: UnmeasuredSource } {
  if (source === DECISIONS) {
    const reason = probe?.error || NO_SOURCE_PART_2;
    return {
      reading: { source, observedAt: nowIso, status: 'unmeasured', reason, lastWrite: nowIso, asOf: FAILED },
      unmeasured: { source, reason },
    };
  }

  if (!probe || !probe.ok) {
    const reason = probe?.error || CONN_REFUSED;
    return {
      reading: { source, observedAt: nowIso, status: 'failed', reason, lastWrite: nowIso, asOf: FAILED },
      unmeasured: { source, reason },
    };
  }

  const lastWrite = probe.timestamp || nowIso;
  const { asOf, lagMinutes } = computeAsOf(probe, new Date(lastWrite));
  const reading: SourceReading = {
    source,
    observedAt: nowIso,
    status: 'ok',
    lastWrite,
    asOf,
    ...(lagMinutes ? { lagMinutes } : {}),
  };
  return { reading };
}

export function deriveReadings(input: ReadingsFactInput): ReadingsFactResult {
  const now = input.now || new Date();
  const nowIso = now.toISOString();

  const probeMap = new Map<string, RawProbeResult>();
  for (const p of input.probes) {
    probeMap.set(p.source, p);
  }

  const allSources = [...CANONICAL_SOURCES];
  for (const p of input.probes) {
    if (!allSources.includes(p.source)) {
      allSources.push(p.source);
    }
  }

  const readings: SourceReading[] = [];
  const unmeasured: UnmeasuredSource[] = [];

  for (const source of allSources) {
    const { reading, unmeasured: u } = buildReadingForProbe(source, probeMap.get(source), nowIso);
    readings.push(reading);
    if (u) {
      unmeasured.push(u);
    }
  }

  return { derivedAt: nowIso, readings, unmeasured };
}

export type ReadingsFetcher = () => Promise<ReadingsFactInput>;

export class ReadingsService {
  private cache: ReadingsFactResult = {
    derivedAt: new Date().toISOString(),
    readings: [],
    unmeasured: [],
  };
  private timer?: NodeJS.Timeout;
  private readonly pollIntervalMs: number;
  private readonly fetcher: ReadingsFetcher;

  constructor(fetcher: ReadingsFetcher, pollIntervalMs = 15000) {
    this.fetcher = fetcher;
    this.pollIntervalMs = pollIntervalMs;
  }

  async refresh(): Promise<ReadingsFactResult> {
    try {
      const input = await this.fetcher();
      this.cache = deriveReadings(input);
      return this.cache;
    } catch (err: unknown) {
      const reason = err instanceof Error ? err.message : String(err);
      this.cache = {
        derivedAt: new Date().toISOString(),
        readings: [],
        unmeasured: [{ source: 'pulse:readings', reason }],
      };
      return this.cache;
    }
  }

  getReadings(): ReadingsFactResult {
    return this.cache;
  }

  start(): void {
    if (this.timer) return;
    void this.refresh();
    this.timer = setInterval(() => {
      void this.refresh();
    }, this.pollIntervalMs);
    this.timer.unref();
  }

  stop(): void {
    if (this.timer) {
      clearInterval(this.timer);
      this.timer = undefined;
    }
  }
}

export interface ReadingsLiveConfig {
  boardUrl?: string;
  rolesUrl?: string;
  lokiUrl?: string;
  buzzRelayUrl?: string;
  timeoutMs?: number;
}

async function probeHttpUrl(url: string, timeoutMs: number): Promise<boolean> {
  try {
    const res = await fetch(url, { signal: AbortSignal.timeout(timeoutMs) });
    return res.ok;
  } catch {
    return false;
  }
}

async function probeBoardLive(boardUrl: string, timeoutMs: number): Promise<RawProbeResult> {
  try {
    const res = await fetch(`${boardUrl}/wip`, { signal: AbortSignal.timeout(timeoutMs) });
    if (!res.ok) return { source: 'board', ok: false, error: `HTTP ${res.status}` };
    const json = (await res.json()) as { timestamp?: string; data?: { timestamp?: string } };
    const ts = json.timestamp || json.data?.timestamp;
    return { source: 'board', ok: true, timestamp: ts };
  } catch (err: unknown) {
    const msg = err instanceof Error ? err.message : CONN_REFUSED;
    return { source: 'board', ok: false, error: msg };
  }
}

export function buildLiveReadingsFetcher(config: ReadingsLiveConfig = {}): ReadingsFetcher {
  const boardUrl = config.boardUrl || process.env.BOARD_URL || 'http://localhost:3340/api/chorus/context/board';
  const rolesUrl = config.rolesUrl || process.env.ROLES_URL || 'http://localhost:3360/v1/roles/roles';
  const lokiUrl = config.lokiUrl || process.env.LOKI_URL || 'http://localhost:3102';
  const buzzRelayUrl = config.buzzRelayUrl || process.env.BUZZ_RELAY_URL || '';
  const timeoutMs = config.timeoutMs || 1500;

  return async (): Promise<ReadingsFactInput> => {
    const boardPromise = probeBoardLive(boardUrl, timeoutMs);
    const rolesPromise = probeHttpUrl(rolesUrl, timeoutMs).then((ok) => ({
      source: 'roles',
      ok,
      error: ok ? undefined : 'door unreadable',
    }));
    const lokiPromise = probeHttpUrl(`${lokiUrl}/ready`, timeoutMs).then((ok) => ({
      source: 'loki',
      ok,
      error: ok ? undefined : CONN_REFUSED,
    }));
    const roomsProbe: RawProbeResult = buzzRelayUrl
      ? { source: 'rooms', ok: true }
      : { source: 'rooms', ok: false, error: 'relay not configured' };
    const decisionsProbe: RawProbeResult = {
      source: DECISIONS,
      ok: false,
      error: NO_SOURCE_PART_2,
    };

    const [board, roles, loki] = await Promise.all([boardPromise, rolesPromise, lokiPromise]);
    return {
      now: new Date(),
      probes: [board, roomsProbe, decisionsProbe, roles, loki],
    };
  };
}
