# Spec for Run: Generated Pulse API & #4638 Tree Shape (#4743 AC4)

## 1. Context & Architecture (The Run Stage)
Per Jeff's 2026-10-23 crawl/walk/run sequencing:
- **Crawl:** DAL atomic update and canonical writes (#4715–#4718).
- **Walk 1:** `athena-gen` routes `/pulse/<domain>/<collection>` and composed domains (#4742).
- **Walk 2:** Product radius proved and modeled through the door (#4743).
- **Run:** Full generated Pulse API from the model (#4744).

## 2. Tree Noun Architecture (#4638)
The tree models the operating awareness hierarchy:
```
Prompt (record: burst of lines from real actor joined within 5s)
  └── Role (derived state: moving / quiet + lastCallAt)
        ├── Turn (thoughts & stream events within session run)
        │     └── Note (working note / finding when uncarded)
        └── Card (the carrier of execution)
              ├── Pipeline / Run (startedByPrompt)
              ├── Verbs
              └── Note (onCard, inTurn)
```

### Invariant Rules
1. **Edges Point Up (#4302 Rule):**
   - `Message` points up to `Prompt` (`inPrompt`).
   - `Turn` points up to `SessionRun` (`turnOf`) and optionally `Prompt` (`answersPrompt`).
   - `Note` points up to `Turn` (`inTurn`) and optionally `Card` (`onCard`).
   - `PipelineRun` and `StreamEvent` point up to `Prompt` (`startedByPrompt`).
2. **Domain Residency:**
   - `Prompt`, `Turn`, and `Note` live in `urn:chorus:domains:awareness` homed under domain `awareness`.
3. **Immutability & Decay:**
   - `Prompt` is durable record.
   - `Turn` and `Note` are short-term memory (`rdfs:subClassOf chorus:ShortTermMemory`) decaying after turn completion.

## 3. Generated API Surface Spec (athena-gen / pulse-api)
The generated API (`com.chorus.pulse-api`, listening on port 3392) serves:
- **Owned Domains (Read / Write via Door):**
  - `/pulse/awareness/prompts`
  - `/pulse/awareness/turns`
  - `/pulse/awareness/notes`
  - `/pulse/awareness/rolestates`
  - `/pulse/awareness/readings`
  - `/pulse/awareness/heartbeats`
  - `/pulse/awareness/waitingons`
  - `/pulse/streams/events`
- **Composed Domains (Read-Only 405 on Write):**
  - `/pulse/cards/cards`
  - `/pulse/cards/acs`
  - `/pulse/messages/messages`
  - `/pulse/messages/channels`
- **Composite Tree Projection:**
  - `GET /pulse/tree` — top-level tree assembling Prompts → Roles → Turns/Cards.
  - `GET /pulse/tree/:role` — role slice projection injected into context envelope.
