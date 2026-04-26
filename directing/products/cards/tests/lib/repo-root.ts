// #2504 — repoRoot() helper for tests in directing/products/cards/.
// Mirrors #2505's chorus_root() pattern (Rust). Tests must not hardcode
// /Users/jeffbridwell/CascadeProjects/chorus paths — they don't resolve
// on Linux CI runners. CHORUS_ROOT env wins; otherwise climb from this
// file's location up to the repo root.
import * as path from 'node:path';

export function repoRoot(): string {
  const env = process.env.CHORUS_ROOT;
  if (env) return env;
  // tests/lib/repo-root.ts → tests/lib → tests → cards → products → directing → repo
  return path.resolve(__dirname, '..', '..', '..', '..', '..');
}
