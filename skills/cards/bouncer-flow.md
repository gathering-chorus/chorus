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
- **clearing-nudge to jeff** — Wren sent via the MCP nudge primitive;
  trace logged success; Jeff received nothing on the surface he was watching.
- **Bridge POST to `/api/message`** — Wren posted to the Bridge stream and
  received `{"ok":true}`; Jeff received nothing where his eyes were.

The only channel that reliably reached Jeff was the model's own response text.
That's also the channel he can never miss — he is reading it by definition.

## The Approval row (#4505)

When an agent (`DEPLOY_ROLE` in `wren / silas / kade`) runs `cards add` with a
description that meets the six-section bar, the bouncer:

1. Builds the `cards add` argv the approve will replay. Only `--owner
   --priority --domain --type --origin --desc --sequence` ride an ask;
   anything else (`--desc-file`, `--desc -`, `--parent`) is refused at post.
2. Posts ONE Approval row to the door (`/decisions/approvals`) as the asking
   role. The door stamps the owner from the verified writer. A retry of the
   same role+title is "already waiting", not a second ask.
3. Prints one heads-up line (the first ask links Jeff's list; later asks
   while others wait add to it without a second link) and the composed ask.
4. Exits non-zero.

The model contract: surface the `[card-approval]` block verbatim in your next
reply. Never paraphrase it.

## Jeff's path

Jeff opens his approvals list on the go page (`/go/approvals`), oldest first.
Approve or decline is a passkey signature over that ask's exact argv. Approve
writes his ApprovalDecision row as principal-jeff and replays the argv through
`cards add` as data with `DEPLOY_ROLE=jeff`; decline records and files nothing.
**Typing "approve" in a role's terminal does nothing** — the chat keyword
responder was retired by #4505. A row edited after the page was shown, a
decided ask, or a signature from any other key is refused.

## Six required sections

Every agent `cards add` description must include, each substantive:

- `## Experience` — what changes for the user
- `## Why this matters` — who benefits, what breaks without this, why now (≥30 words)
- `## Why it helps Chorus` — how this serves team coordination (≥30 words)
- `## Why it's not gold plating or a nit` — load-bearing not cosmetic (≥30 words)
- `## Dependencies` — what needs to land first; count is the truth-teller (≥20 words)
- `## Scope of impact` — surfaces touched, who affected, what could break elsewhere (≥20 words)

Missing or thin sections → single refusal listing every gap. No row is
posted; no ask reaches Jeff. The agent fixes the description and retries.

## Bypasses

- `DEPLOY_ROLE=jeff` (or unset) — bouncer hook returns early. Jeff's terminal
  files directly. Description validation still applies (universal quality bar).
- `NODE_ENV=test` — hermetic test runs bypass.

No env-var bypass an agent can set in their own shell.
