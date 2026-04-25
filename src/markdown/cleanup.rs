use once_cell::sync::Lazy;
use regex::Regex;

static LINK_COPIED_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\s*Link copied!").unwrap());

/// Truncates the input so it begins at the first Markdown header line.
///
/// If a line exists whose leading-trimmed text starts with `#`, the returned
/// string contains that line and all following lines joined with `\n`. If no
/// such header exists or the first header is already the first line, the
/// original `text` is returned unchanged. If the original `text` ends with a
/// trailing newline, the returned string preserves that trailing newline.
///
/// # Examples
///
/// ```
/// let s = "Intro\n\n  # Heading\ncontent\n";
/// let out = strip_preamble(s);
/// assert_eq!(out, "# Heading\ncontent\n");
/// ```
pub(super) fn strip_preamble(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut first_header_idx = None;

    for (idx, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with('#') {
            first_header_idx = Some(idx);
            break;
        }
    }

    match first_header_idx {
        None | Some(0) => text.to_string(),
        Some(idx) => {
            let mut remaining = lines[idx..].join("\n");
            if text.ends_with('\n') {
                remaining.push('\n');
            }
            remaining
        }
    }
}

/// Removes occurrences of the "Link copied!" substring (optionally preceded by whitespace) from the input text.
///
/// # Examples
///
/// ```
/// let s = "Title\n\n  Link copied!\nContent\n";
/// let cleaned = remove_link_copied(s);
/// assert!(!cleaned.contains("Link copied!"));
/// assert!(cleaned.contains("Title"));
/// assert!(cleaned.contains("Content"));
/// ```
pub(super) fn remove_link_copied(text: &str) -> String {
    LINK_COPIED_RE.replace_all(text, "").to_string()
}

/// Removes lines that start with "Ask Devin about" (after trimming leading whitespace).
///
/// This function splits the input text into lines, filters out any line whose
/// trimmed form begins with `Ask Devin about`, and rejoins the remaining lines
/// with `\n`. If the original input ends with a trailing newline, the result
/// preserves that trailing newline.
///
/// # Examples
///
/// ```
/// let src = "Intro\n  Ask Devin about reminders\nBody\n";
/// let cleaned = remove_ask_devin_lines(src);
/// assert_eq!(cleaned, "Intro\nBody\n");
/// ```
pub(super) fn remove_ask_devin_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let filtered: Vec<&str> = lines
        .into_iter()
        .filter(|line| !line.trim().starts_with("Ask Devin about"))
        .collect();
    let mut result = filtered.join("\n");
    if text.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Converts literal `\n` sequences into actual newline characters.
///
/// Replaces every occurrence of the two-character sequence `\n` (a backslash followed by `n`) with a real newline.
///
/// # Examples
///
/// ```
/// let out = fix_literal_backslash_n("a\\nb");
/// assert_eq!(out, "a\nb");
/// ```
pub(super) fn fix_literal_backslash_n(text: &str) -> String {
    // Replace literal \n (backslash followed by n) with actual newlines
    // This fixes corrupted content from DeepWiki exports
    text.replace("\\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_preamble_before_first_heading_and_preserves_trailing_newline() {
        assert_eq!(strip_preamble("junk\n# Title\nBody\n"), "# Title\nBody\n");
    }

    #[test]
    fn removes_link_copied_and_ask_devin_artifacts() {
        let text = remove_link_copied("# Title Link copied!\nAsk Devin about this\nBody\n");
        assert_eq!(remove_ask_devin_lines(&text), "# Title\nBody\n");
    }

    #[test]
    fn converts_literal_backslash_n_to_newline() {
        assert_eq!(fix_literal_backslash_n("a\\nb"), "a\nb");
    }

    // --- Additional edge-case tests ---

    #[test]
    fn strip_preamble_returns_unchanged_when_no_header_exists() {
        let text = "no header here\njust plain text\n";
        assert_eq!(strip_preamble(text), text);
    }

    #[test]
    fn strip_preamble_returns_unchanged_when_header_is_first_line() {
        let text = "# Already a header\ncontent\n";
        assert_eq!(strip_preamble(text), text);
    }

    #[test]
    fn strip_preamble_preserves_absence_of_trailing_newline_after_stripping() {
        let input = "intro\n# Header\nbody";
        let result = strip_preamble(input);
        assert_eq!(result, "# Header\nbody");
        assert!(!result.ends_with('\n'));
    }

    #[test]
    fn strip_preamble_on_empty_string_returns_empty() {
        assert_eq!(strip_preamble(""), "");
    }

    #[test]
    fn strip_preamble_strips_multiple_preamble_lines() {
        let input = "line 1\nline 2\nline 3\n# Heading\nbody\n";
        assert_eq!(strip_preamble(input), "# Heading\nbody\n");
    }

    #[test]
    fn remove_link_copied_removes_multiple_occurrences_in_same_doc() {
        let input = "Title Link copied!\nContent Link copied!\n";
        let result = remove_link_copied(input);
        assert!(!result.contains("Link copied!"));
        assert!(result.contains("Title"));
        assert!(result.contains("Content"));
    }

    #[test]
    fn remove_link_copied_leaves_text_without_marker_unchanged() {
        let input = "Normal text\nNo artifact here\n";
        assert_eq!(remove_link_copied(input), input);
    }

    #[test]
    fn remove_link_copied_removes_marker_preceded_by_spaces() {
        // The regex includes optional leading whitespace before "Link copied!"
        let input = "A   Link copied!B";
        let result = remove_link_copied(input);
        assert!(!result.contains("Link copied!"));
        assert!(result.contains("AB"));
    }

    #[test]
    fn remove_ask_devin_lines_handles_leading_whitespace_variants() {
        let input = "Before\n   Ask Devin about something\nAfter\n";
        let result = remove_ask_devin_lines(input);
        assert_eq!(result, "Before\nAfter\n");
    }

    #[test]
    fn remove_ask_devin_lines_removes_multiple_consecutive_devin_lines() {
        let input = "# Header\nAsk Devin about A\nAsk Devin about B\nBody\n";
        let result = remove_ask_devin_lines(input);
        assert_eq!(result, "# Header\nBody\n");
    }

    #[test]
    fn remove_ask_devin_lines_preserves_trailing_newline() {
        let input = "# Header\nAsk Devin about foo\n";
        let result = remove_ask_devin_lines(input);
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn remove_ask_devin_lines_no_match_returns_original() {
        let input = "Line one\nLine two\n";
        assert_eq!(remove_ask_devin_lines(input), input);
    }

    #[test]
    fn fix_literal_backslash_n_replaces_multiple_sequences() {
        let input = "a\\nb\\nc";
        assert_eq!(fix_literal_backslash_n(input), "a\nb\nc");
    }

    #[test]
    fn fix_literal_backslash_n_no_sequences_returns_original() {
        let input = "plain text with\nreal newlines\n";
        assert_eq!(fix_literal_backslash_n(input), input);
    }

    #[test]
    fn fix_literal_backslash_n_empty_string() {
        assert_eq!(fix_literal_backslash_n(""), "");
    }

    #[test]
    fn fix_literal_backslash_n_does_not_double_convert_real_newlines() {
        // Already-real newlines should not be disturbed
        let input = "line1\nline2\\nline3\n";
        let result = fix_literal_backslash_n(input);
        // The literal \n becomes a real newline; the existing newlines stay
        assert_eq!(result, "line1\nline2\nline3\n");
    }
}
