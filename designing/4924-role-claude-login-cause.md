# Cause Analysis: #4924 Role Claude Login Keychain Prompts and 401 Expiration

## 1. Measured Mechanics

1. **Mach Session Inheritance**:
   When Jeff runs `chorus-principal login <role>` (or `/login`), the process runs via `sudo -n -u chorus-<role> -H bash -c "..."` inside Jeff's GUI Terminal/tmux session. The process inherits the Mach bootstrap namespace of Jeff's Aqua session (`gui/501`).
   - Measured: When run from an Aqua GUI session context, `security list-keychains -d user` points to `"/Users/jeffbridwell/Library/Keychains/login.keychain-db"`.

2. **The 5–6 GUI Prompts**:
   Claude Code's storage engine (`Kn()` -> `O(R, b)`) uses `/usr/bin/security` as its primary storage (`R`). During sign-in it executes 5–6 separate `security` invocations:
   - `show-keychain-info`
   - Probe writability: `add-generic-password`
   - Probe cleanup: `delete-generic-password`
   - Search existing key: `find-generic-password -a <user> -s "Claude Code"`
   - Search credentials: `find-generic-password -a <user> -s "Claude Code-credentials"`
   - Store OAuth credentials: `security -i add-generic-password -U -a <user> -s "Claude Code-credentials"`
   - Store device key: `security -i add-generic-password -U -a <user> -s "Claude Code"`
   Because the target keychain belongs to `jeffbridwell` (UID 501) while the process runs as `chorus-<role>` (UID 502/503/504), macOS `securityd` raises the Aqua GUI authorization dialog: `security wants to use the "login" keychain`.

3. **The 2-Second Timeout (`S = 2000`)**:
   In Claude Code (`chunk-at388cvq.js`), `security -i` has a hardcoded timeout of 2,000 ms:
   `u = await sS("security", ["-i"], { input: s, stdio: ["pipe", "pipe", "pipe"], reject: false, timeout: S });`
   When Jeff cancels or is prompted, it takes >2 seconds. `u.timedOut` evaluates to `true`.

4. **Fallback Skipped on Transient Timeout**:
   Claude Code's storage adapter (`O(e, r).update`) evaluates:
   `if (u.transient) return c("secure_storage_credentials_write", ... "primary_transient_skip_fallback"), u;`
   Because `u.timedOut` is `true`, `u.transient` is `true`. Claude Code **explicitly skips writing the plaintext fallback file (`~/.claude/.credentials.json`)**!

5. **Token Loss and the 8-Hour Crash**:
   Because neither Keychain nor `.credentials.json` is written, credentials only live in process RAM. Anthropic's OAuth access token has an 8-hour lifetime. When it expires, Claude Code has no refresh token on disk to refresh it, dying with:
   `API Error: 401 OAuth access token has expired. Please run /login`.

---

## 2. Why Wren's Account Differs from Kade & Silas

1. **Wren's Account State**:
   - In Wren's detached session, `security list-keychains` shows NO keychain (`list-keychains` is empty; Wren has no `~/Library/Keychains` and no `~/Library/Preferences/com.apple.security.plist`).
   - When Claude Code invokes `/usr/bin/security add-generic-password`, `security` fails instantly (<10ms) with `no default keychain` / `SecKeychainSearchCopyNext`.
   - Because it fails immediately without prompting or hanging, `u.timedOut = false` and `u.transient = false`.
   - Claude Code immediately takes the fallback branch: `primary_failed_fallback_success`, saving `~/.claude/.credentials.json` (mode `0600`) with the OAuth refresh token. Wren's `.credentials.json` was written at 11:50. (Silas's was written at 11:14 under a similar detached condition).
2. **Kade & Silas Interactive Login**:
   - When Jeff initiates `chorus-principal login kade` or `silas` from an active Terminal attached to Aqua, the process inherits Jeff's Mach Aqua session port.
   - `securityd` routes credential storage to Jeff's `login.keychain-db`, raising the SecurityAgent GUI prompt 5-6 times.
   - Waiting for Jeff to cancel exceeds 2,000ms -> `timedOut = true` -> fallback skipped -> credentials never persist to disk -> 401 mid-session.

---

## 3. Two Options for Silas to Pick

### Option A: Session Start Isolation (Start all 5 like Wren)
- **Mechanism**: Isolate every role's tmux pane/session from Jeff's Aqua Mach bootstrap session at launch (e.g. via `launchctl bsexec` into the background/per-user bootstrap domain, or starting the role's tmux server in an unattached daemon session detached from `gui/501`).
- **Effect**: Role accounts have no Aqua bootstrap connection and no default keychain. `security list-keychains` is empty for all roles.
- **Behavior**: `/usr/bin/security` fails immediately (<10ms) without GUI dialogs; Claude Code natively writes `~/.claude/.credentials.json` (mode 0600) with the refresh token on every sign-in.
- **Pros**: Cleanest OS-level boundary; matches how background services run; eliminates all cross-account Mach leakage.
- **Cons**: Requires adjusting how `chorus-principal` spawns tmux or panes.

### Option B: Fail-Fast Keychain / Role Environment Bypass
- **Mechanism**: In `platform/scripts/chorus-env-setup.sh` / `own_account_inner_cmd`, ensure that for role accounts, `security` generic-password commands fail fast without touching Jeff's keychain (or set an explicit empty keychain / wrapper that exits non-zero instantly without waiting for GUI prompts).
- **Effect**: When Claude Code probes or stores to the keychain, it gets an immediate non-zero exit in <5ms.
- **Behavior**: Claude Code treats the keychain failure as permanent (not transient), does not time out, and natively writes `~/.claude/.credentials.json` (mode 0600) with the refresh token.
- **Pros**: Simple, localized change; doesn't alter launchd/tmux architecture.
- **Cons**: Relies on environment/shim rather than clean OS session boundaries.
