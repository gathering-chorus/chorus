//! #3356 — query/webid helpers the verifier uses, RELOCATED verbatim from owl-api
//! lib.rs (behavior-preserving; owl-api re-exports these). select_v = SPARQL-JSON
//! single-var parser; role_from_webid = the webid→role string parse (ADR-054 will
//! later swap the CONSUMER to a graph query — this move changes NO logic).

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

pub fn role_from_webid(web_id: &str) -> Option<String> {
    if let Some(after) = web_id.split("/_agents/").nth(1) {
        let role = after.split('/').next()?;
        return if role.is_empty() { None } else { Some(role.to_string()) };
    }
    // CSS pod shape: scheme://host/<name>/profile/card#me (no _agents, no .ttl)
    let rest = web_id.split("://").nth(1)?;
    let mut segs = rest.split('/');
    let _host = segs.next()?;
    let name = segs.next()?;
    if segs.next()? == "profile" && segs.next()?.starts_with("card") && !name.is_empty() {
        return Some(name.to_string());
    }
    None
}
