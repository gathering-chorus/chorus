---
name: lm
description: Look at me — latest posture/sentiment score
user-invocable: true
---

# lm — Look at Me

Pull the latest posture/sentiment capture. Two keystrokes.

## How to Use

1. Read the latest line from today's scores file:
   ```bash
   TODAY=$(date +%Y-%m-%d)
   tail -1 /tmp/posture-timelapse/$TODAY/scores.jsonl
   ```
2. Parse the JSON — fields include: mood, energy, expression, posture, tension, breath
3. Report naturally — "You look focused, posture is good, energy medium" not a JSON dump

If no scores exist today, say so — the posture LaunchAgent may not be running.
