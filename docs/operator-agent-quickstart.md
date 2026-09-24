# Switch a Chorus role to OpenCode

This guide is for Jeff or another authorized operator on an **existing Chorus deployment**. A contributor can build and test the integration without installing Chorus services, provisioning role identities, or running these deployment commands on their own Mac.

OpenCode is the agent runtime: it reads files, runs tools, and maintains the coding conversation. The model provider is the service OpenCode calls. You can use a provider already supported by OpenCode, or supply a compatible Chat Completions or Responses endpoint. Switching Wren's runtime leaves Wren's identity and Chorus responsibilities intact.

## The short path

Run from the checked-out Chorus revision you intend to deploy, in the deployment account that owns the role's credentials and services:

```sh
python3 platform/scripts/chorus-agent-setup install --restart-services
python3 platform/scripts/chorus-agent-setup setup wren --runtime opencode
python3 platform/scripts/chorus-agent-setup check wren
python3 platform/scripts/chorus-agent-setup switch wren opencode
```

`install --restart-services` builds, installs, and activates the integration in the existing Chorus services. `setup` asks for the runtime/model settings and generates the profile and instructions. `check` explains anything still missing. `switch` starts the configured session and opens a terminal conversation. The installed shortcut is `~/.chorus/bin/chorus-agent-setup`; the repository command works without changing your shell PATH.

An existing deployment may also need its API, Pulse, hooks, and MCP services updated or restarted. A successful build alone does not mean the running services changed. Use the steps below for the first rollout.

## Before the first rollout

You need:

- An existing, working Chorus deployment, including its API, CSS identity issuer, hooks, Pulse messaging service, and ordinary card/worktree tools.
- The role's existing credential under `${CHORUS_IDENTITY_DIR:-$HOME/.chorus/identity}/wren/cred.json`. The credential's `hostAccount`, when set, must match the account running the command.
- Python 3, Node **22 or newer**/npm, Rust/Cargo, and the signing identity used by the deployment's existing `build-signed.sh` workflow.
- An OpenCode **V2** CLI and provider authentication. V1 and V2 are different integrations; installing the stable V1 package is insufficient.

