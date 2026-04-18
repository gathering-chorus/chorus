//! Test quality gate (#2196)
//!
//! PreToolUse on Write of `*.test.ts` / `*.test.js` / `*.spec.ts`:
//! rejects the write if any `test()` / `it()` block lacks both
//!   (a) an invocation of an imported production symbol AND
//!   (b) an assertion mechanism (expect, .rejects, .resolves,
//!        toMatchSnapshot, .toThrow, throw-assertion pattern).
//!
//! Rationale (#2196 evidence): current tdd_gate.rs enforces temporal
//! ordering (tests before code, tests run before demo) but never reads
//! test content. Roles have been shipping assertion-free tests that
//! clear the gate — status=0-as-pass, boot-and-exit smoke harnesses,
//! scaffold boilerplate that invokes no production code. Blocks honest
//! coverage-lift work (#2205): coverage numbers are meaningless while
//! the gate accepts "tests" that don't test.
//!
//! Grandfather: Edit on an existing file is exempt. Enforcement fires
//! on Write (new file creation) only. This matches AC: "No change to
//! existing passing tests — grandfather clause is fine, enforcement on
//! new files only."
//!
//! Skip for chore/swat/fix — reactive work, same pattern as tdd_gate.

use crate::types::{permission_deny_json, HookInput, HookResponse};
use std::collections::HashSet;

const GOOD_EXAMPLE: &str = "platform/api/tests/handlers/athena-validate.test.ts";

/// A single test() / it() block extracted from source.
#[derive(Debug)]
struct TestBlock {
    name: String,
    body: String,
}

/// Parse `import { a, b } from '...'` / `import * as ns from '...'` /
/// `import def from '...'` and return (identifiers, source_path) pairs.
fn extract_imports(source: &str) -> Vec<(Vec<String>, String)> {
    let mut out = Vec::new();
    // Collapse multi-line imports for easier regex.
    let collapsed = source.replace('\n', " ");
    // Match: import <clause> from '<path>'; — clause is greedy up to `from`
    let re = regex::Regex::new(
        r#"import\s+(?:type\s+)?(\{[^}]*\}|\*\s+as\s+\w+|\w+(?:\s*,\s*\{[^}]*\})?)\s+from\s+['"]([^'"]+)['"]"#,
    )
    .unwrap();
    for cap in re.captures_iter(&collapsed) {
        let clause = &cap[1];
        let path = cap[2].to_string();
        let mut idents = Vec::new();
        if let Some(inner) = clause.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            for piece in inner.split(',') {
                let name = piece.split(" as ").last().unwrap_or(piece).trim();
                if !name.is_empty() {
                    idents.push(name.to_string());
                }
            }
        } else if let Some(rest) = clause.strip_prefix("* as ") {
            idents.push(rest.trim().to_string());
        } else {
            // Default or `default, { … }` — take the leading identifier.
            let head = clause.split(|c: char| c == ',' || c == '{').next().unwrap_or("");
            let name = head.trim();
            if !name.is_empty() {
                idents.push(name.to_string());
            }
            // Also handle trailing `{ … }` in the same clause.
            if let Some(start) = clause.find('{') {
                if let Some(end) = clause[start..].find('}') {
                    let inner = &clause[start + 1..start + end];
                    for piece in inner.split(',') {
                        let name = piece.split(" as ").last().unwrap_or(piece).trim();
                        if !name.is_empty() {
                            idents.push(name.to_string());
                        }
                    }
                }
            }
        }
        out.push((idents, path));
    }
    out
}

/// Treat an import as production iff its path is relative (./ or ../),
/// does not point at a test/spec/fixture/helper sibling, and isn't a
/// bare node/jest/test-utility module.
fn is_production_import_path(path: &str) -> bool {
    if !path.starts_with("./") && !path.starts_with("../") {
        return false;
    }
    let lower = path.to_lowercase();
    if lower.contains(".test")
        || lower.contains(".spec")
        || lower.contains("/fixtures")
        || lower.contains("/fixture")
        || lower.contains("/helpers")
        || lower.contains("/helper")
        || lower.contains("/mocks")
        || lower.contains("/__mocks__")
    {
        return false;
    }
    true
}

