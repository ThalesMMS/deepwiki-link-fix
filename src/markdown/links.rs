use std::collections::HashMap;

use once_cell::sync::Lazy;
use regex::Regex;

static INTERNAL_LINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\]\((/[^)/\s]+/[^)/\s]+(?:/[^\s)]*)?)\)").unwrap());
static REF_STYLE_LINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)(^\s*\[[^\]]+\]:\s*)(/[^)/\s]+/[^)/\s]+(?:/[^\s)]*)?)").unwrap());
static BLOB_SHA_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"https://github\.com/([^/]+)/([^/]+)/blob/([0-9a-f]{7,40})/").unwrap()
});
static SECTION_LINK_MAP: Lazy<HashMap<&'static str, Regex>> = Lazy::new(|| {
    SECTION_ANCHORS
        .iter()
        .map(|(section, _)| {
            let pattern = format!(
                r"https://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/blob/(?P<sha>[0-9a-f]{{7,40}})/{}",
                regex::escape(section)
            );
            (*section, Regex::new(&pattern).unwrap())
        })
        .collect()
});

static SECTION_ANCHORS: &[(&str, &str)] = &[
    ("Networking Section", "networking-configuration"),
    ("Virtual Environment Section", "virtual-environment-setup"),
    ("Module Import Section", "module-import-issues"),
    ("WSL.exe Section", "wslexe-issues"),
    ("Path Translation Section", "path-translation-issues"),
    ("Performance Section", "performance-optimization"),
    ("Line Ending Section", "line-ending-issues"),
    ("Distribution Section", "distribution-selection"),
];
static SECTION_ANCHOR_MAP: Lazy<HashMap<&'static str, &'static str>> =
    Lazy::new(|| SECTION_ANCHORS.iter().copied().collect());

/// Rewrite Markdown internal GitHub paths into absolute `https://github.com` URLs.
///
/// This function converts Markdown inline links whose targets start with a leading `/`
/// and contain at least two slash-separated segments, and reference-style link
/// definitions with the same kind of target, by prefixing those targets with
/// `https://github.com`.
///
/// # Examples
///
/// ```
/// let src = "See this repo: ](/owner/repo/path/to/file)\n[ref]: /owner/repo/README.md";
/// let out = fix_internal_links(src);
/// assert!(out.contains("](https://github.com/owner/repo/path/to/file)"));
/// assert!(out.contains("[ref]: https://github.com/owner/repo/README.md"));
/// ```
///
/// # Returns
///
/// A `String` containing the input Markdown with matched internal link targets
/// rewritten to absolute `https://github.com` URLs.
pub(super) fn fix_internal_links(text: &str) -> String {
    let text = INTERNAL_LINK_RE.replace_all(text, |caps: &regex::Captures| {
        format!("](https://github.com{})", &caps[1])
    });

    let text = REF_STYLE_LINK_RE.replace_all(&text, |caps: &regex::Captures| {
        format!("{}https://github.com{}", &caps[1], &caps[2])
    });

    text.to_string()
}

/// Rewrites GitHub blob URLs that point to known README sections into README anchor links.
///
/// For each configured section name this replaces occurrences of
/// `https://github.com/{owner}/{repo}/blob/{sha}/...{Section Name}...` with
/// `https://github.com/{owner}/{repo}/blob/{sha}/README.md#{anchor}` where `anchor`
/// is the mapped README anchor for that section.
///
/// # Examples
///
/// ```
/// let input = "See https://github.com/owner/repo/blob/abcdef1/Networking Section for details.";
/// let out = crate::markdown::links::fix_section_links(input);
/// assert!(out.contains("https://github.com/owner/repo/blob/abcdef1/README.md#networking-configuration"));
/// ```
pub(super) fn fix_section_links(text: &str) -> String {
    let mut result = text.to_string();

    for (section, re) in SECTION_LINK_MAP.iter() {
        let anchor = SECTION_ANCHOR_MAP.get(section).unwrap();
        result = re
            .replace_all(&result, |caps: &regex::Captures| {
                format!(
                    "https://github.com/{}/{}/blob/{}/README.md#{}",
                    &caps["owner"], &caps["repo"], &caps["sha"], anchor
                )
            })
            .to_string();
    }

    result
}

