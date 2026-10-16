// @domain: messages

/** A failed read: `status` is set when the source answered with a non-2xx; otherwise `message` is the thrown error's text (may be empty). */
export interface FetchFailure {
  ok: false;
  status?: number;
  message: string;
}

export type Fetched<T> = { ok: true; value: T } | FetchFailure;

export function errorMessage(err: unknown): string {
  if (typeof err === 'object' && err !== null && 'message' in err && typeof err.message === 'string') {
    return err.message;
  }
  return '';
}

/** Runs `fn` with an AbortSignal that fires after `ms`; the timer is always cleared. */
export async function withTimeout<T>(ms: number, fn: (signal: AbortSignal) => Promise<T>): Promise<T> {
  const controller = new AbortController();
  const timer = setTimeout(() => controller.abort(), ms);
  try {
    return await fn(controller.signal);
  } finally {
    clearTimeout(timer);
  }
}

export function fetchWithTimeout(url: string, ms: number): Promise<Response> {
  return withTimeout(ms, (signal) => fetch(url, { signal }));
}

/** GET `url`; ok only on a 2xx. The body is not read. */
export async function probeUrl(url: string, ms: number): Promise<Fetched<number>> {
  try {
    const res = await fetchWithTimeout(url, ms);
    if (!res.ok) return { ok: false, status: res.status, message: '' };
    return { ok: true, value: res.status };
  } catch (err) {
    return { ok: false, message: errorMessage(err) };
  }
}

/** Reads a JSON body from a Response; a parse failure is a FetchFailure with the parser's message. */
export async function readJson<T>(res: Response): Promise<Fetched<T>> {
  if (!res.ok) return { ok: false, status: res.status, message: '' };
  try {
    return { ok: true, value: (await res.json()) as T };
  } catch (err) {
    return { ok: false, message: errorMessage(err) };
  }
}

/** GET `url` and parse JSON; ok only on a 2xx with a parseable body. */
export async function fetchJson<T>(url: string, ms: number): Promise<Fetched<T>> {
  let res: Response;
  try {
    res = await fetchWithTimeout(url, ms);
  } catch (err) {
    return { ok: false, message: errorMessage(err) };
  }
  return readJson<T>(res);
}

/** `status N` for a non-2xx, else the error message, else `fallback`. */
export function failureReason(f: FetchFailure, fallback: string): string {
  if (f.status !== undefined) return `status ${f.status}`;
  return f.message || fallback;
}
