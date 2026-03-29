# Brief: Guardrail Quick Wins — Husky, Dependabot, CodeQL

**From**: Silas (Architect)
**To**: Kade (Engineer)
**Date**: 2026-02-14
**Priority**: Medium — hardening, not blocking anything
**Context**: Jeff and I audited the full guardrail chain (see `architect/guardrails-and-feedback-loops.md`). Three gaps are trivially fixable — each under 10 minutes. They close holes where we have the tooling but it's not wired up.

---

## 1. Wire Husky Pre-Commit Hook (~10 min)

**The gap**: `npm run precommit` exists and runs security → lint → all tests. But there's no git hook — it only runs if someone remembers to call it. If an agent pushes code without running it, the only safety net is CI (which catches it later, after the commit is already on the branch).

**Fix**:

```bash
npx husky init
```

This creates `.husky/` directory. Then set the pre-commit hook:

```bash
echo "npm run precommit" > .husky/pre-commit
```

**Verify**: Make a trivial change, try to commit. The precommit chain (security → lint → tests) should run and block the commit if anything fails.

**Note**: `package.json` already has `"prepare": "npm run build:all"`. Change it to:

```json
"prepare": "husky && npm run build:all"
```

This ensures Husky installs its hooks on `npm install`.

**Commit the `.husky/` directory** so the hook is shared across all environments.

---

## 2. Add Dependabot (~5 min)

**The gap**: No automated dependency update PRs. Updates are manual — stale dependencies accumulate silently until `npm audit` or Trivy catches a CVE.

**Fix**: Create `.github/dependabot.yml`:

```yaml
version: 2
updates:
  - package-ecosystem: "npm"
    directory: "/"
    schedule:
      interval: "weekly"
      day: "monday"
    open-pull-requests-limit: 10
    reviewers:
      - "WJeffBridwell"
    labels:
      - "dependencies"
    ignore:
      - dependency-name: "@types/*"
        update-types: ["version-update:semver-patch"]

  - package-ecosystem: "docker"
    directory: "/"
    schedule:
      interval: "weekly"
      day: "monday"
    labels:
      - "dependencies"
      - "docker"

  - package-ecosystem: "github-actions"
    directory: "/"
    schedule:
      interval: "monthly"
    labels:
      - "dependencies"
      - "ci"
```

This covers:
- **npm**: Weekly PRs for dependency updates (ignores trivial @types patches)
- **Docker**: Weekly PRs for base image updates
- **GitHub Actions**: Monthly PRs for action version updates

PRs go through the full CI pipeline before merge — the existing gates catch breaking changes.

---

## 3. Make CodeQL Blocking (~1 min)

**The gap**: CodeQL analysis in CI runs with `continue-on-error: true`. Findings don't block the build — they're visible in GitHub's Security tab but a PR with code-level vulnerabilities can still merge.

**Fix**: In `.github/workflows/ci-cd.yml`, find the CodeQL analyze step and remove `continue-on-error`:

```yaml
# Before:
- name: Perform CodeQL Analysis
  uses: github/codeql-action/analyze@v3
  continue-on-error: true

# After:
- name: Perform CodeQL Analysis
  uses: github/codeql-action/analyze@v3
```

That's it. One line removed. CodeQL findings now block the build.

**Risk**: If CodeQL has existing findings that we've been ignoring, this will break the build until they're resolved. Before removing the line:

1. Check the Security tab on GitHub for existing CodeQL alerts
2. If there are open alerts, triage them first (fix or dismiss with reason)
3. Then remove `continue-on-error`

If there are no existing alerts, just remove the line.

---

## Summary

| Fix | Effort | What it closes |
|-----|--------|----------------|
| Husky pre-commit hook | 10 min | Agents can't skip precommit checks |
| Dependabot config | 5 min | Stale dependencies get flagged automatically |
| CodeQL blocking | 1 min | Code-level vulnerabilities block the build |

All three are independent — do in any order.

— Silas
