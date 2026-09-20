/* eslint-disable security/detect-object-injection -- Environment keys and reference names are validated before lookup. */
// eslint-disable-next-line @typescript-eslint/no-explicit-any -- Versioned provider envelopes are validated at each adapter boundary; unknown vendor fields are retained as telemetry.
export type Json = Record<string, any>;
export type Emit = (type: string, data?: Json, session?: string, turn?: string) => void;
export class AdapterError extends Error {
  constructor(public code: string, message: string) { super(message); }
}
export interface Runtime {
  probe(params: Json): Promise<Json>;
  start(params: Json): Promise<Json>;
  resume(params: Json): Promise<Json>;
  send(params: Json): Promise<Json>;
  cancel(params: Json): Promise<Json>;
  stop(params: Json): Promise<Json>;
  status(params: Json): Promise<Json>;
  approve(params: Json): Promise<Json>;
}
export function required(value: unknown, name: string): string {
  if (typeof value !== 'string' || !value.trim()) throw new AdapterError('invalid_request', `${name} is required`);
  return value;
}
export function environment(refs: Json = {}): NodeJS.ProcessEnv {
  const env = { ...process.env };
  for (const [name, reference] of Object.entries(refs)) {
    if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name) || typeof reference !== 'string' || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(reference)) throw new AdapterError('configuration', 'Invalid environment reference');
    if (process.env[reference] === undefined) throw new AdapterError('configuration', `Missing environment reference: ${reference}`);
    env[name] = process.env[reference];
  }
  return env;
}
export function deadline(config: Json = {}): number {
  const timeout = config.timeout_ms ?? 60000;
  if (!Number.isSafeInteger(timeout) || timeout <= 0 || timeout > 3600000) throw new AdapterError('configuration', 'timeout_ms must be between 1 and 3600000');
  return timeout;
}
