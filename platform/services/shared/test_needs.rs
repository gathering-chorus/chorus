// @domain: tests
// #4539 — the `@needs:` header: where a test can prove anything. Shared by the
// tagger (chorus-crawl copies it to the Test row's testNeeds) and werk-test (a
// card run reads the werk's own file first, so a card that adds or removes the
// header is judged by what it ships). std only; include!()d by both crates.

/// #4539 — where a test can prove anything, from its own header:
/// `# @needs: machine` (it reads this box) or `# @needs: nightly` (it reads the
/// clock), in `//`, `#`, `*` or `/*` comment form, in the first 40 lines. The
/// first such line decides. A card run does not select such a test; the
/// nightly does. `Some(Err(word))` is a header with a word we don't know: the
/// crawl names it out loud (Silas 04:00 — a typo must not silently put a box
/// test back in card runs).
pub fn needs_header(content: &str) -> Option<Result<&'static str, String>> {
    for line in content.lines().take(40) {
        let t = line.trim_start();
        let Some(t) = t
            .strip_prefix("/*")
            .or_else(|| t.strip_prefix("//"))
            .or_else(|| t.strip_prefix('#'))
            .or_else(|| t.strip_prefix('*'))
        else {
            continue;
        };
        let Some(rest) = t.trim_start().strip_prefix("@needs:") else { continue };
        let word: String = rest.trim_start().chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
        return Some(match word.to_ascii_lowercase().as_str() {
            "machine" => Ok("machine"),
            "nightly" => Ok("nightly"),
            _ => Err(word),
        });
    }
    None
}

/// The `@needs:` value when the header names a known one.
pub fn declared_needs(content: &str) -> Option<&'static str> {
    needs_header(content).and_then(|r| r.ok())
}
