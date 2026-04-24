/**
 * Shared handler utilities (#2173 AC4).
 *
 * `run()` wraps a throwing function in the uniform `{status, body}` shape.
 * Every handler that today looks like:
 *     try { res.json(someFn()) } catch (e) { res.status(500).json({error: ...}) }
 * collapses to:
 *     const r = await run(() => someFn()); res.status(r.status).json(r.body);
 *
 * Error -> 500 mapping. Non-Error throws stringify. Both sync and async
 * functions are handled by awaiting inside the util.
 */

import type { FetchResult } from './sessions';
import type { Request, Response, NextFunction } from 'express';

export async function run<T>(fn: () => T | Promise<T>): Promise<FetchResult> {
  try {
    const body = await fn();
    return { status: 200, body };
  } catch (e) {
    const message = e instanceof Error ? e.message : String(e);
    return { status: 500, body: { error: message } };
  }
}

/** Wrap an async Express handler so the returned Promise is consumed (and
 *  rejections forwarded to next()). Keeps `@typescript-eslint/no-misused-promises`
 *  happy: the wrapper itself returns void, while still awaiting the async body.
 *  Use as `app.get('/x', asyncRoute(async (req, res) => { ... }))`. #2463 wave 2. */
export function asyncRoute(
  fn: (req: Request, res: Response, next: NextFunction) => Promise<unknown>,
): (req: Request, res: Response, next: NextFunction) => void {
  return (req, res, next) => {
    fn(req, res, next).catch(next);
  };
}
