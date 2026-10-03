// login_steps.ts — #4367. The steps of features/login.feature. Every step drives
// the real chorus-principal binary inside the fixture world of
// lib/login-harness.bash (stub tmux, claude, token minter and curl, all writing
// under one scratch dir per scenario); nothing here reaches a live service.
// A step a card has not built yet returns 'pending' naming that card; the
// scenario carries @waiting-<card> and reads RED by name in every report.
import { Before, After, Given, When, Then } from '@cucumber/cucumber';
import { execFileSync } from 'child_process';
import * as fs from 'fs';
import * as os from 'os';
import * as path from 'path';

const ROOT = path.resolve(__dirname, '../../../..');
const HARNESS = path.join(ROOT, 'platform/tests/lib/login-harness.bash');
const BIN = process.env.CHORUS_PRINCIPAL_TEST_BIN
  || path.join(ROOT, 'platform/services/chorus-principal/target/release/chorus-principal');
const STEP = { timeout: 15_000 };   // #4409: one budget; every step drives a stubbed world in seconds

let T = '';
// #4409: the journey steps read THIS scenario's world from here. They used to take
// the newest login-feature-* dir in the shared temp dir, which is another process's
// world whenever login.feature runs at the same time (the nightly runs suites in parallel).
export function loginWorld(): string { return T; }

Before({ tags: '@login' }, function () {
  T = fs.mkdtempSync(path.join(os.tmpdir(), 'login-feature-'));
});
After({ tags: '@login' }, function () {
  if (T) fs.rmSync(T, { recursive: true, force: true });
});

// Run a bash snippet in the fixture world. `run <cmd…>` keeps output and
// status in $T/out and $T/status (like bats `run`); any other failing command
// fails the step with the world's output attached.
function sh(script: string, stdin = ''): string {
  const pre = [
    'set -eu', `source "${HARNESS}"`, 'login_harness_env',
    '[ -f "$T/env.extra" ] && source "$T/env.extra"',
    'run() { set +e; "$@" > "$T/out" 2>&1; echo $? > "$T/status"; set -e; }',
    'out_has() { grep -qF -- "$1" "$T/out"; }',
    `SCRIPT="${BIN}"`,
  ].join('\n');
  try {
    return execFileSync('bash', ['-c', `${pre}\n${script}`], {
      env: { ...process.env, T, ROOT }, input: stdin, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'],
    });
  } catch (e: any) {
    const out = fs.existsSync(path.join(T, 'out')) ? fs.readFileSync(path.join(T, 'out'), 'utf8') : '';
    throw new Error(`step check failed:\n${script}\n--- stderr ---\n${e.stderr || ''}\n--- last command output ---\n${out}`, { cause: e });
  }
}
// a turn's hook: its exit (2 = the turn was refused) is kept in $T/turn.status
const seen = (role: string, payload: object) =>
  sh(`set +e; AWAKE_SEEN_SYNC=1 "$SCRIPT" seen ${role} > "$T/turn.out" 2>&1; echo $? > "$T/turn.status"`, JSON.stringify(payload));
// cucumber reads only the literal 'pending'; the card is named by the scenario's
// @waiting-<card> tag, and the report carries it (werk-test CUKE_FLATTEN_JS)
const waiting = (_card: number): 'pending' => 'pending';

// ---------------------------------------------------------------- Given

Given(/^a fixture world with the principals jeff \(person\), wren, silas and kade \(agents\), and bridge \(service\)$/, STEP, function () {
  if (!fs.existsSync(BIN)) throw new Error(`chorus-principal is not built at ${BIN}`);
  sh('login_harness_stubs');
});

Given('{word} is logged out', STEP, function (role: string) {
  sh(`test ! -f "$T/identity/${role}/login.json"`);
});

Given('{word} is logged in', STEP, function (role: string) {
  sh(`run "$SCRIPT" login ${role}; test "$(cat "$T/status")" -eq 0; test -f "$T/identity/${role}/session.row.json"`);
});

