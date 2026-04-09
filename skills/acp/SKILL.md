---
name: acp
description: Accept card + commit + push — full acceptance flow in one command.
user-invocable: true
---

# /acp — Accept, Commit, Push

One command for the full acceptance flow. Jeff says `acp <card-id>` and the card is Done, committed, and pushed.

## Arguments

```
CARD_ID=<first argument>
```

If no card ID given, check for a card currently in demo state or the most recent demo brief.

## Step 0: Demo gate (DEC-048)

Before accepting, verify a demo happened. Check for:
1. A demo brief in your `briefs/` directory referencing this card (e.g., `*demo*${CARD_ID}*`)
2. Or Jeff explicitly saying "accept" / "acp" after seeing the work live

If **neither** exists and **you are the building role** (not Jeff or Wren accepting):
- **STOP.** Do not proceed.
- Say: `#${CARD_ID} needs a demo before acceptance. Run /demo ${CARD_ID} first.`
- Exit the skill.

If **Jeff or Wren** is running /acp, they are the accepting authority — proceed (they've seen it or are overriding).

## Step 1: Accept the card

```bash
bash /Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/board-ts done ${CARD_ID}
```

## Step 2: Emit spine event

```bash
/Users/jeffbridwell/CascadeProjects/chorus/platform/scripts/chorus-log card.accepted <your-role> card=${CARD_ID}
```

## Step 2.5: State file sync (if Wren)

If Wren is running /acp, check if `projects.md` or `backlog.md` need updating based on what just shipped. Update inline — don't defer to close-out.

## Step 3: Commit

Stage and commit all pending changes in the chorus repo. Use `git-queue.sh` for serialized commits:

```bash
cd /Users/jeffbridwell/CascadeProjects/chorus && DEPLOY_ROLE=<your-role> bash platform/scripts/git-queue.sh commit <your-dirs>/ -- -m "<your-role>: acp #${CARD_ID} — <short description>"
```

Where `<your-dirs>` is your role's directory plus any shared files you changed (e.g., `chorus/architect/ platform/`).

## Step 4: Push (race-safe)

**CRITICAL: Always pull --rebase before push.** Multiple roles commit to main. A bare `git push` will fail if another role pushed first. Do NOT improvise with `git stash` — that loses changes.

```bash
cd /Users/jeffbridwell/CascadeProjects/chorus && git pull --rebase && git push
```

If `git pull --rebase` fails due to dirty working tree:
1. **STOP.** You have uncommitted changes that should have been in Step 3.
2. Run `git status` to see what's dirty.
3. Commit those changes with `git-queue.sh`, then retry Step 4.
4. **NEVER run `git stash` during acp.** Stash-drop sequences lose work.

If `git pull --rebase` fails due to conflicts:
1. Report the conflict to Jeff. Do not force-push or reset.
2. The accept still stands (card is Done), but the push needs manual resolution.

## Step 5: Confirm

One line:

```
Accepted #<card-id> — committed and pushed.
```

## Rules

- No confirmation prompt — Jeff said accept, so accept
- If the card isn't in WIP or Demo state, warn but still proceed (Jeff overrides)
- Always emit the spine event before committing
- If push fails (hook, network), report the failure but the accept still stands
- **NEVER use `git stash` during acp.** Two incidents in 12 hours lost committed work via stash-drop races. All changes must be committed before pushing. If the working tree is dirty at push time, commit first — don't stash.
- **Always `git pull --rebase` before `git push`.** Bare push fails when another role pushed first. The rebase replays your commit cleanly on top.
