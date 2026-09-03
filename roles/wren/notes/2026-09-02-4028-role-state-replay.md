# #4028 — replay of the derived role state over the 2026-08-28 spine

Run 2026-09-02 15:30 with `node platform/api/dist/cli/role-state-replay.js 2026-08-28 11:00 11:25 12:00 15:33` — the same `stateFromStreams` the live endpoint uses.

```
sample (Boston)   role   state     card (from card.pulled, reconstructed)  last event            last activity
----------------  -----  --------  --------------------------------------  --------------------  -------------
2026-08-28 11:00  silas  waiting   #4025                                   system.heartbeat      16s ago      
2026-08-28 11:00  wren   idle      #4026                                   mcp.transport.error   —            
2026-08-28 11:00  kade   waiting   #4022                                   agent.activity        6s ago       
2026-08-28 11:25  silas  waiting   #4025                                   system.heartbeat      16s ago      
2026-08-28 11:25  wren   building  —                                       pair.navigator.stall  782s ago     
2026-08-28 11:25  kade   building  #4022                                   agent.activity        6s ago       
2026-08-28 12:00  silas  waiting   #4025                                   system.heartbeat      16s ago      
2026-08-28 12:00  wren   building  —                                       hook.decision         95s ago      
2026-08-28 12:00  kade   waiting   #4022                                   agent.activity        6s ago       
2026-08-28 15:33  silas  building  —                                       system.heartbeat      9s ago       
2026-08-28 15:33  wren   building  —                                       observer.digest       1s ago       
2026-08-28 15:33  kade   building  #4022                                   agent.activity        19s ago      
60631 spine lines read for 2026-08-28 (lookback 60 min per sample); same function as GET /api/chorus/context/roles.
```

Against the card's expectations (written 08-28 from the DECLARED file):
- kade building #4022 late morning — matches (11:25 building #4022; 11:00 and 12:00 read *waiting*: a demo he had presented was still unanswered).
- wren active 15:33 — matches (building, observer.digest 1s before).
- silas building 11:00–11:25 on #4025 — the streams say *waiting*: #4025 had been presented and had no go yet. The declared file said building; the stream says he was waiting on Jeff.
- wren idle 12:00 — the streams say *building*: a hook.decision 95 s before noon. The declared file said idle; the tool calls say otherwise.

Two of four 'expectations' were the declared file's opinion. That is the defect this card removes.

## Same function over today (2026-09-02), the two windows Silas asked to see

```
silas  13:45 building #4064 · 14:15 building (last activity 461s) · 14:45 building (667s) · 15:00 building (195s)
wren   10:00–11:30 waiting #4045 (a presented demo with no go — true, I was waiting on Jeff) · 13:45–15:00 idle (true)
```

Silas's probes counted as work; his quiet hour did not. The declared file had said `wren=idle` through six pipeline rounds that morning; the streams say `waiting`, which is what was actually happening.

## 18:22 — re-present after #4075 landed

The pipeline re-announced round 127's proof as "same patch, prove already paid" while the wren variant
slot had been torn down by #4075's accept (env-down runs per role, and two cards of one role share the
slot). The demo nudge pointed at a dead :3345. Re-proving with a content change so env-up runs again;
the shared-slot teardown is the gap to close next (a presented sibling card must not lose its variant
to another card's land).
