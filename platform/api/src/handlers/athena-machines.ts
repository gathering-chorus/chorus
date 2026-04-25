/**
 * GET /api/athena/machines — Machines with services running on them (#2187).
 *
 * SPARQL returns one row per (machine, service) pair. Handler dedupes on
 * machine URI and accumulates services. Machines with no services still
 * produce a record (empty services[]).
 */
import type { FetchResult } from './codebase-topology';

export interface SparqlMachineBinding {
  machine: { value: string };
  label?: { value: string };
  ip?: { value: string };
  role?: { value: string };
  service?: { value: string };
  serviceLabel?: { value: string };
}

export interface SparqlMachinesResult {
  results: { bindings: SparqlMachineBinding[] };
}

export interface AthenaMachinesDeps {
  sparql: (query: string) => Promise<SparqlMachinesResult>;
  loadQuery: (name: string) => string;
  now?: () => number;
  envelope?: (name: string, data: unknown, durationMs: number, extra?: Record<string, unknown>) => unknown;
}

interface ServiceEntry {
  uri: string;
  label: string;
}

interface MachineEntry {
  uri: string;
  label: string;
  ip: string | null;
  role: string | null;
  services: ServiceEntry[];
}

function defaultEnvelope(name: string, data: unknown, durationMs: number, extra: Record<string, unknown> = {}) {
  return {
    _meta: { source: 'athena', query_name: name, duration_ms: durationMs, ...extra },
    data,
  };
}

function fallbackLabel(uri: string): string {
  const hashIdx = uri.lastIndexOf('#');
  return hashIdx === -1 ? uri : uri.slice(hashIdx + 1);
}

type MachineBinding = {
  machine: { value: string };
  label?: { value: string };
  ip?: { value: string };
  role?: { value: string };
  service?: { value: string };
  serviceLabel?: { value: string };
};

function ensureMachine(map: Map<string, MachineEntry>, b: MachineBinding): MachineEntry {
  const uri = b.machine.value;
  let entry = map.get(uri);
  if (!entry) {
    entry = {
      uri,
      label: b.label?.value ?? fallbackLabel(uri),
      ip: b.ip?.value ?? null,
      role: b.role?.value ?? null,
      services: [],
    };
    map.set(uri, entry);
  }
  return entry;
}

function appendService(entry: MachineEntry, b: MachineBinding): void {
  if (!b.service) return;
  entry.services.push({
    uri: b.service.value,
    label: b.serviceLabel?.value ?? fallbackLabel(b.service.value),
  });
}

function buildMachineList(bindings: MachineBinding[]): MachineEntry[] {
  const map = new Map<string, MachineEntry>();
  for (const b of bindings) {
    const entry = ensureMachine(map, b);
    appendService(entry, b);
  }
  return Array.from(map.values());
}

export async function fetchAthenaMachines(deps: AthenaMachinesDeps): Promise<FetchResult> {
  const now = deps.now ?? Date.now;
  const envelope = deps.envelope ?? defaultEnvelope;
  const start = now();
  try {
    const result = await deps.sparql(deps.loadQuery('machines'));
    const machines = buildMachineList(result.results.bindings as MachineBinding[]);
    return {
      status: 200,
      body: envelope('machines', machines, now() - start, { count: machines.length }),
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      status: 500,
      body: envelope('machines', { error: message }, now() - start, { error: true }),
    };
  }
}
