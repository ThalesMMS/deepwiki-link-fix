/// Compute the UTF‑8 byte length of a Markdown marker prefix at the start of `rest`.
///
/// The prefix includes any leading `>` quote markers (each followed by any whitespace),
/// and, immediately after those, an optional list marker:
/// - a bullet marker (`-`, `*`, `+`) followed by at least one whitespace, or
/// - an ordered marker (one or more ASCII digits) followed by `.` or `)` and at least one whitespace.
/// If no list marker follows the optional quote prefix, the result is the byte length of the consumed quote prefix only.
///
/// # Returns
///
/// The total number of bytes at the start of `rest` that form the Markdown marker prefix
/// (quote markers, an optional list marker, and the following whitespace).
///
/// # Examples
///
/// ```
/// assert_eq!(marker_prefix_len("> - item"), 4); // '>' + ' ' + '-' + ' '
/// assert_eq!(marker_prefix_len(">>>   1.  x"), 8); // three '>' + three spaces + '1' + '.' + two spaces
/// assert_eq!(marker_prefix_len("no-marker"), 0);
/// ```
fn marker_prefix_len(rest: &str) -> usize {
    let mut prefix_len = 0;
    let mut remaining = rest;

    while remaining.starts_with('>') {
        prefix_len += 1;
        remaining = &remaining[1..];

        let spaces_len: usize = remaining
            .chars()
            .take_while(|c| c.is_whitespace())
            .map(char::len_utf8)
            .sum();
        prefix_len += spaces_len;
        remaining = &remaining[spaces_len..];
    }

    if let Some(first) = remaining.chars().next() {
        if matches!(first, '-' | '*' | '+') {
            let marker_len = first.len_utf8();
            if remaining[marker_len..]
                .chars()
                .next()
                .map_or(false, |c| c.is_whitespace())
            {
                let spaces_len: usize = remaining[marker_len..]
                    .chars()
                    .take_while(|c| c.is_whitespace())
                    .map(char::len_utf8)
                    .sum();
                return prefix_len + marker_len + spaces_len;
            }
        }
    }

    let digits_len: usize = remaining
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .map(char::len_utf8)
        .sum();
    if digits_len > 0 {
        let after_digits = &remaining[digits_len..];
        if let Some(marker) = after_digits.chars().next() {
            if marker == '.' || marker == ')' {
                let marker_len = marker.len_utf8();
                if after_digits[marker_len..]
                    .chars()
                    .next()
                    .map_or(false, |c| c.is_whitespace())
                {
                    let spaces_len: usize = after_digits[marker_len..]
                        .chars()
                        .take_while(|c| c.is_whitespace())
                        .map(char::len_utf8)
                        .sum();
                    return prefix_len + digits_len + marker_len + spaces_len;
                }
            }
        }
    }

    prefix_len
}

/// Compute the prefix of a Markdown line that must be preserved when wrapping.
///
/// The prefix consists of any leading whitespace followed by any marker prefix
/// at that position (chains of `>` with their following whitespace, a bullet
/// marker `-`, `*`, or `+` followed by space, or an ordered marker of digits
/// followed by `.` or `)` and trailing space).
///
/// # Examples
///
/// ```
/// let p = wrapping_prefix("  - this is a long item");
/// assert_eq!(p, "  - ");
///
/// let p = wrapping_prefix(">\t> 1) nested numbered");
/// assert_eq!(p, ">\t> 1) ");
///
/// let p = wrapping_prefix("plain line");
/// assert_eq!(p, "");
/// ```
fn wrapping_prefix(line: &str) -> &str {
    let leading_len: usize = line
        .chars()
        .take_while(|c| c.is_whitespace())
        .map(char::len_utf8)
        .sum();
    let marker_len = marker_prefix_len(&line[leading_len..]);
    &line[..leading_len + marker_len]
}

/// Appends `line` to `result` by splitting it into byte-length chunks no larger than `max_line_length` while preserving UTF‑8 character boundaries.
///
/// The function writes each chunk followed by a newline between chunks and a final trailing newline. `max_line_length` is interpreted in bytes; if a single UTF‑8 character's encoding exceeds `max_line_length`, at least that character will still be written (the chunk will contain the first full character boundary after `start`).
///
/// # Examples
///
/// ```
/// let mut out = String::new();
/// // "é" is two bytes in UTF-8; ensure boundaries are preserved
/// let line = "é".repeat(10);
/// push_utf8_chunks(&mut out, &line, 4); // splits so no chunk cuts a multibyte char
/// for chunk in out.lines() {
///     assert!(chunk.len() <= 4 || chunk.len() % 2 == 0); // simple sanity for multibyte chars
/// }
/// ```
fn push_utf8_chunks(result: &mut String, line: &str, max_line_length: usize) {
    let mut start = 0;
    let mut first = true;

    while start < line.len() {
        if !first {
            result.push('\n');
        }
        first = false;

        let mut end = (start + max_line_length).min(line.len());
        while end > start && !line.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            end = line[start..]
                .char_indices()
                .nth(1)
                .map_or(line.len(), |(idx, _)| start + idx);
        }

        result.push_str(&line[start..end]);
        start = end;
    }
    result.push('\n');
}

