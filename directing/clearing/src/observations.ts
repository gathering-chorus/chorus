/**
 * #4010 — ONE predicate for "does this observation reach the room?".
 *
 * The 06:19 defect, root-caused at last: the streams pane (server.ts
 * parseObservation) SKIPS machinery digests — nudge/chorus-log/role-state/
 * cards/smoke-check — so they never render. The tile (tiles.ts
 * applyLastObservation) aged from the RAW last observation line, skip or not.
 * A role whose newest activity was `mcp: chorus_cards_view` showed a 3m tile
 * age while the pane's newest renderable line for it was 11m old: two surfaces
 * reading two event sets from one spine, which is exactly what the #3976
 * reconciliation flow refuses (Jeff, 2026-08-22: "if u check the role state
 * and streams they MUST match at any given time").
 *
 * The filter itself is right — Jeff should not read `cards view` noise in the
 * room. What was wrong is that it lived in ONE of the two readers. Both now
 * import this predicate, so they cannot drift apart again without a diff that
 * says so.
 */
export const OBS_SKIP_TOKENS = ['nudge', 'chorus-log', 'role-state', 'cards', 'smoke-check'];

export function isRenderableDigest(digest: string): boolean {
  return !OBS_SKIP_TOKENS.some((t) => digest.includes(t));
}