/// Collect the identifiers that came from production imports.
fn production_symbols(source: &str) -> HashSet<String> {
    let mut set = HashSet::new();
    for (idents, path) in extract_imports(source) {
        if is_production_import_path(&path) {
            for id in idents {
                set.insert(id);
            }
        }
    }
    set
}

/// Scan source for test() / it() / test.skip(…) / it.only(…) blocks
/// and return each body paired with its name string. Uses brace matching
/// from the opening `=>` or `function(` body start.
fn extract_test_blocks(source: &str) -> Vec<TestBlock> {
    let mut blocks = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Look for `test(` / `test.skip(` / `test.only(` / `it(` / `it.skip(` / `it.only(`
        let rest = &source[i..];
        let found = ["test(", "test.skip(", "test.only(", "it(", "it.skip(", "it.only("]
            .iter()
            .filter_map(|kw| rest.find(kw).map(|pos| (pos, kw.len())))
            .min_by_key(|(pos, _)| *pos);
        let (rel_pos, kw_len) = match found {
            Some(x) => x,
            None => break,
        };
        // Ensure the keyword boundary is a non-identifier char (avoid matching `mytest(`)
        let abs = i + rel_pos;
        if abs > 0 {
            let prev = bytes[abs - 1];
            if prev.is_ascii_alphanumeric() || prev == b'_' || prev == b'$' {
                i = abs + kw_len;
                continue;
            }
        }
        let after_paren = abs + kw_len;
        // Extract name literal (first '...' or "..." or `...`)
        let name_slice = &source[after_paren..];
        let name = extract_first_string(name_slice).unwrap_or_default();
        // Find the arrow or function-body opening `{` after the name argument
        let body_open = match find_body_open(source, after_paren) {
            Some(p) => p,
            None => {
                i = after_paren;
                continue;
            }
        };
        let body_close = match match_brace(source, body_open) {
            Some(p) => p,
            None => {
                i = after_paren;
                continue;
            }
        };
        let body = source[body_open + 1..body_close].to_string();
        blocks.push(TestBlock { name, body });
        i = body_close + 1;
    }
    blocks
}

fn extract_first_string(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
            i += 1;
            continue;
        }
        if c == b'\'' || c == b'"' || c == b'`' {
            let quote = c;
            let start = i + 1;
            let mut j = start;
            while j < bytes.len() {
                if bytes[j] == b'\\' {
                    j += 2;
                    continue;
                }
                if bytes[j] == quote {
                    return Some(s[start..j].to_string());
                }
                j += 1;
            }
            return None;
        }
        return None;
    }
    None
}