Given("wren's login token has been renewed", STEP, function () {
  sh('later=$(( $(date +%s) + 3000 )); mk_token wren "$later"; rm -f "$T/identity/wren/seen.at"\n' +
     'python3 -c \'import sys,time;print(time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime(int(sys.argv[1]))))\' "$later" > "$T/want-expiry"');
});

Given("wren's token names kade", STEP, function () {
  sh('python3 -c \'import json,sys;print(json.load(open(sys.argv[1]))["expiresAt"])\' "$T/identity/wren/session.row.json" > "$T/want-expiry"\n' +
     'mk_token kade $(( $(date +%s) + 3000 )); cp "$T/token-kade.fixture" "$T/token-wren.fixture"; rm -f "$T/identity/wren/seen.at"');
});

Given("wren's presence was checked more than ten minutes ago and the focus has not changed", STEP, function () {
  sh(`python3 - "$T/identity/wren/presence.row.json" <<'PY'
import json,sys; d=json.load(open(sys.argv[1])); d["focusedNow"]="false"; d["checkedAt"]="2026-01-01T00:00:00Z"; json.dump(d,open(sys.argv[1],"w"))
PY
rm -f "$T/identity/wren/seen.at"`);
});

Given('wren is logged in and talking', STEP, function () {
  // a live pane that has spoken, with its login on file for that pid
  sh(`running wren 56344
printf '{"state":"recorded","session":"wren-x-1","pid":56344}' > "$T/identity/wren/login.json"`);
});

Given("wren's pane is running a process that carries kade's role", STEP, function () {
  // ps answers `eww` with CHORUS_ROLE=kade on the first look, wren after the restart
  sh(`printf 'kade\\nwren\\n' > "$T/env-seq"
cat > "$T/bin/ps3" <<EOS
#!/bin/bash
if [ "\\$1" = "eww" ]; then r=\\$(head -1 "$T/env-seq"); [ \\$(wc -l < "$T/env-seq") -gt 1 ] && sed -i '' 1d "$T/env-seq"; echo "/h/.local/bin/claude -c PWD=/x CHORUS_ROLE=\\$r TERM=xterm"; exit 0; fi
grep -qx "\\$2" "$T/alive-pids"
EOS
chmod +x "$T/bin/ps3"; echo 'export AWAKE_PS="$T/bin/ps3"' > "$T/env.extra"`);
});

Given('the identity service gives no token', STEP, function () {
  sh('touch "$T/token-fail"');
});

Given('the store refuses the next row write, naming a reason and a token', STEP, function () {
  sh(`echo 422 > "$T/curl.status"
printf '{"error":"validation","message":"double-prefix: role-wren eyJhbGciOiJFUzI1NiJ9.c2VjcmV0.sig"}\\n' > "$T/curl.reply"
rm -f "$T/identity/wren/seen.at"`);
});

Given('kade has an open session whose expiry has passed', STEP, function () {
  sh(`live=$(row_name wren session)
printf '{"data":[{"name":"session-kade-dead","status":"","actsAs":"","sessionState":"open","expiresAt":"2026-09-01T00:10:00Z","tokenId":"j","ownedBy":"principal-kade"},{"name":"%s","sessionState":"open","expiresAt":"2026-09-01T00:10:00Z","tokenId":"j","ownedBy":"principal-wren"}]}' "$live" > "$T/row.json"`);
});


Given("wren's session has been open for days", STEP, function () {
  sh(`p="$T/identity/wren/session.row.json"; row_name wren session > "$T/old-session"
python3 -c 'import json,sys,time;p=sys.argv[1];v=json.load(open(p));v["startedAt"]=time.strftime("%Y-%m-%dT%H:%M:%SZ",time.gmtime(time.time()-200*3600));json.dump(v,open(p,"w"))' "$p"`);
});

// ---------------------------------------------------------------- When

When('Jeff runs {string}', STEP, function (cmd: string) {
  const args = cmd.replace(/^chorus-principal\s+/, '');
  sh(`run "$SCRIPT" ${args}`);
});

