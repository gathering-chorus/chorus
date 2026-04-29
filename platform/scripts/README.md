# platform/scripts/ — Chorus shell scripts

Operational scripts for the Chorus team substrate. Most are sourced by LaunchAgents (`~/Library/LaunchAgents/com.chorus.*.plist`) or invoked directly by roles.

## Environment setup — `chorus-env-setup.sh` (#2571)

Every shell script that needs `CHORUS_ROOT` or `CHORUS_ROLE` sources this file as line 1:

```bash
#!/bin/bash
set -euo pipefail
source "$(dirname "${BASH_SOURCE[0]}")/chorus-env-setup.sh"

# ... rest of script can use $CHORUS_ROOT, $CHORUS_ROLE
```

The setup self-locates from `BASH_SOURCE[0]` and exports the canonical values. It is authoritative — it overrides any pre-existing `CHORUS_ROOT` in the calling env, so per-worktree runs resolve correctly even when an outer shell has the wrong value cached.

**Why this exists:** the previous pattern `CHORUS_ROOT="${CHORUS_ROOT:-/Users/jeffbridwell/CascadeProjects/chorus}"` (introduced in #1917 across 30 scripts) was a managed-runtime-dep — it carried a hardcoded Mac path as silent fallback, breaking under per-role worktrees and any non-shell-rc-inheriting context (cron, ssh non-login, LaunchAgent without `EnvironmentVariables` block). #2571 retires the fallback in favor of source-from-substrate. Same defensive-substitution family as #2505 (Rust `chorus_root()` fail-loud `expect()`) and #2563 (`env!()` macro). See memory `feedback_eliminate_runtime_dep_dont_manage_it` for the team pattern.

**LaunchAgent pattern:** plists that invoke a chorus script via `/bin/bash` no longer need an `EnvironmentVariables` block — the script self-bootstraps:

```xml
<key>ProgramArguments</key>
<array>
    <string>/bin/bash</string>
    <string>/path/to/chorus/platform/scripts/your-script.sh</string>
</array>
```

If the LaunchAgent invokes a non-bash interpreter (Python, Node), wrap with `/bin/bash -c 'source ... && exec ...'`:

```xml
<key>ProgramArguments</key>
<array>
    <string>/bin/bash</string>
    <string>-c</string>
    <string>source /path/to/chorus/platform/scripts/chorus-env-setup.sh && exec python3 /path/to/script.py</string>
</array>
```

## Two-script split: env-setup vs role-env

Two adjacent scripts handle env. Distinct jobs:

- **`platform/scripts/chorus-env-setup.sh`** — script-time, one-shot. Sourced by every chorus shell script as line 1. Authoritative for `CHORUS_ROOT`. Sets `CHORUS_ROLE` if cwd is in a `roles/<name>/` dir.
- **`platform/shell/chorus-role-env.sh`** — shell-rc-time. Sourced by `~/.zshrc`. Installs a zsh `chpwd` hook (and bash `PROMPT_COMMAND` fallback) that re-fires the role assignment when the user `cd`s between role dirs interactively. Sources `chorus-env-setup.sh` for the initial assignment so the case-on-PWD logic isn't duplicated.

Use env-setup when writing a script. Don't touch role-env unless you're changing how interactive shells track role drift.

## Tests

`tests/chorus-env-setup.bats` — contract tests for the setup script. Run with `bats platform/scripts/tests/chorus-env-setup.bats` from chorus root.
