# Native runtime hooks

`chorus-hook-shim runtime-hook <runtime> <event>` translates native hook payloads
into the existing Chorus policy vocabulary. The existing Claude entrypoints are
unchanged. Native role sessions require a supervisor-issued `CHORUS_SESSION_ID`
and launcher-owned `CHORUS_ROLE` or `DEPLOY_ROLE`; JSON payloads cannot enroll a
session or grant trusted enforcement.

Supported bindings are Claude/Codex command hooks, Gemini command hooks, and an
OpenCode **V2** plugin. A generated bundle is staging material, not a conformance
attestation. Verify the installed runtime version, hook loading, denial behavior,
history, and context delivery before enrolling a verified profile. Unknown tool
shapes refuse execution. Policy `ask` decisions refuse until an adapter implements
an explicit approval exchange; they never become implicit permission.

## Policy and evidence

File tool aliases resolve to the same Read/Edit/Write checks. Codex/OpenCode patch
inputs are parsed completely before evaluation; every add, update, delete,
original move source, and move destination is checked. Existing parent symlinks
are resolved before path policy. Missing paths, malformed patches, and unknown
tools fail closed. Shell operations use the existing Bash policy and retain its
limitations; this translator is not a filesystem sandbox.

Native hooks publish bounded, normalized observations through the supervisor's
owner-only Unix socket. Only successful completed operations become tool-use
evidence. Error output remains error evidence. Credentials and capability claims
are excluded from metadata. V2 enrollment selects the canonical
`agent-events/<session_id>.jsonl` journal for `SessionCache`; a missing, empty, or
torn journal yields explicit unavailable evidence instead of a permissive empty
transcript. The raw Claude JSONL importer remains for legacy sessions.

Assistant synthesis is extracted from actual text blocks. Quoted tool arguments
or tool results cannot impersonate an assistant synthesis. Enrolled non-Claude
sessions use the shared Chorus context index without silently importing Claude's
automatic memory directories.

## Context delivery

Session start and prompt boundaries request context from the supervisor. Handoff
context and each valid Pulse message are appended to the native additional-context
response with sender identity preserved. Only after stdout has been successfully
written and flushed does the shim acknowledge those numeric message IDs and the
handoff context token. A broken output pipe leaves claims unacknowledged.

Enrolled response-debt checks use only context-acknowledged messages delivered
to that Chorus session and verified outbound replies from the same session.
They retain the bounded respond-first gate and its reply escape path, with
refusal counters scoped to role and session. Missing message schema or database
evidence produces an explicit advisory, never a claimed clear balance. Native
context bypasses the legacy global role nudge fold to avoid duplicate delivery.

OpenCode's plugin resolves the native session ID to a registered Chorus identity
before invoking policy. It never uses plugin-location cwd or a shared process role
as session identity. Authenticated MCP requires one dedicated server endpoint per
live session. Generated profile binding supplies the operator profile name and
credential-file/environment references, never token values.

## Staging configuration

`platform/scripts/agent-config.py --runtime <runtime> --role <role> --output <new-directory>`
renders canonical fragments, MCP configuration, hooks, and skills. Output must not
exist. It never overwrites custom files or installs user configuration. Per-file
and total byte budgets fail before creating output; source and generated hashes
are recorded in `bundle.json`.

For OpenCode endpoints add `--profiles <operator-profiles.json> --profile <name>`.
Use `profile.model: "chorus-endpoint/coder"` and `provider.model_id` for the actual
endpoint model. `provider.protocol` selects Chat Completions or Responses, and
`provider.api_key_env` names the credential variable. OpenCode MCP profile binding
still requires its dedicated server to inherit the correct role and token-file
environment. Profiles without a custom provider can use the same binding flag.

Skills keep canonical workflow content. Non-Claude projections replace only MCP
host prefixes with logical names resolved by runtime tool discovery. Claude
commands, native delegation, memory paths, and special metadata are reported for
explicit review; copying a skill does not prove its workflow is portable.

## Legacy boundaries

The Claude PID/TTY registry and terminal injection paths remain as compatibility
support for roles without a configured runtime profile. Native hooks bypass the
Claude ancestry-based registration healer. `chorus-awake` delegates configured
roles to `chorus-agent launch`; a supervisor failure never falls back to a second
Claude conversation. The paired `build-signed.sh chorus-agent` shortcut installs
the CLI and daemon only when explicitly run; the LaunchAgent file is an opt-in
example and is not installed by the renderer.