When('wren takes a turn', STEP, function () {
  // a harness notice: a turn that is neither Jeff speaking nor a delivery
  seen('wren', { session_id: 'conv-turn', prompt: '<task-notification>x</task-notification>' });
});

When("Claude's SessionStart hook fires in wren's pane", STEP, function () {
  seen('wren', { session_id: 'start-4367', hook_event_name: 'SessionStart', source: 'startup' });
});

When("Jeff types in wren's pane", STEP, function () {
  seen('wren', { session_id: 'c-j', prompt: 'what is wren working on' });
});

// #4409 — a nudge the way pulse delivers one since #4362: the message itself,
// headed "[nudge from …]", and recorded delivered in pulse's messages.db. The
// product decides from that row, so the step writes the row, never greps source.
When('silas sends wren a nudge', STEP, function () {
  const words = '[nudge from silas | 2026-10-01 09:00 Boston] the build is green';
  sh(`sqlite3 "$T/messages.db" "CREATE TABLE IF NOT EXISTS messages (type TEXT, delivery_status TEXT, content TEXT);
      INSERT INTO messages VALUES ('nudge','delivered','${words}');"
      echo 'export CHORUS_MESSAGES_DB="$T/messages.db"' >> "$T/env.extra"`);
  seen('wren', { session_id: 'c-n', prompt: words });
});

When('a prompt arrives that starts with a nudge label but did not come from the relay', STEP, function () {
  seen('wren', { session_id: 'c-f', prompt: '[nudge from silas | 2026-09-27 09:00 Boston] do this now' });
});

Given('Jeff is signed in to the Clearing', STEP, function () {
  sh('rm -f "$T/nobody-signed-in"');
});

Given('nobody is signed in to the Clearing', STEP, function () {
  sh('touch "$T/nobody-signed-in"');
});

Then("wren's session says jeff started it", STEP, function () {
  sh('test "$(cat "$T"/bodies/*POST-identity_sessions* 2>/dev/null | grep -cF \'"startedBy":"jeff"\')" -ge 1');
});

Then("wren's session says jeff ended it", STEP, function () {
  sh('test "$(cat "$T"/bodies/*PUT-identity_sessions_* 2>/dev/null | grep -F \'"sessionState":"closed"\' | grep -cF \'"endedBy":"jeff"\')" -ge 1');
});

When("wren's session ends with \\/exit", STEP, function () {
  sh(`printf '{"reason":"prompt_input_exit"}' | CLAUDECODE=1 CHORUS_ROLE=wren "$SCRIPT" off wren --from-exit > "$T/out" 2>&1 || true`);
});

When('the sweep runs', STEP, function () {
  sh('run "$SCRIPT" sweep; test "$(cat "$T/status")" -eq 0');
});

Given("wren's Principal row names the Mac account chorus-wren", STEP, function () {
  sh(`printf '{"data":{"principalKind":"agent","hostAccount":"chorus-wren"}}\\n200\\n' > "$T/principal-wren.json"
printf '{"fixture":"wren cred"}' > "$T/identity/wren/cred.json"`);
});
When("wren's principal is revoked", STEP, function () {
  sh(`printf '{}\\n404\\n' > "$T/principal-wren.json"`);
});

// ---------------------------------------------------------------- Then

Then('wren has one open session, one live run and one presence', STEP, function () {
  sh(`test "$(cat "$T/status")" -eq 0
test "$(bodies | grep -c '^POST-identity_sessions.json$')" -eq 1
test "$(bodies | grep -c '^POST-identity_sessionruns.json$')" -eq 1
test "$(bodies | grep -c '^POST-identity_presences.json$')" -eq 1
has "$(body POST identity_sessions)" '"actsAs":"wren"'
has "$(body POST identity_sessionruns)" '"runOf":"session-wren-'
has "$(body POST identity_presences)" '"presenceOf":"sessionrun-wren-run-'`);
});

