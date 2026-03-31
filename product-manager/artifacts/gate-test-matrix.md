# Gate Integration Test Matrix

## Test for each card type: create card → set state → attempt edit → verify gate behavior

| Type | Edit without git log | Edit without Chorus search | Demo without tests | Demo on chore | Expected |
|------|---------------------|---------------------------|-------------------|---------------|----------|
| fix | BLOCK "run git log" | BLOCK "search Chorus" | BLOCK "run tests" | — | All three gates fire |
| enhance | ALLOW | BLOCK "search Chorus" | BLOCK "run tests" | — | Chorus search + TDD |
| new | ALLOW (own domain) | ALLOW (own domain) | BLOCK "run tests" | — | TDD only in own domain |
| chore | ALLOW | ALLOW | SKIP (demo skip) | SKIP | Minimal gates |
| swat | ALLOW | ALLOW | ALLOW | — | Bypass all |

## Execution pattern per cell:
1. Create test card: `cards add "Gate test: <type>" --type <type> --owner kade -q`
2. Set role state: POST /api/chorus/role-state {role: kade, state: building, card: <id>, type: <type>}
3. Attempt the edit/demo action
4. Check hooks.log for BLOCK/ALLOW decision
5. Report PASS/FAIL
6. Clean up test card: `cards move <id> wont-do`
