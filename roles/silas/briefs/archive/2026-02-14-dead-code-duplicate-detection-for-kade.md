# Brief: Dead Code & Duplicate Detection Tooling

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-14
**Priority**: Medium — code hygiene, not blocking anything
**Context**: Jeff asked about scanning for dead and duplicate code. The E2E Sprint 2 work surfaced redundant auth checks in 14 handler methods — exactly the kind of thing these tools catch before it becomes a problem. We have partial tooling already (ESLint catches unused locals, legacy JSCPD reports exist). This brief fills the gaps.

---

## Deliverable 1: Install knip (dead code / unused exports / unused dependencies)

**What**: [knip](https://knip.dev/) — single tool that finds unused files, unused exports, unused dependencies, and unlisted dependencies across the whole TypeScript project.

**Install**:
```bash
npm install -D knip
```

**Add to package.json scripts**:
```json
{
  "scripts": {
    "knip": "knip",
    "knip:fix": "knip --fix"
  }
}
```

**Config**: knip usually works zero-config for TypeScript projects. If it needs tuning, create `knip.json`:
```json
{
  "entry": ["src/app.ts"],
  "project": ["src/**/*.ts"],
  "ignore": ["src/**/*.test.ts", "src/**/*.spec.ts"],
  "ignoreDependencies": ["@types/*"]
}
```

**Run it, report findings**. Don't auto-fix everything — some "unused" exports may be used dynamically (e.g., Express middleware registered by convention). Use judgment:
- Unused files → safe to remove if truly unreferenced
- Unused exports → safe to remove if no dynamic imports
- Unused dependencies → safe to remove from package.json
- Unlisted dependencies → add to package.json

**Do NOT add to precommit hook yet.** Run it manually first, clean up what's safe, then we'll decide whether to gate on it.

---

## Deliverable 2: Reactivate jscpd (duplicate code detection)

**What**: [jscpd](https://github.com/kucherenko/jscpd) — copy/paste detector. Legacy reports already exist at `/jscpd-report/`. Reactivate it with an npm script.

**Check if installed**:
```bash
npx jscpd --version
```

If not installed:
```bash
npm install -D jscpd
```

**Add to package.json scripts**:
```json
{
  "scripts": {
    "jscpd": "jscpd src/ --min-tokens 50 --reporters console --format typescript"
  }
}
```

**Config** — create `.jscpd.json` if one doesn't exist:
```json
{
  "threshold": 5,
  "reporters": ["console", "html"],
  "output": "jscpd-report",
  "ignore": [
    "node_modules",
    "dist",
    "coverage",
    "**/*.test.ts",
    "**/*.spec.ts",
    "e2e/**"
  ],
  "minTokens": 50,
  "minLines": 5,
  "format": ["typescript", "javascript"]
}
```

- `minTokens: 50` and `minLines: 5` — catches meaningful duplication, not trivial matches
- `threshold: 5` — percentage threshold; above 5% total duplication triggers a warning
- Ignore test files — test duplication is often intentional (similar test patterns)

**Run it, report findings.** The HTML report at `jscpd-report/` is useful for reviewing what's duplicated. Focus on:
- Handler methods with similar patterns (the auth check duplication Kade already fixed is the archetype)
- Copy-pasted SPARQL queries or Turtle manipulation code
- Repeated middleware setup patterns

---

## Deliverable 3: Enable TypeScript unused checks

**What**: Turn on `noUnusedLocals` and `noUnusedParameters` in `tsconfig.json`. These are TypeScript compiler flags that flag unused variables and parameters at build time — a second layer behind ESLint's `@typescript-eslint/no-unused-vars`.

**Edit tsconfig.json** — add to compilerOptions:
```json
{
  "compilerOptions": {
    "noUnusedLocals": true,
    "noUnusedParameters": true
  }
}
```

**Expect some noise.** Common patterns that trigger false positives:
- Express middleware signatures: `(req, res, next)` where `next` isn't used → prefix with underscore: `_next`
- Destructured values where you only use some fields → use `_` prefix

Run `npm run build` after enabling, fix what breaks. Standard approach: prefix intentionally unused params with `_`.

---

## Sequencing

| Deliverable | Effort | Dependencies |
|-------------|--------|--------------|
| knip | ~30 min (install + first run + triage findings) | Nothing |
| jscpd | ~20 min (reactivate + first run) | Nothing |
| tsconfig unused checks | ~30 min (enable + fix build errors) | Nothing |

All three are independent — do in any order.

## What to report back

After running both tools:
1. **knip findings**: How many unused files, exports, dependencies? List the significant ones.
2. **jscpd findings**: Total duplication percentage. Top 5 largest duplicate blocks (file + line range).
3. **tsconfig build**: How many unused local/param errors surfaced? All fixed?

This gives Jeff and me a baseline. If the numbers are low, great — the codebase is clean. If they're high, we prioritize cleanup.

— Silas
