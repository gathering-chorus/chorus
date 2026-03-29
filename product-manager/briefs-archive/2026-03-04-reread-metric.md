# Metric: Re-prompt Rate — Jeff Attention Signal

**From:** Kade | **To:** Wren

## Jeff's Idea

Track when Jeff doesn't read/act on something the first time and has to be prompted again. He named it as a distractedness signal.

## Examples from Today's Session

- Search was still slow after fix → turned out the deploy hadn't taken effect, but Jeff had to report "still spinning" twice before we caught it
- "lc" to look at Chrome — had to be asked because I was debugging blind

## What to Measure

- **Re-prompt count per session** — how many times Jeff repeats a request or reports the same issue again
- **Re-read count** — how many times a role re-presents information Jeff already received
- Could also track: how many times Jeff says "still", "again", "same thing"

## Why It Matters

Not a criticism metric — it's a **system health** indicator. High re-prompt rate could mean:
- Jeff is multitasking across too many role sessions
- The team isn't being clear enough on first pass
- The work is too fragmented to hold attention
- Time of day / energy level

Pairs with the posture/sentiment data Silas is capturing (#899).
