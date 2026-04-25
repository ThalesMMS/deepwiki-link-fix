mod cleanup;
mod links;
mod mermaid;
mod tables;
mod wrapping;

pub(crate) use mermaid::sanitize_mermaid_block;

/// Process markdown-like text through a fixed pipeline of cleanup, link fixes, mermaid sanitization, table normalization, and line wrapping.
///
/// Returns the transformed `String`.
///
/// # Examples
///
/// ```
/// let input = "# Title\nLink copied!\nAsk Devin about this\nLiteral\\nBreak\n[repo](/owner/repo/path)\n";
/// let out = process_text(input);
/// assert!(out.starts_with("# Title\n"));
/// assert!(out.contains("https://github.com/owner/repo/path"));
/// assert!(out.contains("Literal\nBreak"));
/// assert!(!out.contains("Link copied!"));
/// assert!(!out.contains("Ask Devin about"));
/// ```
pub(crate) fn process_text(text: &str) -> String {
    let text = cleanup::strip_preamble(text);
    let text = cleanup::remove_link_copied(&text);
    let text = cleanup::remove_ask_devin_lines(&text);
    let text = cleanup::fix_literal_backslash_n(&text);
    let text = links::fix_internal_links(&text);
    let text = links::fix_section_links(&text);
    let text = links::strip_github_blob_sha(&text);
    let text = mermaid::sanitize_mermaid(&text);
    let text = tables::fix_table_content(&text);
    let text = wrapping::fix_long_lines(&text);
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_text_applies_cleanup_links_mermaid_tables_and_wrapping() {
        let input = "intro\n# Title Link copied!\nAsk Devin about this\nLiteral\\nBreak\nSee [repo](/owner/repo/path).\n```mermaid\nflowchart TD\nA[\"- item\"]\n```\n| col |\n| a very long column value that wraps cleanly |\n";

        let output = process_text(input);

        assert!(output.starts_with("# Title\n"));
        assert!(output.contains("[repo](https://github.com/owner/repo/path)"));
        assert!(output.contains("A[\"Unsupported markdown: list\"]"));
        assert!(output.contains("a very long column value that<br>wraps cleanly"));
        assert!(output.contains("Literal\nBreak"));
        assert!(!output.contains("Ask Devin about"));
        assert!(!output.contains("Link copied!"));
    }

    // --- Additional edge-case tests ---

    #[test]
    fn process_text_returns_empty_for_empty_input() {
        assert_eq!(process_text(""), "");
    }

    #[test]
    fn process_text_unchanged_content_returns_same_string() {
        // Content with no preamble, no artifacts, no internal links, no mermaid, short lines
        let input = "# Title\n\nSimple content.\n";
        let output = process_text(input);
        assert_eq!(output, input);
    }

    #[test]
    fn process_text_strips_preamble_before_header() {
        let input = "some preamble\n\n# Header\nBody\n";
        let output = process_text(input);
        assert!(output.starts_with("# Header\n"));
        assert!(!output.contains("some preamble"));
    }

    #[test]
    fn process_text_strips_github_blob_sha_from_links() {
        let input = "# Title\nSee https://github.com/owner/repo/blob/abc1234/file.md\n";
        let output = process_text(input);
        assert!(output.contains("https://github.com/owner/repo/file.md"));
        assert!(!output.contains("/blob/abc1234/"));
    }

    #[test]
    fn process_text_removes_ask_devin_lines() {
        let input = "# Header\nAsk Devin about this\nKeep this line\n";
        let output = process_text(input);
        assert!(!output.contains("Ask Devin"));
        assert!(output.contains("Keep this line"));
    }

    #[test]
    fn process_text_fixes_literal_backslash_n() {
        let input = "# Title\nLine one\\nLine two\n";
        let output = process_text(input);
        // The literal \n should be converted to a real newline
        assert!(output.contains("Line one\nLine two"));
    }

    #[test]
    fn process_text_sanitizes_mermaid_sequence_diagrams() {
        let input = "# Diagram\n```mermaid\nsequenceDiagram\nparticipant User Browser\nUser Browser->>Server: req\n```\n";
        let output = process_text(input);
        assert!(output.contains("participant User_Browser as User Browser"));
        assert!(output.contains("User_Browser->>Server"));
    }

    #[test]
    fn process_text_wraps_long_lines() {
        let long_line = "word ".repeat(25);
        let input = format!("# Title\n{}\n", long_line.trim_end());
        let output = process_text(&input);
        let body_lines: Vec<&str> = output.lines().filter(|l| !l.starts_with('#')).collect();
        assert!(body_lines.iter().all(|l| l.len() <= 80));
    }
}