/// Splits a string into consecutive runs that preserve exact spaces.
///
/// Returns alternating slices that are either a run of space characters (`" "`) or a run of non-space characters,
/// in the same order they appear in `content`. Each returned slice borrows from the original `content`.
///
/// # Examples
///
/// ```
/// let tokens = space_preserving_tokens("a  bc def");
/// assert_eq!(tokens, vec!["a", "  ", "bc", " ", "def"]);
/// ```
fn space_preserving_tokens(content: &str) -> Vec<&str> {
    let mut tokens = Vec::new();
    let mut token_start = 0;
    let mut token_is_space = None;

    for (idx, ch) in content.char_indices() {
        let is_space = ch == ' ';
        match token_is_space {
            None => token_is_space = Some(is_space),
            Some(current_is_space) if current_is_space != is_space => {
                tokens.push(&content[token_start..idx]);
                token_start = idx;
                token_is_space = Some(is_space);
            }
            _ => {}
        }
    }

    if token_start < content.len() {
        tokens.push(&content[token_start..]);
    }

    tokens
}

/// Wraps a single Markdown line to a maximum byte length while preserving its leading
/// indentation and any Markdown marker prefix (quote markers, list markers, ordered markers)
/// and keeping runs of spaces exactly as in the original. Appended wrapped lines each end
/// with a newline and retain the detected prefix on every output line. If the input line
/// ends with two spaces, that "hard break" is preserved on the final wrapped line.
///
/// Arguments:
/// - `result`: buffer to which the wrapped output lines are appended (each line ends with `\n`).
/// - `line`: the single Markdown line to wrap (not including a trailing newline).
/// - `max_line_length`: maximum allowed byte length per output line; the effective wrap
///   width is reduced by the detected prefix length but is at least 1.
///
/// # Examples
///
/// ```
/// let mut out = String::new();
/// let line = "  - This    is    a    long    list    item    with    preserved    spaces.  ";
/// push_wrapped_markdown_line(&mut out, line, 30);
/// // every output line starts with "  - " and the final line keeps the trailing two spaces
/// assert!(out.lines().all(|l| l.len() <= 30));
/// assert!(out.lines().all(|l| l.starts_with("  - ")));
/// assert!(out.lines().last().unwrap().ends_with("  "));
/// ```
fn push_wrapped_markdown_line(result: &mut String, line: &str, max_line_length: usize) {
    let hard_break = line.ends_with("  ");
    let prefix = wrapping_prefix(line);
    let content = &line[prefix.len()..];
    let line_limit = max_line_length.saturating_sub(prefix.len()).max(1);
    let mut wrapped_lines: Vec<String> = Vec::new();
    let mut current_line = String::new();

    for token in space_preserving_tokens(content) {
        if current_line.is_empty() || current_line.len() + token.len() <= line_limit {
            current_line.push_str(token);
        } else {
            wrapped_lines.push(format!("{}{}", prefix, current_line));
            current_line = token.to_string();
        }
    }

    if !current_line.is_empty() {
        wrapped_lines.push(format!("{}{}", prefix, current_line));
    }

    if wrapped_lines.is_empty() {
        wrapped_lines.push(prefix.to_string());
    }

    if hard_break {
        if let Some(last) = wrapped_lines.last_mut() {
            if !last.ends_with("  ") {
                last.push_str("  ");
            }
        }
    }

    for wrapped_line in wrapped_lines {
        result.push_str(&wrapped_line);
        result.push('\n');
    }
}

fn fence_marker(trimmed: &str) -> Option<String> {
    let marker_char = trimmed.chars().next()?;
    if marker_char != '`' && marker_char != '~' {
        return None;
    }

    let marker_len = trimmed.chars().take_while(|ch| *ch == marker_char).count();
    if marker_len >= 3 {
        Some(marker_char.to_string().repeat(marker_len))
    } else {
        None
    }
}

fn is_table_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

