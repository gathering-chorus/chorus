# Test-Value Policy (draft)

**Kade, 2026-04-18. #2173 AC2.** This is a role-local draft that moves to `loom-policies` (Wren's substrate, #2151) once the ontology-level policy format lands. The policy is binding on test authoring, test review, and gate-code / gate-quality decisions until superseded.

## The positive test

**A test is valuable if its failure would change Jeff's action.**

Change Jeff's action = he reverts a deploy, escalates, opens a card, re-runs a demo differently, or updates a decision. If a test can fail without Jeff noticing — or if its failure would not change what he does — the test is not valuable. Delete it, or replace it with one that is.

This is the load-bearing question. Every other rule below is a derived heuristic for predicting the answer without having to run the thought experiment every time.

## The three smell signals

When any of these are true about a test you're writing or reviewing, stop and ask the positive test.

### 1. The assertions are about mocks, not behavior

**Smell:** `expect(mockFuseki.query).toHaveBeenCalledWith(...)` or `expect(fakeFetcher).toHaveBeenCalledTimes(1)`.

**Why it's a smell:** you're proving that the test setup matched the test expectation. Both were written in the same PR, so they agree by construction. Rewrite the implementation in a way that calls a different SPARQL query but produces the same response, and this test still passes — which means it's not protecting behavior.

**Good shape:** assert on the return shape, status code, side effects Jeff sees. Inject the dep to avoid real I/O, but express the assertion in terms of the function's contract, not its wiring.

Concrete — from this session:
- `seenUrl = '...'` check in `codebase-topology.test.ts:calls the chorus-api upstream URL` **is borderline.** It asserts the URL we send. Kept because the URL is part of the observable contract (if we pointed at the wrong upstream, Jeff would see stale data). But it's close to the smell line.
- `expect(seenInit?.signal).toBeInstanceOf(AbortSignal)` **is smellier.** It asserts we pass *some* AbortSignal without checking the 8s timeout actually protects the user. Leave in place for now; flag for replacement with a "slow upstream maps to 502 within N seconds" behavior test when we have a clock fake.

### 2. The test survives rewriting the implementation with a different algorithm

**Smell:** I could rewrite the function's internals — swap data structures, change the loop shape, reorder calls — and the test still passes without edits, even though the *observable* behavior differs.

**Why it's a smell:** the test is testing implementation detail, not contract. Implementations change; contracts don't.

**Good shape:** write the test in terms of input → observable output. Let the function implementation change freely as long as the contract holds.

Concrete — from this session:
- `process_error_streams_increments_count_on_repeat_pattern` (chorus-hooks): **passes this test** — it specifies that the same pattern seen twice produces count=2, which is a contract Jeff relies on (dedup).
- `execute_card_action_owner_routes_silas_for_infra_source` (chorus-hooks, dropped from #2167): **fails this test** — it asserts `--owner=Silas` appears in argv. If we changed owner routing to a separate mapping file, the behavior stays correct but this test breaks. It was written to hit a coverage line, not to protect Jeff.

### 3. The test name doesn't describe behavior Jeff would recognize

**Smell:** `processes input correctly`, `returns expected shape`, `handles the edge case`, `test 1`, `does_status_runs_against_empty_state_without_panicking`.

**Why it's a smell:** the name can't survive being read by Jeff six months from now. "Does the test still describe a thing I care about?" is a real maintenance question. Names that don't answer it rot.

**Good shape:** the name reads like a clause of a spec. "GET /health returns 200 and status:ok" is legible to Jeff. "upstream 503 passes through status + error body" is legible. "role with no card clears card from previous building state" is legible.

Concrete — from this session:
- `GET /health returns 200 and {status: ok}` (in-process-harness): **passes.**
- `fetcher throws (network error) maps to 502` (codebase-topology): **passes.**
- `do_status_runs_against_empty_state_without_panicking` (chorus-hooks, session #2167): **fails.** This tests that a function didn't panic — which is compiler-level, not behavior-level. A real behavior test would describe what `do_status` *produces* when state is empty, not that it survives.

## Where the policy bites

Three places, in increasing severity:

### At write time

Every test you write, run the positive test against it before committing. If the answer is "no, this failure wouldn't change Jeff's action," either rewrite it or don't write it. Adding a test that fails this filter makes the suite *less* trustworthy, not more — because it trains reviewers to assume tests are coverage theater.

### At gate-code time

When a role submits work for `gate-code`, the reviewer (Kade) spot-checks the new tests against the three smells. A single smelly test is not a block. A pattern of smelly tests — added to hit a coverage floor — is a block, and the card bounces with "the coverage number went up but the protection didn't."

### At demo-preflight

A failing test is not evidence of protection. A *passing* test might be. Demo-preflight should surface: new tests added this card, and for each, which outcome they protect. If the builder can't articulate the outcome in one sentence, the test probably shouldn't exist.

## Anti-cases — things this policy does NOT say

- It does not say unit tests are less valuable than integration tests. Unit tests that pass the positive test are first-class. Integration tests that fail the positive test are garbage.
- It does not say "don't mock." Mocks are fine *as setup* — the problem is mocks *as subjects of assertion*. `fetcher = () => mockResponse(503, {})` is setup; `expect(fetcher).toHaveBeenCalled()` is assertion.
- It does not say "every test must have an E2E counterpart." The pyramid shape belongs in the Quality service design, not here. This policy is about the value of each test at its own layer.
- It does not say "coverage doesn't matter." Coverage is a proxy. When the proxy diverges from the thing it proxies (protection), trust the thing, update the proxy.

## Observed cost, 2026-04-17

On #2167, 170 tests were shipped across chorus-inject + chorus-hooks + ops.rs. Applying this policy retroactively:

- ~40 pass the positive test (test names describe behavior, assertions are on outputs, rewriting the implementation would break them only if the behavior changed). Example: `process_error_streams_skips_non_error_log_levels`.
- ~90 are borderline — they test wiring-plus-output. Example: `execute_card_action_parses_new_card_id_and_updates_state`. Kept; flagged for review if they break under refactor.
- ~40 fail the positive test outright — they test regex matches in argv, that `do_status` doesn't panic, that `compile_false_positives` returns the right count. These were coverage-farming. They stay for now (deletion is a bigger card), but they don't get cited as evidence of protection.

Coverage went up 20 points. Protection went up less. That's the gap this policy is here to close.

## Out of scope

- Mutation testing, fault injection, chaos tests — those are advanced tools that augment but don't replace this policy. If they land later, they sharpen the "would this failure change Jeff's action?" question empirically.
- Test runtime budgets. Slow tests are a DX problem; unvaluable tests are a quality problem. Different card.
- What to do about the 40 coverage-farming tests from #2167. Follow-on if we want to pay the cleanup debt; otherwise they stay as documented technical debt against the service design (Gap 1).

## Status

DRAFT. Reviewable by Wren for gate-product on #2173 AC2. Moves to `loom-policies` sub-domain when the ontology-level policy shape lands (currently tracked in #2151).
