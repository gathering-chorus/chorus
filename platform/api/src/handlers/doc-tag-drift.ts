/* eslint-disable security/detect-object-injection --
 * Levenshtein DP table is keyed by validated string indices; not user input.
 */
/**
 * doc-tag-drift — detect docs claiming subdomains Athena doesn't recognize (#2520 AC6).
 *
 * Pure function shape: takes a list of doc tags + the live Athena subdomain
 * set, returns the drift list. Tests pass a fixture set; production calls
 * Athena's /api/athena/subdomains.
 */

import type { DocTags } from './doc-tagger';

export interface DocWithTags {
  href: string;
  source: string;
  title: string;
  tags: DocTags;
}

export interface DriftEntry {
  href: string;
  title: string;
  claimedSubdomain: string;
  closestMatch?: string;
}

function levenshtein(a: string, b: string): number {
  if (a === b) return 0;
  if (!a.length) return b.length;
  if (!b.length) return a.length;
  const dp: number[][] = Array.from({ length: a.length + 1 }, () => new Array(b.length + 1).fill(0));
  for (let i = 0; i <= a.length; i++) dp[i][0] = i;
  for (let j = 0; j <= b.length; j++) dp[0][j] = j;
  for (let i = 1; i <= a.length; i++) {
    for (let j = 1; j <= b.length; j++) {
      const cost = a[i-1] === b[j-1] ? 0 : 1;
      dp[i][j] = Math.min(dp[i-1][j] + 1, dp[i][j-1] + 1, dp[i-1][j-1] + cost);
    }
  }
  return dp[a.length][b.length];
}

function closestSubdomain(claimed: string, valid: string[]): string | undefined {
  let best: { id: string; dist: number } | undefined;
  for (const v of valid) {
    const d = levenshtein(claimed, v);
    if (!best || d < best.dist) best = { id: v, dist: d };
  }
  // Only suggest if reasonably close (within 1/3 of the length)
  if (best && best.dist <= Math.max(2, Math.floor(claimed.length / 3))) {
    return best.id;
  }
  return undefined;
}

export function detectDrift(docs: DocWithTags[], validSubdomains: string[]): DriftEntry[] {
  const validSet = new Set(validSubdomains);
  const drift: DriftEntry[] = [];
  for (const d of docs) {
    if (!d.tags.subdomain) continue;
    if (validSet.has(d.tags.subdomain)) continue;
    drift.push({
      href: d.href,
      title: d.title,
      claimedSubdomain: d.tags.subdomain,
      closestMatch: closestSubdomain(d.tags.subdomain, validSubdomains),
    });
  }
  return drift;
}