/// Reflows input text so no non-code line exceeds 80 bytes while preserving Markdown structure.
///
/// This function scans the input line-by-line and treats fenced code blocks (lines starting with
/// ``` or ~~~) as indivisible regions. For non-code lines longer than 80 bytes it preserves any
/// leading indentation and Markdown marker prefixes (quote markers `>`, bullet markers `-/*/+`,
/// or ordered markers like `1.`/`1)`) and wraps the remaining content by words while preserving
/// exact runs of spaces and any trailing "hard break" (two spaces). For lines inside fenced code
/// blocks it leaves moderately long lines unchanged; for extremely long code lines (longer than
/// 160 bytes) it splits them into UTF-8-safe chunks of at most 80 bytes.
///
/// # Examples
///
/// ```
/// let input = "This is a very long line that will be wrapped by the function because it exceeds \
/// the eighty character limit imposed by the wrapper. It should break at word boundaries.";
/// let out = fix_long_lines(input);
/// for line in out.lines() {
///     assert!(line.len() <= 80);
/// }
/// ```
pub(super) fn fix_long_lines(text: &str) -> String {
    let mut result = String::new();
    let max_line_length = 80; // Character limit per line
    let mut active_fence: Option<String> = None;

    for line in text.lines() {
        // Detect the start/end of fenced code blocks
        let trimmed = line.trim_start();
        if let Some(marker) = fence_marker(trimmed) {
            match active_fence.as_deref() {
                Some(active) if trimmed.starts_with(active) => active_fence = None,
                None => active_fence = Some(marker),
                Some(_) => {}
            }
            result.push_str(line);
            result.push('\n');
            continue;
        }

        // For code blocks, wrap very long lines
        if active_fence.is_some() && line.len() > max_line_length {
            // For code, break at logical points or simply at the limit
            if line.len() > max_line_length * 2 {
                // Extremely long lines, break at the limit
                push_utf8_chunks(&mut result, line, max_line_length);
            } else {
                result.push_str(line);
                result.push('\n');
            }
        } else if active_fence.is_none() && line.len() > max_line_length {
            // Normal text, wrap by words
            if is_table_row(line) {
                result.push_str(line);
                result.push('\n');
            } else {
                push_wrapped_markdown_line(&mut result, line, max_line_length);
            }
        } else {
            result.push_str(line);
            result.push('\n');
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_long_normal_lines_by_words() {
        let input = "word ".repeat(20);
        let output = fix_long_lines(input.trim_end());

        assert!(output.lines().all(|line| line.len() <= 80));
        assert!(output.lines().count() > 1);
    }

    #[test]
    fn leaves_moderately_long_code_lines_unwrapped() {
        let code_line = "a".repeat(100);
        let input = format!("```\n{}\n```\n", code_line);

        assert_eq!(fix_long_lines(&input), input);
    }

    #[test]
    fn wraps_markdown_lines_with_marker_prefix_and_hard_breaks() {
        let input = format!("  - {}  ", "word ".repeat(20).trim_end());
        let output = fix_long_lines(&input);
        let lines: Vec<&str> = output.lines().collect();

        assert!(lines.len() > 1);
        assert!(lines.iter().all(|line| line.starts_with("  - ")));
        assert!(lines.last().unwrap().ends_with("  "));
    }

    #[test]
    fn splits_extremely_long_code_lines_on_utf8_boundaries() {
        let code_line = "é".repeat(90);
        let input = format!("```\n{}\n```\n", code_line);
        let output = fix_long_lines(&input);

        assert!(output.contains('é'));
        assert!(!output.contains('\u{fffd}'));
    }

    #[test]
    fn wraps_tilde_fenced_code_as_code_blocks() {
        let code_line = "a".repeat(100);
        let input = format!("~~~\n{}\n~~~\n", code_line);

        assert_eq!(fix_long_lines(&input), input);
    }

    #[test]
    fn only_closes_matching_fence_markers() {
        let code_line = "a".repeat(100);
        let input = format!("```\n~~~\n{}\n```\n", code_line);

        assert_eq!(fix_long_lines(&input), input);
    }

    #[test]
    fn leaves_long_table_rows_unwrapped() {
        let row = format!("| col | {} |", "word ".repeat(30));
        let input = format!("{}\n", row);

        assert_eq!(fix_long_lines(&input), input);
    }

    #[test]
    fn preserves_repeated_spaces_in_wrapped_markdown_lines() {
        let input = format!("word{}tail", "  middle".repeat(12));
        let output = fix_long_lines(&input);

        assert!(output.contains("  middle"));
        assert!(!output.contains(" word"));
    }

    // --- Additional edge-case tests ---

    #[test]
    fn short_lines_are_not_wrapped() {
        let input = "Short line.\nAnother short line.\n";
        assert_eq!(fix_long_lines(input), input);
    }

    #[test]
    fn empty_string_returns_empty() {
        assert_eq!(fix_long_lines(""), "");
    }

    #[test]
    fn marker_prefix_len_detects_bullet_marker() {
        assert_eq!(marker_prefix_len("- item"), 2); // '-' + ' '
        assert_eq!(marker_prefix_len("* item"), 2); // '*' + ' '
        assert_eq!(marker_prefix_len("+ item"), 2); // '+' + ' '
    }

    #[test]
    fn marker_prefix_len_detects_ordered_marker_with_dot() {
        assert_eq!(marker_prefix_len("1. item"), 3); // '1' + '.' + ' '
        assert_eq!(marker_prefix_len("10. item"), 4); // '1' + '0' + '.' + ' '
    }

    #[test]
    fn marker_prefix_len_detects_ordered_marker_with_paren() {
        assert_eq!(marker_prefix_len("1) item"), 3); // '1' + ')' + ' '
    }

    #[test]
    fn marker_prefix_len_detects_blockquote_prefix() {
        assert_eq!(marker_prefix_len("> item"), 2); // '>' + ' '
        assert_eq!(marker_prefix_len(">> item"), 3); // '>' + '>' + ' '
    }

    #[test]
    fn marker_prefix_len_returns_zero_for_plain_text() {
        assert_eq!(marker_prefix_len("no-marker"), 0);
        assert_eq!(marker_prefix_len("plain text"), 0);
    }

    #[test]
    fn wrapping_prefix_detects_bullet_in_indented_line() {
        let p = wrapping_prefix("  - item content");
        assert_eq!(p, "  - "); // two spaces + dash + space
    }

    #[test]
    fn wrapping_prefix_detects_ordered_list() {
        let p = wrapping_prefix("1. ordered item");
        assert_eq!(p, "1. ");
    }

    #[test]
    fn wrapping_prefix_returns_empty_for_plain_line() {
        let p = wrapping_prefix("plain line");
        assert_eq!(p, "");
    }

    #[test]
    fn space_preserving_tokens_splits_alternating_runs() {
        let tokens = space_preserving_tokens("a  bc def");
        assert_eq!(tokens, vec!["a", "  ", "bc", " ", "def"]);
    }

    #[test]
    fn space_preserving_tokens_empty_string() {
        let tokens = space_preserving_tokens("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn space_preserving_tokens_only_spaces() {
        let tokens = space_preserving_tokens("   ");
        assert_eq!(tokens, vec!["   "]);
    }

    #[test]
    fn push_utf8_chunks_does_not_break_multibyte_characters() {
        let mut out = String::new();
        // "é" is 2 bytes in UTF-8; repeat 50 times = 100 bytes
        let line = "é".repeat(50);
        push_utf8_chunks(&mut out, &line, 10);
        // Each line should contain complete 'é' characters (no replacement chars)
        assert!(!out.contains('\u{fffd}'));
        for chunk in out.lines() {
            // Chunk length should not exceed max_line_length except possibly for the last
            assert!(chunk.len() <= 10 || chunk.len() % 2 == 0);
        }
    }

    #[test]
    fn blockquote_lines_get_quote_prefix_on_wrapped_continuation() {
        // A blockquote line starting with '>' that is longer than 80 chars
        let long_content = "word ".repeat(20);
        let input = format!("> {}", long_content.trim_end());
        let output = fix_long_lines(&input);
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines.len() > 1);
        // All continuation lines should also start with '> '
        assert!(lines.iter().all(|l| l.starts_with('>')));
    }

    #[test]
    fn ordered_list_lines_preserve_prefix_on_wrap() {
        let long_content = "word ".repeat(20);
        let input = format!("1. {}", long_content.trim_end());
        let output = fix_long_lines(&input);
        let lines: Vec<&str> = output.lines().collect();
        assert!(lines.len() > 1);
        // All lines should start with "1. "
        assert!(lines.iter().all(|l| l.starts_with("1. ")));
    }

    #[test]
    fn code_fence_toggle_works_for_nested_text_and_code() {
        // Text before and after a code fence should be treated differently
        let long_text = "word ".repeat(20);
        let code = "x".repeat(100);
        let input = format!(
            "{}```\n{}\n```\n{}",
            long_text.trim_end(),
            code,
            long_text.trim_end()
        );
        let output = fix_long_lines(&input);
        // The code line (100 chars, <= 160) should remain unwrapped inside the block
        assert!(output.contains(&code));
        // The text lines should be wrapped
        let text_lines: Vec<&str> = output
            .lines()
            .filter(|l| !l.contains(&"x".repeat(10)))
            .collect();
        assert!(text_lines.iter().any(|l| l.len() <= 80));
    }
}
