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
  justification: string | null;
}

// The declared vocabulary. A token outside this set is not a declaration — it's
// prose or fixture noise (e.g. a doc comment's "@test-type: X" placeholder).
const VALID_TYPES = new Set(['unit', 'integration', 'api', 'ui', 'perf', 'security', 'bdd', 'e2e']);

// A declaration is honored ONLY in the file's LEADING comment block — a comment
// line that begins with @test-type, before any real code. This defeats the
// declaration-in-data false positive: a `// @test-type: api` string buried in a
// test fixture (or a prose mention mid-file) is NOT the file's declaration. Once
// real code starts, we stop looking. Combined with vocab validation, neither a
// fixture string nor a prose placeholder can masquerade as a declaration.
// Full declaration: the type plus any inline justification (the text after the
// type — used to justify a declared-lighter-than-signal override, eslint-disable
// style). Returns null when there is no valid header declaration.
export function parseDeclarationInfo(content: string): Declaration | null {
  for (const raw of content.split('\n')) {
    const line = raw.trim();
    if (line === '') continue;
    const m = line.match(/^(?:\/\/|#|\*)\s*@test-type:\s*([a-z0-9-]+)\s*(.*)$/i);
    if (m) {
      const t = m[1].toLowerCase();
      if (!VALID_TYPES.has(t)) return null;
      const rest = m[2].replace(/^[\s—–-]+/, '').trim();
      return { type: t, justification: rest.length > 0 ? rest : null };
    }
    // still inside the leading comment block? keep scanning; else stop.
    if (line.startsWith('//') || line.startsWith('#') || line.startsWith('/*') || line.startsWith('*')) continue;
    return null;
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