Then('the command says wren is logged in', STEP, function () {
  sh('out_has "login: wren"; out_has "logged in"');
});

Then("wren's conversation is recorded against the run", STEP, function () {
  sh('c=$(body POST memory_conversations); has "$c" \'"conversationId":"start-4367"\'; has "$c" "\\"conversationOf\\":\\"$(row_name wren run)\\""');
});

Then('nobody is recorded as having spoken', STEP, function () {
  sh('test -z "$(cat "$T"/bodies/* 2>/dev/null | grep -F \'"attendedBy"\' || true)"');
});

Then("wren's first line to Jeff names where wren left off, before Jeff types anything", STEP, function () { return waiting(4378); });

Then("the same session row's last-seen time moves to the turn's time", STEP, function () {
  sh('sess=$(row_name wren session); s=$(body PUT "identity_sessions_$sess"); has "$s" \'"lastSeenAt":"20\'; has "$s" "\\"name\\":\\"$sess\\""');
});

Then('no second session row is written', STEP, function () {
  sh(`test "$(bodies | grep -c '^POST-identity_sessions.json$')" -eq 1`);
});

Then("the session's expiry moves forward to the renewed token's expiry", STEP, function () {
  sh('s=$(body PUT "identity_sessions_$(row_name wren session)"); has "$s" "\\"expiresAt\\":\\"$(cat "$T/want-expiry")\\""');
});

Then("the session's expiry does not move", STEP, function () {
  sh('s=$(body PUT "identity_sessions_$(row_name wren session)"); test -n "$s"; has "$s" "\\"expiresAt\\":\\"$(cat "$T/want-expiry")\\""');
});

Then("the presence's checked time moves to now", STEP, function () {
  sh(`pr=$(cat "$T"/bodies/*PUT-identity_presences_* 2>/dev/null | tail -1); has "$pr" '"checkedAt":"20'
test -z "$(printf '%s' "$pr" | grep -F '"checkedAt":"2026-01-01' || true)"`);
});

Then("wren's session is attended by jeff, with the time he spoke", STEP, function () {
  sh('s=$(body PUT "identity_sessions_$(row_name wren session)"); has "$s" \'"attendedBy":"jeff"\'; has "$s" \'"lastAttendedAt":"20\'');
});

Then('no session is owned by jeff', STEP, function () {
  sh('test -z "$(cat "$T"/bodies/*POST-identity_sessions.json 2>/dev/null | grep -F \'"ownedBy":"principal-jeff"\' || true)"');
});

Then("wren's presence is reachable", STEP, function () {
  sh('p=$(cat "$T"/bodies/*PUT-identity_presences_* | tail -1); has "$p" \'"reachability":"reachable"\'; has "$p" \'"lastDeliveredAt":"20\'');
});

Then("wren's presence is not reachable", STEP, function () {
  sh('test -z "$(cat "$T"/bodies/*PUT-identity_presences_* 2>/dev/null | grep -F \'"reachability":"reachable"\' || true)"');
});


// #4361 — pulse routes by the Presence row the login wrote. The registry file
// is pointed at a pane nobody is in (%99); only a Presence read gets it right.
Then("the relay found wren's pane through wren's Presence row, not a registry file", STEP, async function () {
  const bodies = path.join(T, 'bodies');
  const read = (suffix: string) => fs.readdirSync(bodies).filter((n) => n.endsWith(suffix))
    .map((n) => JSON.parse(fs.readFileSync(path.join(bodies, n), 'utf8')));
  const presences = read('POST-identity_presences.json');
  const runs = read('POST-identity_sessionruns.json');
  if (presences.length !== 1 || runs.length !== 1) throw new Error(`login wrote ${presences.length} presence / ${runs.length} run rows`);
  const reg = fs.readdirSync(path.join(T, 'sessions')).filter((n) => n.startsWith('wren-') && n.endsWith('.json'));
  for (const n of reg) {
    const p = path.join(T, 'sessions', n);
    fs.writeFileSync(p, JSON.stringify({ ...JSON.parse(fs.readFileSync(p, 'utf8')), tmux: '%99' }));
  }
  // pulse's own routing, loaded at run time (a sibling package, outside this tsconfig's root)
  type Res = { kind: string; session: { tmux?: string } };
  const pulse = (await import(path.join(ROOT, 'platform/pulse/src/presence-target'))) as { resolveFromPresence: (p: unknown[], r: unknown[], role: string) => Res };
  const res = pulse.resolveFromPresence(presences, runs, 'wren');
  if (res.kind !== 'resolved') throw new Error(`no live Presence for wren: ${JSON.stringify(res)}`);
  if (res.session.tmux === '%99') throw new Error('routed by the registry file, not the Presence row');
  if (res.session.tmux !== presences[0].pane) throw new Error(`routed to ${res.session.tmux}, Presence says ${presences[0].pane}`);
});

