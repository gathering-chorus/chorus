// #2754 — semantic-recall floor.
//
// Set to 1 (effectively no filter) so short messages from any role enter the
// embedding pass. Pre-#2754 this was 100, which silently erased Jeff's
// short-imperative communication style ("yes", "do it", "go") from semantic
// recall — ~133K of 905K messages skipped, including most of Jeff's input.
//
// Single source of truth: imported by server.ts (embedDelta wiring) and
// health-cache.ts (unembedded-count metric). Do not duplicate the value
// inline — that's the original bug.
//
// If Ollama call rate becomes a perf problem, raise this deliberately and
// land a card naming the trade-off. Do not silently floor short content.
export const MIN_EMBED_LENGTH = 1;
