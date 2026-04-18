/**
 * GET /api/chorus/seed-media/:filename (extracted #2189).
 *
 * Validates filename (alphanumeric + dot/dash/underscore only), checks the file
 * exists under the seed-media directory. If ok, returns a filePath for the
 * adapter to send; otherwise returns a structured error body.
 *
 * Keeps res.sendFile out of the handler so the handler is testable without
 * Express and the adapter stays thin.
 */
import * as pathMod from 'path';

export interface SeedMediaDeps {
  exists?: (p: string) => boolean;
  baseDir: string;
  joinPath?: (...parts: string[]) => string;
}

export type SeedMediaResult =
  | { status: 200; filePath: string }
  | { status: 400 | 404; body: { error: string } };

const SAFE_RE = /^[a-zA-Z0-9._-]+$/;

export function resolveSeedMedia(
  filename: string,
  { exists = () => true, baseDir, joinPath = pathMod.join }: SeedMediaDeps,
): SeedMediaResult {
  if (!SAFE_RE.test(filename)) {
    return { status: 400, body: { error: 'Invalid filename' } };
  }
  const filePath = joinPath(baseDir, filename);
  if (!exists(filePath)) {
    return { status: 404, body: { error: 'Media not found' } };
  }
  return { status: 200, filePath };
}