Follow the [OpenCode V2 installation instructions](https://opencode.ai/v2/docs/) for your deployment. They currently offer `brew install anomalyco/tap/opencode-v2` and the npm package `@opencode/cli`. Setup detects and pins the installed version; it does not silently upgrade an existing OpenCode installation. If your V2 executable is named `opencode2` or is outside PATH, add `--executable /absolute/path/to/opencode2` to `setup`.

Use the role's deployment account when its credential requires it. Running a command as Jeff does not automatically authorize using another account's bound credential. The setup helper reports an account mismatch; it does not copy credentials, rewrite the binding, or provision a replacement identity.

The API and Pulse must reach the same owner-restricted supervisor socket used by the session. A central service in another account or on another Mac needs the deployment's explicit service wiring; do not make the socket world-writable to get past a readiness failure.

## 1. Install the integration

Finish active work before changing the running services. Then run:

```sh
python3 platform/scripts/chorus-agent-setup install
```

The installer uses the selected checkout, committed npm lockfiles, and the existing Rust signing/install pipeline. It prepares the TypeScript services and workers and installs the Chorus agent binaries. It does not switch any role or send a paid model request.

By default it does not restart existing deployment services. To explicitly restart the relevant existing LaunchAgents after installation, use:

```sh
python3 platform/scripts/chorus-agent-setup install --restart-services
```

Review the reported service actions. The helper plans updates for the known API, Pulse, hooks, and MCP LaunchAgents, points their commands at this checkout and the selected Node executable, and reloads them so those settings take effect. It preserves unrelated settings and saves the original plists beside them as `.plist.agent-backup`. An unrecognized custom launcher is refused before building and needs operator review. Other services are not restarted.

The default build-only command leaves service commands unchanged. Native Node dependencies built with Node 22+ cannot safely run under an old Node 20 service command; align those commands before manual activation, or use `install --restart-services` for the supported launch configurations.

Before admitting arbitrary clients, the HTTP MCP service must use request authentication. If existing Claude clients still use unauthenticated MCP, migrate their credentials first; enabling strict mode will correctly refuse those callers. To explicitly enable strict authentication in the existing MCP LaunchAgent and restart the affected services:

```sh
python3 platform/scripts/chorus-agent-setup install --strict-mcp --restart-services
```

Existing unrelated LaunchAgent settings are preserved. `--yes` is available for an operator who has already reviewed the installation actions. It does not supply missing credentials or turn a failed readiness check into success.

## 2. Configure OpenCode and its model

```sh
python3 platform/scripts/chorus-agent-setup setup wren --runtime opencode
```

The wizard offers:

| Choice | What you provide |
| --- | --- |
| OpenCode provider | An OpenCode `provider/model` identifier and the environment variable supplying that provider's API key. |
| OpenAI-compatible Chat Completions | The API base URL, upstream model ID, and the name of the environment variable containing its key. |
| OpenAI-compatible Responses | The API base URL, upstream model ID, and the name of the environment variable containing its key. |

Use the API protocol your endpoint actually implements. A Chat Completions endpoint is not automatically a Responses endpoint. The base URL is normally the provider's API root, such as `https://gateway.example.com/v1`, rather than its individual `/chat/completions` route.

The wizard stores credential **references**, not provider keys. Make the chosen variable available in the shell that runs `switch` and `open`, using your usual secret manager or shell environment. Do not paste provider keys into checked-in configuration or pass them as command arguments. The current guided workflow requires an API-key environment variable for built-in providers as well as custom endpoints. It uses private OpenCode state and does not import OAuth logins or credentials from a personal OpenCode installation.

The generated profile uses an isolated OpenCode configuration, Chorus instructions, skills, and authenticated MCP bridge. It does not replace unrelated personal OpenCode configuration or the role's existing custom instruction files. The role's canonical anchor remains its read surface; card work still belongs in the card's worktree.

Setup also shows the runtime's declared enforcement gaps and asks the operator to accept trusted enrollment explicitly. Trusted enrollment does not certify the runtime, disable supported policy denials, or grant a different Chorus identity.

The wizard also asks whether OpenCode should `ask` before running tools (the default) or `allow` them under the installed Chorus policy. Choose `allow` for an autonomous coding loop if you authorize that access. This removes OpenCode's own per-tool prompts; supported Chorus hook denials, MCP role permissions, and human-only operations still apply. For unattended configuration, use `--tool-permissions allow --trust-runtime` to make that operator decision explicit.

`setup` prepares files. It does not change an active conversation or invoke the model. For a repeatable custom-endpoint configuration, supply the same choices as options:

```sh
python3 platform/scripts/chorus-agent-setup setup wren --runtime opencode \
  --protocol openai-chat --base-url https://gateway.example.com/v1 \
  --model upstream-coding-model --key-env COMPANY_LLM_API_KEY \
  --trust-runtime
```

`--trust-runtime` is the explicit operator acceptance of the listed enforcement gaps. Omit it for the interactive confirmation. Use `--protocol openai-responses` for a Responses endpoint. `--name NAME` saves another selectable configuration without replacing the currently selected profile; select it with `switch wren NAME`. Each setup creates a new revision, preserving the old one for any existing session.

If the role anchor or card worktree base differs from the deployment default, provide `--workspace /absolute/role/anchor` and `--worktree-base /absolute/chorus-werk`. The global `--root /absolute/chorus` and `--state-dir /absolute/state` options go **before** the subcommand. The signed installer uses the standard `~/.chorus` state directory.

## 3. Check readiness

```sh
python3 platform/scripts/chorus-agent-setup check wren
```

Use this before the first switch and after updating a runtime. Read the named failures and their suggested remedies. Checks distinguish local configuration and executable readiness from running service readiness. They do not certify a complete card workflow or spend tokens merely to prove a model is reachable.

Do not continue by weakening identity checks or changing socket permissions. A failure normally means an old service is still running, a provider variable is absent, the wrong account is in use, or deployment wiring does not point at the selected supervisor.

## 4. Start or switch the role

For a role with no existing session:

```sh
python3 platform/scripts/chorus-agent-setup switch wren opencode
```

If Wren is currently in a legacy Claude session, finish its turn, save the task/worktree/obligation summary to a handoff file, and stop that session through its normal interface. Then explicitly acknowledge that it is stopped:

```sh
python3 platform/scripts/chorus-agent-setup switch wren opencode \
  --handoff /absolute/path/wren-handoff.md --legacy-stopped
```

`--legacy-stopped` is an operator assertion; it does not kill a Claude process. Do not use it while the old role session is still operating. For a session already tracked by the new supervisor, switching uses its explicit registration and handoff instead of selecting whichever vendor conversation happens to be newest.

The helper refreshes the role's Chorus credential using the existing `chorus-identity-token` issuer, starts the owner-local supervisor as needed, starts a dedicated OpenCode server, and enrolls the selected role. Cross-runtime changes start a fresh conversation with Chorus handoff context; they cannot resume Claude's private history inside OpenCode.

The chat continues showing peer-message work and pending permissions while it waits for your input. Text submitted while a turn is running is refused with a message rather than silently queued.

You converse through the **Chorus terminal chat**, not a separately launched OpenCode TUI. This keeps session identity, message admission, approvals, and turn completion under the same session controller. The dedicated OpenCode server lives while that terminal connection is open. Do not start an unrelated OpenCode session against it.

Terminal commands:

| Command | Behavior |
| --- | --- |
| Ordinary text | Send a message to the current session. |
| `/paste` | Enter multiline text using the terminator shown by the prompt. |
| `/status` | Show session state and runtime information. |
| `/cancel` | Request cancellation of the active turn. |
| `/approve REQUEST_ID once` | Allow a pending approval once, subject to the required human identity. |
| `/approve REQUEST_ID reject` | Reject a pending approval. |
| `/quit` | Disconnect, retaining the explicit session for later resume. |

With the default `ask` tool-permission setting, starting the chat without a human token can succeed, but tool work waits at an approval request. Supply `--human-token-file` on `switch` and keep the terminal open to answer `/approve REQUEST_ID once` or `reject`. If the operator selected `allow` during setup, OpenCode's ordinary tool calls proceed under Chorus policy without these per-tool approvals.

A permission request requires Jeff's verified human credential. The role's token cannot approve its own request merely because a human is typing in the terminal. Supply an existing owner-only human token file when opening the conversation:

```sh
python3 platform/scripts/chorus-agent-setup open wren \
  --human-token-file /absolute/private/jeff.token
```

The same option is available on `switch`. This file is a Chorus identity token, not the model-provider API key. The helper renews the role's credential; the existing human-credential workflow must keep the human token current.

## Return later, inspect, or stop

Resume the saved session explicitly:

```sh
python3 platform/scripts/chorus-agent-setup open wren
```

Show the runtime/session state:

```sh
python3 platform/scripts/chorus-agent-setup status
```

Release a role's registration when it is finished:

```sh
python3 platform/scripts/chorus-agent-setup stop wren
```

Stopping can refuse while a turn, delivery, or approval needs reconciliation. Resolve the reported state first. Never replay a possibly accepted mutating operation just because a connection was lost.

Changing provider/model settings requires a controlled session switch or handoff. It does not rewrite an active turn. A change to Wren's runtime also does not change gates, ops, or Clearing provider settings; those workloads have separate profiles.

## Common failures

| Message or symptom | Action |
| --- | --- |
| Missing role credential or host-account mismatch | Run in the intended deployment account with its existing identity. Use the established Chorus identity provisioning process if that role has never been provisioned. |
| Identity endpoint missing or refused | Deploy/restart the updated API and verify CSS, issuer/JWKS configuration, and the requested role. A role header is not a substitute for a token. |
| API cannot reach supervisor | Check the API's configured `CHORUS_AGENT_SOCKET`, account, and local deployment topology against the setup output. |
| Hooks unavailable | Start the deployment's hooks service and verify its owner-local socket. Do not bypass the hook to get a model running. |
| Pulse routing unavailable | Deploy/restart the updated Pulse service and confirm it shares the configured supervisor and Pulse secret. |
| MCP authentication not strict | Migrate existing clients to authenticated MCP, then explicitly enable strict mode on the exposed MCP service. |
| Unsupported OpenCode version | Install the V2 CLI or reconfigure after deliberately validating an updated release. The profile pins a specific version. |
| Provider or model unavailable | Check the exact OpenCode model identifier or upstream model ID, API protocol, and provider credential environment. Credentials must reach the dedicated server. |
| Existing active/uncertain session | Finish/cancel or reconcile that session before replacing its primary binding. |
| Generated file was modified | Preserve the local changes and follow the conflict instructions. Do not erase custom instructions to force setup through. |

## What is tested and what the operator validates

The repository's setup/switch tests use temporary homes, fake executables, local fixture services, and deterministic responses. They check configuration preservation, identity failures, command construction, switching, and cleanup without relying on a contributor's live Chorus installation or paid provider requests.

A real deployment still needs its bounded smoke test and card-workflow acceptance: load role context, pull the correct card worktree, exercise policy, exchange a message with another role, run the ordinary gates, and hand off or resume. Keep the existing Claude path available during rollout. The longer canary and rollback criteria remain in the [migration runbook](agent-runtime-migration.md).
