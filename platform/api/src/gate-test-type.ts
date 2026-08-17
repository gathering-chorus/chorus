// #3442 — test-type declaration gate.
//
// Enforcement layer on top of the tagger. A test must DECLARE its type and the
// declaration must not contradict the content signals. This is what stops the
// drift the audit found: a *-unit.test.ts touching real fs declaring "unit"
// is blocked, because its signals say integration.
//
// Declaration form: `@test-type: <type>` in any comment (// or #), so .ts/.rs
// and .bats/.sh all express it the same way.
//
// RULE (navigated with Silas — total-order determinism):
//   - No declaration            → blocked.
//   - signalled !== 'unit' and declared !== signalled → blocked (contradiction).
//     The tagger's total order yields exactly one true type; the declaration
//     must match it. Declaring lower than the real signal IS the drift.
//   - signalled === 'unit' (no real content signal) → the content tagger has no
//     opinion to contradict; the declaration stands (covers path-based bdd/e2e,
//     which carry no content signal — handled as a pre-layer, not here).
import { tagTestType, type TestType } from './tag-test-type';

export interface GateResult {
  ok: boolean;
  declared: string | null;
  signalled: TestType;
  reason?: string;
  override?: boolean; // declared != signalled, but justified (signal is fixture-data)
}

export interface Declaration {
  type: string;
  /** #3912 — the second axis (Silas ruling 2026-08-16): concern = subject
   *  (api/ui/perf/security), orthogonal to layer = cost. */
  concern: string | null;
  justification: string | null;
}

// The declared vocabulary. A token outside this set is not a declaration — it's
// prose or fixture noise (e.g. a doc comment's "@test-type: X" placeholder).
// #3912 two-axes grammar: `layer[:concern]`. A concern ALONE is refused —
// the merged enum rebuilt the ambiguity (a file can be unit AND security).
const VALID_LAYERS = new Set(['unit', 'integration', 'bdd', 'e2e', 'contract', 'fitness', 'smoke']);
const VALID_CONCERNS = new Set(['api', 'ui', 'perf', 'security']);

// A declaration is honored ONLY in the file's LEADING comment block — a comment
// line that begins with @test-type, before any real code. This defeats the
// declaration-in-data false positive: a `// @test-type: api` string buried in a
// test fixture (or a prose mention mid-file) is NOT the file's declaration. Once
// real code starts, we stop looking. Combined with vocab validation, neither a
// fixture string nor a prose placeholder can masquerade as a declaration.
// Full declaration: the type plus any inline justification (the text after the
// type — used to justify a declared-lighter-than-signal override, eslint-disable
// style). Returns null when there is no valid header declaration.
// #3484: comment-prefix check extracted from parseDeclarationInfo — the inline
// 4-way || was the bulk of that function's cognitive complexity (over baseline).
const COMMENT_PREFIXES = ['//', '#', '/*', '*'];
function isCommentLine(line: string): boolean {
  return COMMENT_PREFIXES.some((p) => line.startsWith(p));
}

export function parseDeclarationInfo(content: string): Declaration | null {
  for (const raw of content.split('\n')) {
    const line = raw.trim();
    if (line === '') continue;
    const m = line.match(/^(?:\/\/|#|\*)\s*@test-type:\s*([a-z0-9-]+)(?::([a-z0-9-]+))?\s*(.*)$/i);
    if (!m) {
      // still inside the leading comment block? keep scanning; else stop.
      if (isCommentLine(line)) continue;
      return null;
    }
    const t = m[1].toLowerCase();
    if (!VALID_LAYERS.has(t)) return null;
    const concern = m[2] ? m[2].toLowerCase() : null;
    if (concern !== null && !VALID_CONCERNS.has(concern)) return null;
    const rest = m[3].replace(/^[\s—–-]+/, '').trim();
    return { type: t, concern, justification: rest.length > 0 ? rest : null };
  }
  return null;
}

export function parseDeclaration(content: string): string | null {
  return parseDeclarationInfo(content)?.type ?? null;
}

export function gateTestType(content: string, _relPath: string): GateResult {
  const signalled = tagTestType(content, _relPath);
  const decl = parseDeclarationInfo(content);

  if (!decl) {
    return { ok: false, declared: null, signalled, reason: 'no @test-type declaration' };
  }
  const declared = decl.type;
  // #3912 — a signalled CONCERN (api/ui/perf/security) is satisfied by the
  // concern axis; the layer axis no longer has to impersonate it.
  if (decl.concern !== null && decl.concern === signalled) {
    return { ok: true, declared, signalled };
  }
  if (signalled !== 'unit' && declared !== signalled) {
    // declared != signalled. The signal may be a fixture-data false positive
    // (a test ABOUT a security domain carrying its words as data). Allow the
    // override ONLY with a justification — no SILENT under-claim. The reason is
    // required but not validated (eslint-disable-with-reason model).
    if (decl.justification) {
      return { ok: true, declared, signalled, override: true };
    }
    return {
      ok: false,
      declared,
      signalled,
      reason: `declared "${declared}" but signals say "${signalled}" — if the signal is fixture-data, justify the override: \`@test-type: ${declared} — <why>\``,
    };
  }
  return { ok: true, declared, signalled };
}