/// Removes GitHub "blob" SHA segments from any matching blob URLs in the input text.
///
/// This rewrites occurrences of `https://github.com/<owner>/<repo>/blob/<sha>/...` to
/// `https://github.com/<owner>/<repo>/...`, where `<sha>` is a 7–40 character hexadecimal commit/sha.
/// Other parts of the URL and unrelated text are left unchanged.
///
/// # Examples
///
/// ```
/// let src = "See https://github.com/owner/repo/blob/abc1234/path/to/file.md for details.";
/// let out = strip_github_blob_sha(src);
/// assert_eq!(out, "See https://github.com/owner/repo/path/to/file.md for details.");
/// ```
pub(super) fn strip_github_blob_sha(text: &str) -> String {
    BLOB_SHA_RE
        .replace_all(text, "https://github.com/$1/$2/")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixes_inline_and_reference_style_internal_links() {
        let input = "[Docs](/owner/repo/path)\n[ref]: /owner/repo/other\n";

        assert_eq!(
            fix_internal_links(input),
            "[Docs](https://github.com/owner/repo/path)\n[ref]: https://github.com/owner/repo/other\n"
        );
    }

    #[test]
    fn maps_known_section_links_to_readme_anchors() {
        let input = "https://github.com/owner/repo/blob/abcdef1/Networking Section";

        assert_eq!(
            fix_section_links(input),
            "https://github.com/owner/repo/blob/abcdef1/README.md#networking-configuration"
        );
    }

    #[test]
    fn strips_github_blob_sha_segments() {
        let input = "https://github.com/owner/repo/blob/abcdef1234567890/path/file.rs";

        assert_eq!(
            strip_github_blob_sha(input),
            "https://github.com/owner/repo/path/file.rs"
        );
    }

    // --- Additional edge-case tests ---

    #[test]
    fn fix_internal_links_ignores_external_links() {
        // External links (https://) should not be double-prefixed
        let input = "[Docs](https://github.com/owner/repo/path)\n";
        // fix_internal_links only rewrites paths starting with '/' — this is already absolute
        assert_eq!(fix_internal_links(input), input);
    }

    #[test]
    fn fix_internal_links_does_not_modify_relative_links() {
        // Relative paths without a leading '/' should not be modified
        let input = "[Page](relative/path.md)\n";
        assert_eq!(fix_internal_links(input), input);
    }

    #[test]
    fn fix_internal_links_handles_path_with_fragment() {
        let input = "[Docs](/owner/repo/path#section)\n";
        // Fragment is within the parentheses but the regex stops at ')'; '#' is fine since it's part of the path
        let output = fix_internal_links(input);
        // The regex matches paths starting with '/' — fragment may or may not be included
        // Verify the link at least starts with the expected base
        assert!(output.contains("https://github.com/owner/repo/"));
    }

    #[test]
    fn fix_internal_links_text_without_any_links_is_unchanged() {
        let input = "No links here at all.\n";
        assert_eq!(fix_internal_links(input), input);
    }

    #[test]
    fn fix_section_links_all_known_sections_are_mapped() {
        // Verify every section in SECTION_ANCHORS maps correctly
        let section_pairs = [
            ("Virtual Environment Section", "virtual-environment-setup"),
            ("Module Import Section", "module-import-issues"),
            ("WSL.exe Section", "wslexe-issues"),
            ("Path Translation Section", "path-translation-issues"),
            ("Performance Section", "performance-optimization"),
            ("Line Ending Section", "line-ending-issues"),
            ("Distribution Section", "distribution-selection"),
        ];
        for (section, expected_anchor) in section_pairs {
            let input = format!("https://github.com/owner/repo/blob/abcdef1/{}", section);
            let output = fix_section_links(&input);
            assert!(
                output.contains(&format!("README.md#{}", expected_anchor)),
                "section '{}' did not map to anchor '{}'; got: {}",
                section,
                expected_anchor,
                output
            );
        }
    }

    #[test]
    fn fix_section_links_leaves_unknown_urls_unchanged() {
        let input = "https://github.com/owner/repo/blob/abcdef1/Unknown Section";
        // Should not match any known section → returned unchanged
        assert_eq!(fix_section_links(input), input);
    }

    #[test]
    fn strip_github_blob_sha_removes_seven_char_sha() {
        let input = "https://github.com/owner/repo/blob/abc1234/file.md";
        assert_eq!(
            strip_github_blob_sha(input),
            "https://github.com/owner/repo/file.md"
        );
    }

    #[test]
    fn strip_github_blob_sha_leaves_non_blob_urls_unchanged() {
        let input = "https://github.com/owner/repo/tree/main/folder/";
        assert_eq!(strip_github_blob_sha(input), input);
    }

    #[test]
    fn strip_github_blob_sha_handles_multiple_urls_in_one_string() {
        let input = "See https://github.com/a/b/blob/abc1234/x.md and https://github.com/c/d/blob/def5678/y.md";
        let output = strip_github_blob_sha(input);
        assert!(output.contains("https://github.com/a/b/x.md"));
        assert!(output.contains("https://github.com/c/d/y.md"));
        assert!(!output.contains("/blob/"));
    }
}
