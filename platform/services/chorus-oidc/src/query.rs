//! #3356 — query/webid helpers the verifier uses, RELOCATED verbatim from athena-make
//! lib.rs (behavior-preserving; athena-make re-exports these). select_v = SPARQL-JSON
//! single-var parser. The webid→role string parse that lived beside it is GONE
//! as of #3688 — ADR-054 §3.3 swapped the consumer to a `chorus:holdsRole`
//! graph query (see `oidc::PRINCIPAL_ROLE_QUERY`).

pub fn select_v(body: &str) -> Vec<String> {
    let mut vals = Vec::new();
    for chunk in body.split("\"v\"").skip(1) {
        if let Some(i) = chunk.find("\"value\"") {
            let rest = &chunk[i + 7..];
            if let Some(start) = rest.find('"') {
                let rest = &rest[start + 1..];
                if let Some(raw) = scan_json_string(rest) {
                    vals.push(json_unescape(raw));
                }
            }
        }
    }
    vals
}

/// Slice up to the closing quote of a JSON string, honoring backslash escapes.
fn scan_json_string(rest: &str) -> Option<&str> {
    let bytes = rest.as_bytes();
    let mut esc = false;
    for (i, &c) in bytes.iter().enumerate() {
        if esc {
            esc = false;
        } else if c == b'\\' {
            esc = true;
        } else if c == b'"' {
            return Some(&rest[..i]);
        }
    }
    None
}

/// Decode JSON string escapes (zero-dep): \" \\ \/ \n \r \t \uXXXX incl.
/// surrogate pairs. Unknown escapes pass through verbatim rather than erroring —
/// a read path must not refuse data it can still show.
fn json_unescape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some('u') => {
                let hex: String = chars.by_ref().take(4).collect();
                match u32::from_str_radix(&hex, 16) {
                    Ok(cp) if (0xD800..0xDC00).contains(&cp) => {
                        // high surrogate — pair with the following \uXXXX low half
                        let mut ahead = chars.clone();
                        let paired = (ahead.next() == Some('\\') && ahead.next() == Some('u'))
                            .then(|| ahead.by_ref().take(4).collect::<String>())
                            .and_then(|h2| u32::from_str_radix(&h2, 16).ok())
                            .filter(|lo| (0xDC00..0xE000).contains(lo))
                            .and_then(|lo| char::from_u32(0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00)));
                        if let Some(ch) = paired {
                            out.push(ch);
                            chars = ahead;
                        } else {
                            out.push('\u{FFFD}');
                        }
                    }
                    Ok(cp) => out.push(char::from_u32(cp).unwrap_or('\u{FFFD}')),
                    Err(_) => {
                        out.push('\\');
                        out.push('u');
                        out.push_str(&hex);
                    }
                }
            }
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
}

// #3688 — `role_from_webid` (the webid→role STRING parse) is retired here.
// ADR-054 §3.3: role is now ASKED of the graph via `chorus:holdsRole`
// (oidc::PRINCIPAL_ROLE_QUERY / OidcVerifier::role_for). There is no fallback
// parser: a naming convention that can disagree with the model is exactly the
// thing the edge query replaces.
