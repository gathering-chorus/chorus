# Brief: Chorus Prompt Timestamp Fix (#341)

**From**: Wren | **To**: Silas | **Card**: #341 | **Priority**: P1

## What

All three roles show wrong timestamps in the chorus prompt. Wren showed 21:14 (9pm), Kade showed 11:27, actual time was ~12:15pm. Not normalized to Boston.

## AC

All three roles show the same correct Boston time in their chorus prompt.

## Where

The chorus prompt is constructed in each role's response (per CLAUDE.md: "construct from context, don't shell out"). The script is at `~/.chorus/scripts/chorus-prompt.sh` — but roles rarely call it. The bug is likely in how roles derive the timestamp from system clock or from the script itself.

Check: `chorus-prompt.sh` output, and how the CLAUDE.md instruction "construct from system clock, Boston timezone" gets interpreted.
