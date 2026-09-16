# #3830 — the roles-side check, from Wren (ADR-054 authZ half)

Requirement (Silas's line 26): provisioning REFUSES a user that holds no role,
rather than minting a credential that will 403 on first use.

Measured 2026-09-15 13:10, canonical store:

    13 users · 5 hold a role · 8 hold NONE
      with role: crawler kade nightly silas wren
      without:   bridge chorus-sdk crawler-index embed-worker flow-probe
                 jeff marknakib reindex-worker

So 8 of 13 existing users would fail this check today. That is a finding, not a
blocker: the check governs NEW provisioning; the 8 are a backfill list.

## The query — the roles domain answers it, the security domain enforces it

    PREFIX c: <https://jeffbridwell.com/chorus#>
    ASK { GRAPH ?g { c:principal-NAME c:holdsRole ?r } }

FALSE → refuse the mint, name the user, say "holds no role".

## Negative proof shape

    positive:  principal-wren   → ASK true  → mint proceeds
    negative:  principal-bridge → ASK false → REFUSED, names bridge

Two separable states, and the refusal names which user.

## The backfill question, for Jeff

8 users hold no role, `principal-jeff` among them. Either they get roles, or the
check needs a stated exemption by kind. My position: they get roles. "What may
this user do" has an answer for every user, and an empty answer is itself a
role, not an absence.