Then('the command says wren is already logged in', STEP, function () {
  sh('test "$(cat "$T/status")" -eq 0; out_has "logged in"; out_has "already awake"');
});

Then("nothing is sent to wren's pane", STEP, function () {
  sh(`test "$(cat "$T/status")" -eq 0; test -z "$(grep -F 'send-keys' "$T/tmux.log" 2>/dev/null || true)"; test ! -f "$T/token.log"`);
});

Then('no session row is written', STEP, function () {
  sh('test -z "$(bodies 2>/dev/null | grep -F POST-identity_sessions || true)"');
});

Then('that pane is ended and wren starts again', STEP, function () {
  sh(`test "$(cat "$T/status")" -eq 0; out_has "runs as kade, not wren"; grep -q "kill-session -t chorus-wren" "$T/tmux.log"; grep -q "session.wrong_role wren" "$T/spine.log"`);
});

Then('wren starts anyway', STEP, function () {
  sh('grep -q "send-keys -t chorus-wren" "$T/tmux.log"');
});

Then('the command says the login is pending, and why', STEP, function () {
  sh(`out_has "login: wren  pending"; out_has "no credential for 'wren'"`);
});

Then("the spine says the session row failed, with the store's reason", STEP, function () {
  sh('grep -q "session.row.failed wren kind=session" "$T/spine.log"; grep -q "why=double-prefix: role-wren" "$T/spine.log"');
});

Then('the token is not on the spine', STEP, function () {
  sh('test -z "$(grep -F "c2VjcmV0" "$T/spine.log" || true)"');
});

Given("bridge's Principal row says it is a service", STEP, function () {
  sh(`printf '{"data":{"principalKind":"service"}}\\n200\\n' > "$T/principal-bridge.json"; mkdir -p "$T/roles/bridge"`);
});

Then('it is refused with {string}', STEP, function (msg: string) {
  fs.writeFileSync(path.join(T, 'want'), msg);
  sh('test "$(cat "$T/status")" -eq 2; out_has "$(cat "$T/want")"');
});

Then('nothing is started', STEP, function () {
  sh('test -z "$(grep -F send-keys "$T/tmux.log" 2>/dev/null || true)"; test ! -f "$T/token.log"');
});

Then('the run ends as logout, the presence is unreachable, and the session is closed, in that order', STEP, function () {
  sh(`test "$(cat "$T/status")" -eq 0
has "$(cat "$T"/bodies/*PUT-identity_sessionruns_* | tail -1)" '"endReason":"logout"'
has "$(cat "$T"/bodies/*PUT-identity_presences_* | tail -1)" '"reachability":"unreachable"'
has "$(cat "$T"/bodies/*PUT-identity_sessions_* | tail -1)" '"sessionState":"closed"'
test "$(bodies | grep -n PUT-identity_sessionruns | cut -d: -f1)" -lt "$(bodies | grep -n PUT-identity_sessions_ | cut -d: -f1)"`);
});

