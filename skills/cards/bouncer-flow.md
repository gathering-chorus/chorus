# cards add — Bouncer Flow

The `cards add` bouncer enforces the discipline Jeff named on 2026-05-11: agents
cannot file cards unilaterally; every agent-initiated card lands in Jeff's
attention with a six-section structured justification before Jeff (and only
Jeff) decides to file. The discipline lives at the `cards add` CLI; the
delivery channel is the model's own response surface, not a side channel.

## Why the model's response is the delivery channel

Tonight (2026-05-11) we demonstrated three failure modes in one stretch:

- **Peer relay** — Silas hit the bouncer, saw the composed ask, then relayed a
  summary to Jeff via another path instead of forwarding the four-question
  articulation. Jeff received a meta-question, not the ask.
- **chorus_nudge_message to jeff** — Wren sent via the MCP nudge primitive;
  trace logged success; Jeff received nothing on the surface he was watching.
- **Bridge POST to `/api/message`** — Wren posted to the Bridge stream and
  received `{"ok":true}`; Jeff received nothing where his eyes were.

The only channel that reliably reached Jeff was the model's own response text.
That's also the channel he can never miss — he is reading it by definition.

## The pickup-file contract

When an agent (`DEPLOY_ROLE` in `wren / silas / kade`) runs `cards add` with a
description that meets the six-section bar, the bouncer:

1. Composes the `[card-approval] <role> → jeff` ask from the description.
2. Writes the composed ask to `~/.chorus/pending-approvals/<role>-<timestamp>.txt`.
3. Prints the composed ask to stdout (mirror moment).
4. Exits non-zero with a refusal message.

The model contract — the rule the running session must follow:

- **Before composing a response to Jeff, check `~/.chorus/pending-approvals/`
  for files matching the current role.** If any exist, surface the file
  contents verbatim in the response text — as a markdown block — at the
  top of the response.
- **Mark the file consumed after surfacing.** Delete it, or rename to
  `<file>.consumed`, so the next response doesn't re-surface a duplicate.
- **Never paraphrase or summarize a pending approval.** Verbatim or not at all.
  Paraphrasing is the failure mode Silas demonstrated; it is exactly what the
  pickup substrate exists to prevent.

The contract is enforceable by a hook (PostToolUse on Bash → detect
`[card-approval pickup written to ...]` in output → mark for next response).
That hook is follow-on if model-side discipline turns out insufficient. Pickup
file is the durable artifact regardless.

## Jeff's path

Jeff approves by replying "approve" (or equivalent) in conversation. Jeff
files the card himself from his terminal with `DEPLOY_ROLE=jeff cards add ...`
(or via this session with explicit verbal authorization captured in the
description's audit-trail section).

Jeff denies by replying with the reason or by not acting (10-minute soft
timeout; the pickup file ages out of relevance, not of disk).

## Six required sections

Every agent `cards add` description must include, each substantive:

- `## Experience` — what changes for the user
- `## Why this matters` — who benefits, what breaks without this, why now (≥30 words)
- `## Why it helps Chorus` — how this serves team coordination (≥30 words)
- `## Why it's not gold plating or a nit` — load-bearing not cosmetic (≥30 words)
- `## Dependencies` — what needs to land first; count is the truth-teller (≥20 words)
- `## Scope of impact` — surfaces touched, who affected, what could break elsewhere (≥20 words)

Missing or thin sections → single refusal listing every gap. No pickup file
written; no ask reaches Jeff. The agent fixes the description and retries.

## Bypasses

- `DEPLOY_ROLE=jeff` (or unset) — bouncer hook returns early. Jeff's terminal
  files directly. Description validation still applies (universal quality bar).
- `NODE_ENV=test` — hermetic test runs bypass.

No env-var bypass an agent can set in their own shell.
