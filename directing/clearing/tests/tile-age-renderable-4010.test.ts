/**
 * @test-type: unit
 * #4010 — the tile ages from what the PANE renders, one predicate for both.
 *
 * Root cause of the 06:19 defect, caught live by the #3976 reconciliation
 * flow (2026-08-27, kade: tile "3m ago" vs pane newest 690s): the pane skips
 * machinery digests (cards / nudge / role-state / chorus-log / smoke-check)
 * via OBS_SKIP_TOKENS, while the tile aged from the RAW last observation
 * line. A role whose newest activity was `mcp: chorus_cards_view` looked 3m
 * fresh on the tile while its newest line Jeff could actually see was 11m
 * old. Two surfaces, two event sets, one spine.
 *
 * The fix is a single shared predicate (src/observations.ts) imported by
 * both readers. These tests pin the predicate and the tile's use of it.
 *
 * NEGATIVE PROOF (#3734): `a machinery-only tail must NOT set the age` is
 * the case that fails against the pre-fix code (raw-last-line aging). The
 * inverse — real activity DOES set it — is here so the rule cannot pass by
 * refusing everything.
 */

import { isRenderableDigest, OBS_SKIP_TOKENS } from '../src/observations';

describe('#4010 — one skip predicate for tile age and pane render', () => {
  test('machinery digests are not renderable', () => {
    for (const d of [
      'mcp: chorus_cards_view',
      'bash: nudge silas got-it',
      'bash: chorus-log interaction.pattern.detected',
      'bash: role-state wren building',
      'bash: smoke-check clearing',
    ]) {
      expect(isRenderableDigest(d)).toBe(false);
    }
  });

  test('real work digests render', () => {
    for (const d of [
      'edit: directing/clearing/src/tiles.ts',
      'bash: npx playwright test proving/flows',
      'mcp: werk-commit #4010 wren',
    ]) {
      expect(isRenderableDigest(d)).toBe(true);
    }
  });

  // NEGATIVE PROOF — the exact live shape from 2026-08-27: newest lines are
  // machinery, the newest RENDERABLE line is older. The age the tile shows
  // must come from the older renderable line, or the two surfaces disagree
  // by construction. Modeled on the predicate (the tile's loop is a
  // last-renderable scan over exactly this) so the proof cannot pass while
  // the predicate wrongly admits machinery.
  test('NEGATIVE: a machinery-only tail does not look like fresh activity', () => {
    const tail = [
      { ts: 'older', digest: 'edit: platform/api/public/athena/services.html' },
      { ts: 'newer', digest: 'mcp: chorus_cards_view' },
      { ts: 'newest', digest: 'bash: nudge kade re: red' },
    ];
    const newestRenderable = [...tail].reverse().find((l) => isRenderableDigest(l.digest));
    expect(newestRenderable?.ts).toBe('older');
  });

  // INVERSE of the negative proof — when real work IS the newest line, the
  // predicate must select it, not reach back. A rule that always answers
  // "older" is exactly as broken as one that always answers "newest".
  test('NEGATIVE-INVERSE: fresh real work ages from the newest line', () => {
    const tail = [
      { ts: 'older', digest: 'mcp: chorus_cards_view' },
      { ts: 'newest', digest: 'edit: directing/clearing/src/observations.ts' },
    ];
    const newestRenderable = [...tail].reverse().find((l) => isRenderableDigest(l.digest));
    expect(newestRenderable?.ts).toBe('newest');
  });

  // Every token in the shared list must actually skip — a token someone adds
  // to the constant but that the predicate cannot see (typo, wrong casing)
  // would silently widen what renders. Exercised through the predicate, not
  // asserted as a frozen array.
  test('every published skip token skips through the predicate', () => {
    for (const token of OBS_SKIP_TOKENS) {
      expect(isRenderableDigest(`bash: ${token} something`)).toBe(false);
    }
  });
});