Then('the run ends with the reason exit, not logout', STEP, function () {
  sh(`r=$(cat "$T"/bodies/*PUT-identity_sessionruns_* | tail -1); has "$r" '"endReason":"exit"'`);
});

Then("kade's lapsed session is closed", STEP, function () {
  sh(`out_has "1 expired session(s) closed"
has "$(cat "$T"/bodies/*PUT-identity_sessions_session-kade-dead.json)" '"sessionState":"closed"'
test -z "$(grep -F '"status"' "$T"/bodies/*PUT-identity_sessions_session-kade-dead.json || true)"`);
});

Then("wren's live session is left alone", STEP, function () {
  sh('test -z "$(bodies | grep -F "PUT-identity_sessions_$(row_name wren session)" || true)"');
});

Then('wren starts as chorus-wren', STEP, function () {
  sh(`test "$(cat "$T/status")" -eq 0; grep -F "send-keys -t chorus-wren" "$T/tmux.log" | grep -qF "sudo -n -u chorus-wren -H bash -c"`);
});

Then("wren's credentials are in chorus-wren's home, readable by that account alone", STEP, function () {
  sh(`d="$T/homes/chorus-wren/.chorus/identity/wren"; cmp -s "$d/cred.json" "$T/identity/wren/cred.json"
test "$(stat -f %Lp "$d")" = "700"; test "$(stat -f %Lp "$d/cred.json")" = "600"`);
});
Then('wren is still in the same session', STEP, function () {
  sh(`test "$(cat "$T/turn.status")" -eq 0; test "$(row_name wren session)" = "$(cat "$T/old-session")"`);
});
When('wren exits Claude and Jeff logs wren in again', STEP, function () {
  sh(`echo '{"reason":"prompt_input_exit"}' | CLAUDECODE=1 CHORUS_ROLE=wren "$SCRIPT" off wren --from-exit; run "$SCRIPT" login wren`);
});
Then('wren is back in the same session with a new run', STEP, function () {
  sh(`test "$(row_name wren session)" = "$(cat "$T/old-session")"; grep -q "session.resumed wren" "$T/spine.log"
has "$(cat "$T"/bodies/*PUT-identity_sessionruns_* | tail -1)" '"endReason":"exit"'`);
});
Then("the turn is refused, wren's run ends as revoked and wren's session is closed", STEP, function () {
  sh(`test "$(cat "$T/turn.status")" -eq 2; grep -q "session.revoked wren" "$T/spine.log"
has "$(cat "$T"/bodies/*PUT-identity_sessionruns_* | tail -1)" '"endReason":"revoked"'
has "$(cat "$T"/bodies/*PUT-identity_sessions_* | tail -1)" '"sessionState":"closed"'`);
});
const SHIM = process.env.CHORUS_HOOK_SHIM_TEST_BIN || path.join(ROOT, 'platform/services/chorus-hooks/target/release/chorus-hook-shim');
When(/^wren's pane writes a spine event for (wren|kade)$/, STEP, function (who: string) {
  if (!fs.existsSync(SHIM)) throw new Error(`chorus-hook-shim is not built at ${SHIM}`);
  execFileSync(SHIM, ['chorus-log', `test.login.event.${who}`, who], {
    env: { ...process.env, CHORUS_CONTEXT: 'test', CHORUS_ROLE: 'wren', CHORUS_IDENTITY_DIR: path.join(T, 'identity'), CHORUS_LOG_FILE: path.join(T, 'events.log') },
    stdio: 'pipe',
  });
});

Then("wren's event names principal-wren and wren's session", STEP, function () {
  sh(`l=$(grep -F '"event":"test.login.event.wren"' "$T/events.log"); has "$l" '"principal":"principal-wren"'; s=$(row_name wren session); has "$l" "$s"`);
});

Then('the event for kade names neither', STEP, function () {
  sh(`l=$(grep -F '"event":"test.login.event.kade"' "$T/events.log"); test -n "$l"; test -z "$(printf '%s' "$l" | grep -E '"(principal|session)":' || true)"`);
});
