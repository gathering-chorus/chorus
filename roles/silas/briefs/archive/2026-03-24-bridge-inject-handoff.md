# Bridge → Claude inject — handoff

**Problem:** When Jeff sends a message from Bridge (phone or desktop), it should land instantly in the target role's Claude session. Currently it queues silently and only delivers when the role's next hook fires.

**What works:**
- Role-to-role nudges via `nudge.sh` from within a Claude session — these work because the nudge binary can find the claude PID and its TTY
- The queue + drain path — messages queue to `/tmp/voice-inbox/{role}/pending-inject.txt` and drain on next UserPromptSubmit hook

**What doesn't work:**
- Bridge calling `nudge.sh` via Node `execSync` — the nudge binary says "no active session" because `find_role_pid` can't match the claude PID from Bridge's process context
- Direct osascript `do script` — runs a shell command in the Terminal tab, doesn't type into Claude's input
- `keystroke` — requires Terminal to be frontmost, changes focus

**Root cause:** The nudge binary's `find_role_pid` scans `ps` for processes named `claude`, then uses `lsof` to match CWD. From Bridge's Node process, this detection fails (returns "no active session" even though the claude process exists with a TTY).

**Files:**
- `chorus/bridge/src/server.ts` lines 440-470 — Bridge delivery code
- `messages/services/chorus-hooks/src/process.rs` — `find_role_pid`, `get_tty`, `inject_to_role`
- `messages/services/chorus-hooks/src/nudge.rs` — nudge delivery flow

**What I tried:**
- Setting DEPLOY_ROLE env var
- Direct osascript matching Terminal tab name
- keystroke inject
