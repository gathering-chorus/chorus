/**
 * #2652 (AC8) — cards CLI binary location resolver.
 *
 * Single source of truth for "where is the cards bash wrapper on this host?"
 * Mirrors shim-path.ts. The MCP cards tools spawn this script so MCP and bash
 * callers end up running the same canonical CLI.
 *
 * Resolution order:
 *   1. CHORUS_CARDS_BIN env — explicit override (tests, alt builds).
 *   2. CHORUS_ROOT env — same convention as shim path resolution.
 *   3. __dirname-relative absolute path — final fallback when neither env set.
 */
import * as path from 'path';

const CARDS_REL = 'platform/scripts/cards';

export function resolveCardsPath(env: NodeJS.ProcessEnv = process.env): string {
  if (env.CHORUS_CARDS_BIN) return env.CHORUS_CARDS_BIN;
  if (env.CHORUS_ROOT) return path.join(env.CHORUS_ROOT, CARDS_REL);
  // platform/api/src/cards-path.ts → ../../scripts/cards
  return path.resolve(__dirname, '..', '..', 'scripts', 'cards');
}
