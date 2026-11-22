// @domain: domains — placed by chorus-crawl's domain rules (#4538)
//! #4517 AC3 (Jeff 11-18 00:17) — a PATCH whose body is exactly `{"ownedBy": <caller>}` on a
//! row with no owner is a claim, not an edit: the graph's Write holder names itself, through
//! the DAL's `claim-owner` (absent checked in the same update, owner read back).

/// What a write body says about a claim. `None`: not a claim, the normal path decides.
pub fn claim_body(body: &str, caller: &str) -> Option<Result<(), (u16, String)>> {
    let v: serde_json::Value = serde_json::from_str(body).ok()?;
    let obj = v.as_object()?;
    let owner = obj.get("ownedBy")?;
    if obj.len() != 1 {
        return Some(Err((422, "a claim carries ownedBy alone; send other changes as their own write".into())));
    }
    let named = owner.as_str().unwrap_or("");
    let bare = named.rsplit('#').next().unwrap_or(named).trim_start_matches("principal-");
    if bare != caller || caller.is_empty() {
        return Some(Err((403, format!("a claim names its caller only: '{named}' is not {caller}"))));
    }
    Some(Ok(()))
}

/// The DAL's answer to `claim-owner`, as the door's status.
pub fn claim_status(dal: Result<(), String>) -> (u16, &'static str) {
    match dal {
        Ok(()) => (200, "claimed"),
        Err(e) if e.contains("claim-conflict") => (409, "conflict"),
        Err(e) if e.contains("claim-needs-write") => (403, "authz"),
        Err(e) if e.contains("claim-unconfirmed") => (404, "not-found"),
        Err(_) => (503, "unavailable"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exactly_the_callers_own_name_is_a_claim() {
        assert_eq!(claim_body(r#"{"ownedBy":"kade"}"#, "kade"), Some(Ok(())));
        assert_eq!(claim_body(r#"{"ownedBy":"principal-kade"}"#, "kade"), Some(Ok(())));
        assert_eq!(claim_body(r#"{"comment":"x"}"#, "kade"), None, "not a claim: the normal path decides");
    }

    /// NEGATIVE PROOFS (Silas 00:28): an extra field is 422; naming anyone else is 403.
    #[test]
    fn negative_proofs_extra_fields_and_other_owners_are_refused() {
        assert_eq!(claim_body(r#"{"ownedBy":"kade","comment":"x"}"#, "kade").unwrap().unwrap_err().0, 422);
        assert_eq!(claim_body(r#"{"ownedBy":"silas"}"#, "kade").unwrap().unwrap_err().0, 403);
        assert_eq!(claim_body(r#"{"ownedBy":"kade"}"#, "").unwrap().unwrap_err().0, 403);
    }

    #[test]
    fn the_dal_answer_maps_to_one_status() {
        assert_eq!(claim_status(Ok(())).0, 200);
        assert_eq!(claim_status(Err("claim-conflict: owned by silas".into())).0, 409);
        assert_eq!(claim_status(Err("claim-needs-write: …".into())).0, 403);
        assert_eq!(claim_status(Err("claim-unconfirmed: …".into())).0, 404);
        assert_eq!(claim_status(Err("curl: timeout".into())).0, 503);
    }
}