/// Given a position after `(`, find the opening `{` of the callback body.
fn find_body_open(source: &str, start: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut i = start;
    let mut depth = 1; // we are inside the test( … ) paren group
    while i < bytes.len() && depth > 0 {
        match bytes[i] {
            b'(' => depth += 1,
            b')' => {
                depth -= 1;
                if depth == 0 {
                    return None; // no body in the call args
                }
            }
            b'{' if depth == 1 => {
                // This `{` is inside the args of test(...) — this is the callback body.
                return Some(i);
            }
            b'=' if depth == 1 && i + 1 < bytes.len() && bytes[i + 1] == b'>' => {
                // Arrow function: scan ahead past whitespace for `{`
                let mut j = i + 2;
                while j < bytes.len() && (bytes[j] == b' ' || bytes[j] == b'\t' || bytes[j] == b'\n' || bytes[j] == b'\r') {
                    j += 1;
                }
                if j < bytes.len() && bytes[j] == b'{' {
                    return Some(j);
                }
                // Concise arrow body (no braces) — treat as bodyless, skip.
                return None;
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn match_brace(source: &str, open: usize) -> Option<usize> {
    let bytes = source.as_bytes();
    let mut depth = 0;
    let mut i = open;
    let mut in_str: Option<u8> = None;
    let mut in_line_comment = false;
    let mut in_block_comment = false;
    while i < bytes.len() {
        let c = bytes[i];
        if in_line_comment {
            if c == b'\n' {
                in_line_comment = false;
            }
            i += 1;
            continue;
        }
        if in_block_comment {
            if c == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
                in_block_comment = false;
                i += 2;
                continue;
            }
            i += 1;
            continue;
        }
        if let Some(q) = in_str {
            if c == b'\\' {
                i += 2;
                continue;
            }
            if c == q {
                in_str = None;
            }
            i += 1;
            continue;
        }
        match c {
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'/' => {
                in_line_comment = true;
                i += 2;
                continue;
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                in_block_comment = true;
                i += 2;
                continue;
            }
            b'\'' | b'"' | b'`' => in_str = Some(c),
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(i);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn block_has_assertion(body: &str) -> bool {
    // Common assertion mechanisms.
    let patterns = [
        "expect(",
        ".rejects",
        ".resolves",
        "toMatchSnapshot",
        ".toThrow",
        "assert(",
        "assert.",
    ];
    if patterns.iter().any(|p| body.contains(p)) {
        return true;
    }
    // Explicit throw-assertion pattern: a `throw new Error(` inside a try {...}
    // catch (followed by some assertion) OR an explicit `fail(` call.
    if body.contains("fail(") {
        return true;
    }
    if body.contains("try {") && body.contains("throw ") && body.contains("catch") {
        return true;
    }
    false
}

fn block_calls_production_symbol(body: &str, prod: &HashSet<String>) -> bool {
    if prod.is_empty() {
        return false;
    }
    for name in prod {
        // invocation: `Name(` or `Name.something(` or `new Name(`
        let call = format!("{}(", name);
        let dot = format!("{}.", name);
        let newcall = format!("new {}(", name);
        if body.contains(&call) || body.contains(&dot) || body.contains(&newcall) {
            return true;
        }
    }
    false
}

/// Count how many `test(` / `it(` keyword-boundary invocations appear in source.
/// Used by parser diagnostics (#2210): if the source clearly contains test
/// keywords but extract_test_blocks() returned zero blocks, the parser hit a
/// malformed region and we must fail closed rather than silently allow.
fn count_test_keyword_occurrences(source: &str) -> usize {
    let bytes = source.as_bytes();
    let kws = ["test(", "test.skip(", "test.only(", "it(", "it.skip(", "it.only("];
    let mut count = 0;
    for kw in &kws {
        let mut pos = 0;
        while let Some(rel) = source[pos..].find(kw) {
            let abs = pos + rel;
            let is_boundary = abs == 0 || {
                let prev = bytes[abs - 1];
                !prev.is_ascii_alphanumeric() && prev != b'_' && prev != b'$'
            };
            if is_boundary {
                count += 1;
            }
            pos = abs + kw.len();
        }
    }
    count
}

/// Normalize a test block body for grandfather signature comparison. Strips
/// all whitespace so that reformatting an existing block doesn't un-grandfather
/// it. Name + normalized body forms the signature.
fn block_signature(block: &TestBlock) -> String {
    let mut normalized = String::with_capacity(block.body.len());
    for c in block.body.chars() {
        if !c.is_whitespace() {
            normalized.push(c);
        }
    }
    format!("{}|{}", block.name, normalized)
}

/// Backwards-compat single-source analyse — treats the whole source as newly
/// introduced (no grandfathered blocks). Used by the Write-on-new-file path and
/// by the existing unit tests from #2196.
pub(crate) fn analyse(source: &str) -> Result<(), String> {
    analyse_incoming("", source)
}

/// Diff-aware analyse (#2210). Compares the existing content to the incoming
/// content at the test-block level. A block in `incoming` is subject to the
/// quality check only if its signature is NOT already present in `existing`
/// — grandfathered blocks stay grandfathered, newly introduced blocks must
/// pass.
///
/// Fails closed on parser malformation: if `incoming` clearly contains
/// `test(` / `it(` keyword text but the parser extracted fewer blocks than
/// keywords, reject the write with a parser-error message rather than
/// silently allowing.
pub(crate) fn analyse_incoming(existing: &str, incoming: &str) -> Result<(), String> {
    let incoming_blocks = extract_test_blocks(incoming);
    let keyword_count = count_test_keyword_occurrences(incoming);

    // Parser-diagnostic: if we see test-keyword text but can't parse matching
    // blocks, fail closed.
    if keyword_count > incoming_blocks.len() {
        return Err(format!(
            "could not parse {} test-keyword occurrence(s) — only {} block(s) matched. \
             Run `npx tsc --noEmit` first; gate fails closed on malformed test sources.",
            keyword_count, incoming_blocks.len()
        ));
    }

    if incoming_blocks.is_empty() {
        return Ok(());
    }

    // Build grandfather signature set from existing content.
    let existing_sigs: std::collections::HashSet<String> = extract_test_blocks(existing)
        .iter()
        .map(block_signature)
        .collect();

    let prod = production_symbols(incoming);
    for block in &incoming_blocks {
        // Grandfathered — block is unchanged from existing content, skip check.
        if existing_sigs.contains(&block_signature(block)) {
            continue;
        }
        let has_assert = block_has_assertion(&block.body);
        let has_call = block_calls_production_symbol(&block.body, &prod);
        if !has_assert || !has_call {
            let why = match (has_assert, has_call) {
                (false, false) => "no assertion mechanism AND no invocation of any imported production symbol",
                (true, false) => "no invocation of any imported production symbol",
                (false, true) => "no assertion mechanism (expect/.rejects/.resolves/.toThrow/toMatchSnapshot)",
                (true, true) => unreachable!(),
            };
            return Err(format!(
                "test(\"{}\") {} — see {} for a well-shaped example",
                block.name, why, GOOD_EXAMPLE
            ));
        }
    }
    Ok(())
}

/// Detect whether the file path looks like a test file we should gate on.
fn is_test_file_path(path: &str) -> bool {
    let lower = path.to_lowercase();
    lower.ends_with(".test.ts")
        || lower.ends_with(".test.tsx")
        || lower.ends_with(".test.js")
        || lower.ends_with(".spec.ts")
        || lower.ends_with(".spec.js")
}

/// For a Write: return (path, existing_content, incoming_content).
/// For an Edit: reconstruct incoming by applying old_string → new_string to
/// the on-disk content, and return the same triple.
///
/// Returns None if the tool call isn't a test-file Write/Edit, or if the
/// reconstruction isn't possible (Edit referencing a file that doesn't exist,
/// or old_string not found — let other hooks surface those errors).
fn resolve_test_file_change(input: &HookInput) -> Option<(String, String, String)> {
    let tool = input.tool_name_str();
    let file_path = input.get_tool_input_str("file_path");
    if !is_test_file_path(&file_path) {
        return None;
    }
    let existing = std::fs::read_to_string(&file_path).unwrap_or_default();

    match &*tool {
        "Write" => {
            let incoming = input.get_tool_input_str("content");
            if incoming.is_empty() {
                return None;
            }
            Some((file_path, existing, incoming))
        }
        "Edit" => {
            let old_s = input.get_tool_input_str("old_string");
            let new_s = input.get_tool_input_str("new_string");
            if old_s.is_empty() || existing.is_empty() {
                return None;
            }
            // Apply the edit. Single replacement — matches Edit tool semantics
            // (the Edit tool errors on non-unique old_string, so the first
            // occurrence is the only one).
            let incoming = if let Some(pos) = existing.find(&old_s) {
                let mut buf = String::with_capacity(existing.len() - old_s.len() + new_s.len());
                buf.push_str(&existing[..pos]);
                buf.push_str(&new_s);
                buf.push_str(&existing[pos + old_s.len()..]);
                buf
            } else {
                // Edit wouldn't apply — let the Edit tool report it; don't gate.
                return None;
            };
            Some((file_path, existing, incoming))
        }
        _ => None,
    }
}

pub fn check(input: &HookInput) -> HookResponse {
    // Mirror tdd_gate: chore/swat cards skip.
    let card_type = crate::types::card_type_for_role(input.role().as_str());
    if card_type == "chore" || card_type == "swat" {
        return HookResponse::allow();
    }

    let (path, existing, incoming) = match resolve_test_file_change(input) {
        Some(triple) => triple,
        None => return HookResponse::allow(),
    };
    match analyse_incoming(&existing, &incoming) {
        Ok(()) => HookResponse::allow(),
        Err(reason) => {
            let fname = path.rsplit('/').next().unwrap_or(&path);
            let msg = format!(
                "Test quality gate (#2196/#2210): {} — {}. DEC-1674: tests exercise real behavior, not presence.",
                fname, reason
            );
            HookResponse::deny(&permission_deny_json(&msg))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- parser ---

    #[test]
    fn extracts_named_imports() {
        let src = r#"import { fetchDomain, cache } from '../src/handlers/domain';"#;
        let imports = extract_imports(src);
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0].0, vec!["fetchDomain", "cache"]);
        assert_eq!(imports[0].1, "../src/handlers/domain");
    }

    #[test]
    fn extracts_star_import() {
        let src = r#"import * as handler from '../src/handler';"#;
        let imports = extract_imports(src);
        assert_eq!(imports[0].0, vec!["handler"]);
    }

    #[test]
    fn extracts_default_import() {
        let src = r#"import build from '../src/build';"#;
        let imports = extract_imports(src);
        assert_eq!(imports[0].0, vec!["build"]);
    }

    #[test]
    fn production_import_path_recognises_relative_src() {
        assert!(is_production_import_path("../src/foo"));
        assert!(is_production_import_path("./sibling"));
        assert!(!is_production_import_path("jest"));
        assert!(!is_production_import_path("fs"));
        assert!(!is_production_import_path("../fixtures/data"));
        assert!(!is_production_import_path("./helpers/setup"));
        assert!(!is_production_import_path("../mocks/db"));
        assert!(!is_production_import_path("../src/foo.test"));
    }

    #[test]
    fn collects_only_production_symbols() {
        let src = r#"
            import { realFn } from '../src/real';
            import { describe, it, expect } from 'jest';
            import { helper } from './helpers/util';
        "#;
        let prod = production_symbols(src);
        assert!(prod.contains("realFn"));
        assert!(!prod.contains("describe"));
        assert!(!prod.contains("expect"));
        assert!(!prod.contains("helper"));
    }

    #[test]
    fn extracts_test_blocks_by_name() {
        let src = r#"
            test('does the thing', () => {
                expect(1).toBe(1);
            });
            it('also works', async () => {
                await Promise.resolve();
                expect(true).toBe(true);
            });
        "#;
        let blocks = extract_test_blocks(src);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].name, "does the thing");
        assert_eq!(blocks[1].name, "also works");
    }

    #[test]
    fn ignores_testfoo_identifiers() {
        let src = r#"const mytest = 1; testing('x', () => { expect(1).toBe(1); });"#;
        let blocks = extract_test_blocks(src);
        assert!(blocks.is_empty());
    }

    #[test]
    fn handles_skip_only_variants() {
        let src = r#"
            test.skip('skipped', () => { expect(1).toBe(1); });
            it.only('focused', () => { expect(2).toBe(2); });
        "#;
        let blocks = extract_test_blocks(src);
        assert_eq!(blocks.len(), 2);
    }

    // --- block-level checks ---

    #[test]
    fn detects_expect_assertion() {
        assert!(block_has_assertion("expect(x).toBe(1);"));
    }

    #[test]
    fn detects_rejects_assertion() {
        assert!(block_has_assertion("await expect(fn()).rejects.toThrow();"));
    }

    #[test]
    fn detects_tomatchsnapshot() {
        assert!(block_has_assertion("expect(x).toMatchSnapshot();"));
    }

    #[test]
    fn rejects_assertion_free_body() {
        assert!(!block_has_assertion("const x = 1; fn(); return;"));
    }

    #[test]
    fn detects_production_call() {
        let mut prod = HashSet::new();
        prod.insert("realFn".to_string());
        assert!(block_calls_production_symbol("realFn(arg);", &prod));
        assert!(block_calls_production_symbol("new realFn();", &prod));
        assert!(block_calls_production_symbol("realFn.property", &prod));
        assert!(!block_calls_production_symbol("otherFn();", &prod));
    }

    // --- full analyse() ---

    #[test]
    fn golden_good_test_passes() {
        let src = r#"
            import { realFn } from '../src/real';
            test('real behavior', () => {
                const r = realFn(1);
                expect(r).toBe(2);
            });
        "#;
        assert!(analyse(src).is_ok());
    }

    #[test]
    fn golden_assertion_free_test_fails() {
        // This file would pass the old tdd_gate: tests exist, tests would "pass"
        // because there is nothing to fail. The new gate rejects it.
        let src = r#"
            import { realFn } from '../src/real';
            test('smoke only', () => {
                realFn(1);
            });
        "#;
        let err = analyse(src).unwrap_err();
        assert!(err.contains("no assertion mechanism"));
    }

    #[test]
    fn golden_no_production_call_fails() {
        let src = r#"
            import { realFn } from '../src/real';
            test('touches nothing', () => {
                expect(1 + 1).toBe(2);
            });
        "#;
        let err = analyse(src).unwrap_err();
        assert!(err.contains("no invocation of any imported production symbol"));
    }

    #[test]
    fn only_jest_imports_counts_as_no_production() {
        let src = r#"
            import { describe, it, expect } from 'jest';
            test('pure math', () => {
                expect(1 + 1).toBe(2);
            });
        "#;
        let err = analyse(src).unwrap_err();
        assert!(err.contains("production symbol"));
    }

    #[test]
    fn status_zero_as_pass_is_rejected() {
        // Shape of the evidence from card #2167: exec-returns-zero accepted as pass,
        // no shape assertion on the handler output.
        let src = r#"
            import { handler } from '../src/handler';
            test('server boots', () => {
                const result = handler();
                if (result.status !== 0) { /* nothing */ }
            });
        "#;
        let err = analyse(src).unwrap_err();
        assert!(err.contains("no assertion mechanism"));
    }

    #[test]
    fn pointer_to_good_example_appears_in_error() {
        let src = r#"
            import { realFn } from '../src/real';
            test('bad', () => { realFn(); });
        "#;
        let err = analyse(src).unwrap_err();
        assert!(err.contains("athena-validate.test.ts"));
    }

    #[test]
    fn file_with_no_test_blocks_is_ignored() {
        let src = r#"
            import { realFn } from '../src/real';
            export const fixture = { x: 1 };
        "#;
        assert!(analyse(src).is_ok());
    }

    #[test]
    fn multiple_blocks_any_bad_fails() {
        let src = r#"
            import { realFn } from '../src/real';
            test('good', () => {
                expect(realFn()).toBe(1);
            });
            test('bad', () => {
                realFn();
            });
        "#;
        let err = analyse(src).unwrap_err();
        assert!(err.contains("bad"));
    }

    // --- #2210: diff-aware analyse_incoming ---

    #[test]
    fn diff_grandfathers_existing_bad_block() {
        let existing = r#"
            import { realFn } from '../src/real';
            test('pre-existing bad', () => {
                realFn();
            });
        "#;
        // Incoming is the same content — no new blocks. Grandfathered.
        assert!(analyse_incoming(existing, existing).is_ok());
    }

    #[test]
    fn diff_rejects_newly_added_bad_block() {
        let existing = r#"
            import { realFn } from '../src/real';
            test('pre-existing good', () => {
                expect(realFn()).toBe(1);
            });
        "#;
        let incoming = r#"
            import { realFn } from '../src/real';
            test('pre-existing good', () => {
                expect(realFn()).toBe(1);
            });
            test('newly added bad', () => {
                realFn();
            });
        "#;
        let err = analyse_incoming(existing, incoming).unwrap_err();
        assert!(err.contains("newly added bad"));
    }

    #[test]
    fn diff_allows_newly_added_good_block() {
        let existing = r#"
            import { realFn } from '../src/real';
            test('original', () => {
                expect(realFn()).toBe(1);
            });
        "#;
        let incoming = r#"
            import { realFn } from '../src/real';
            test('original', () => {
                expect(realFn()).toBe(1);
            });
            test('added good', () => {
                expect(realFn()).toBe(2);
            });
        "#;
        assert!(analyse_incoming(existing, incoming).is_ok());
    }

    #[test]
    fn partial_write_race_rejects_followup_bad_block() {
        // Simulates the Silas-flagged race: Write creates an empty/stub file
        // (no test blocks), then a subsequent Edit introduces bad tests.
        let existing_empty_stub = "// placeholder — tests forthcoming\n";
        let incoming_bad = r#"
            import { realFn } from '../src/real';
            test('smoke only', () => {
                realFn();
            });
        "#;
        let err = analyse_incoming(existing_empty_stub, incoming_bad).unwrap_err();
        assert!(err.contains("no assertion"));
    }

    #[test]
    fn grandfather_tolerates_whitespace_reformatting() {
        let existing = r#"
            import { realFn } from '../src/real';
            test('legacy', () => {
                realFn();
            });
        "#;
        // Same block, reformatted (collapsed whitespace) — still grandfathered.
        let incoming = r#"
            import { realFn } from '../src/real';
            test('legacy', () => { realFn(); });
        "#;
        assert!(analyse_incoming(existing, incoming).is_ok());
    }

    #[test]
    fn parser_fails_closed_on_unmatched_braces() {
        // Unbalanced braces: `test(` text present but the body never closes,
        // so extract_test_blocks returns zero. Must fail closed.
        let src = "import { realFn } from '../src/real';\ntest('broken', () => { realFn();\n";
        let err = analyse(src).unwrap_err();
        assert!(err.contains("could not parse"));
    }

    #[test]
    fn parser_fails_closed_on_bodyless_it() {
        // `it(` keyword with no callback body — parser finds the keyword but
        // can't extract a block. Malformed; must fail closed.
        let src = r#"
            import { realFn } from '../src/real';
            it('stub',
        "#;
        let err = analyse(src).unwrap_err();
        assert!(err.contains("could not parse"));
    }

    #[test]
    fn parser_diagnostic_pointer_to_tsc() {
        let src = "test('x', () => { oops\n";
        let err = analyse(src).unwrap_err();
        assert!(err.contains("npx tsc --noEmit"));
    }

    #[test]
    fn count_keyword_respects_word_boundary() {
        // `mytest(` should NOT count; `test(` should. `testing(` should not
        // count even though it contains `test`.
        assert_eq!(count_test_keyword_occurrences("test(x"), 1);
        assert_eq!(count_test_keyword_occurrences("mytest(x"), 0);
        assert_eq!(count_test_keyword_occurrences("testing(x"), 0);
        assert_eq!(count_test_keyword_occurrences("it('x', () => {})"), 1);
        assert_eq!(count_test_keyword_occurrences("unit('x', () => {})"), 0);
    }

    #[test]
    fn block_signature_ignores_whitespace_but_respects_body() {
        let a = TestBlock {
            name: "x".to_string(),
            body: "  realFn();  expect(1).toBe(1);  ".to_string(),
        };
        let b = TestBlock {
            name: "x".to_string(),
            body: "realFn();expect(1).toBe(1);".to_string(),
        };
        let c = TestBlock {
            name: "x".to_string(),
            body: "realFn();expect(2).toBe(2);".to_string(),
        };
        assert_eq!(block_signature(&a), block_signature(&b));
        assert_ne!(block_signature(&a), block_signature(&c));
    }
}
