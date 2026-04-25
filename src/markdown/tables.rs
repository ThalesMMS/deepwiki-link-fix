/// Normalizes pipe-delimited Markdown table blocks by rewriting table rows so long cell contents are split and joined with `<br>`, while preserving non-table text.
///
/// Processes the input Markdown text, detects contiguous lines that begin with `|` as table blocks, and rewrites each table block so any cell whose trimmed content exceeds 30 characters is split on whitespace into segments of at most 30 characters and joined with `"<br>"`. Non-table lines and separator rows are preserved unchanged.
///
/// # Parameters
///
/// - `text`: Input Markdown text that may contain pipe-delimited table blocks.
///
/// # Returns
///
/// A new `String` where detected table blocks are rewritten with long cell contents wrapped using `"<br>"` and all other content is preserved.
///
/// # Examples
///
/// ```
/// let src = "Intro\n| header |\n| ------ |\n| this is a very long cell that should be wrapped into multiple segments |\nOutro\n";
/// let out = fix_table_content(src);
/// assert!(out.contains("<br>"));
/// assert!(out.starts_with("Intro\n"));
/// assert!(out.ends_with("Outro\n"));
/// ```
pub(super) fn fix_table_content(text: &str) -> String {
    let mut result = String::new();
    let mut in_table = false;
    let mut in_fence = false;
    let mut table_rows: Vec<String> = Vec::new();
    let mut table_ends_with_newline = false;

    for raw_line in text.split_inclusive('\n') {
        let line = raw_line.strip_suffix('\n').unwrap_or(raw_line);
        let trimmed = line.trim_start();
        let is_fence = trimmed.starts_with("```") || trimmed.starts_with("~~~");
        if is_fence {
            if in_table {
                flush_table_rows(&mut result, &table_rows, table_ends_with_newline);
                in_table = false;
                table_rows.clear();
            }
            in_fence = !in_fence;
            result.push_str(raw_line);
            continue;
        }

        // Detect Markdown tables
        if !in_fence && line.trim().starts_with('|') {
            if !in_table {
                in_table = true;
                table_rows.clear();
            }
            table_ends_with_newline = raw_line.ends_with('\n');
            table_rows.push(line.to_string());
            continue;
        } else if in_table {
            // End of table
            in_table = false;
            flush_table_rows(&mut result, &table_rows, table_ends_with_newline);
            table_rows.clear();
        }

        if !in_table {
            result.push_str(raw_line);
        }
    }

    // If the text ended with an open table
    if in_table && !table_rows.is_empty() {
        flush_table_rows(&mut result, &table_rows, table_ends_with_newline);
    }

    result
}

fn flush_table_rows(result: &mut String, table_rows: &[String], ends_with_newline: bool) {
    let mut fixed_table = fix_table_rows(table_rows);
    if !ends_with_newline && fixed_table.ends_with('\n') {
        fixed_table.pop();
    }
    result.push_str(&fixed_table);
}

/// Normalize and wrap a block of Markdown table rows.
///
/// Processes each line in `rows` as part of a single Markdown table:
/// - Trims cell contents.
/// - Leaves separator rows (e.g., `|---|---|`) unchanged.
/// - For data rows (lines starting with `|`), splits cells on `|`, trims each cell,
///   and if a cell's text is longer than 30 characters, splits it on whitespace into
///   segments of at most 30 characters and joins those segments with `<br>`.
/// The function returns a rebuilt table block where each data row is reformatted as
/// `| cell1 | cell2 | ... |` with a trailing newline for each row.
///
/// `rows` is expected to contain only the contiguous lines that form a single table.
///
/// # Examples
///
/// ```
/// let rows = vec![
///     "| This is a very long cell that should be wrapped into multiple segments | short |".to_string()
/// ];
/// let fixed = fix_table_rows(&rows);
/// assert!(fixed.contains("<br>"));
/// assert!(fixed.starts_with("| "));
/// ```
fn fix_table_rows(rows: &[String]) -> String {
    let mut result = String::new();

    for row in rows {
        if is_separator_row(row) {
            result.push_str(row);
            result.push('\n');
            continue;
        }

        if row.trim().starts_with('|') {
            let columns: Vec<&str> = row.split('|').collect();
            let mut fixed_columns: Vec<String> = Vec::new();

            for (i, col) in columns.iter().enumerate() {
                if i == 0 || i == columns.len() - 1 {
                    // The first and last columns are empty (before/after the |)
                    continue;
                }

                let col_content = col.trim();

                // Wrap very long columns
                if col_content.len() > 30 {
                    let words: Vec<&str> = col_content.split_whitespace().collect();
                    let mut current_line = String::new();
                    let mut lines = Vec::new();

                    for word in words {
                        if current_line.is_empty() {
                            current_line.push_str(word);
                        } else if current_line.len() + 1 + word.len() <= 30 {
                            current_line.push(' ');
                            current_line.push_str(word);
                        } else {
                            lines.push(current_line);
                            current_line = word.to_string();
                        }
                    }

                    if !current_line.is_empty() {
                        lines.push(current_line);
                    }

                    // Join lines with <br> so they wrap in the PDF
                    fixed_columns.push(lines.join("<br>"));
                } else {
                    fixed_columns.push(col_content.to_string());
                }
            }

            // Rebuild the table row
            result.push_str("|");
            for col in &fixed_columns {
                result.push(' ');
                result.push_str(col);
                result.push_str(" |");
            }
            result.push('\n');
        } else {
            // Separator row (|---|---|)
            result.push_str(row);
            result.push('\n');
        }
    }

    result
}

fn is_separator_row(row: &str) -> bool {
    let trimmed = row.trim();
    if !trimmed.starts_with('|') {
        return false;
    }

    let cells: Vec<&str> = trimmed.trim_matches('|').split('|').collect();
    !cells.is_empty() && cells.iter().all(|cell| is_separator_cell(cell))
}

fn is_separator_cell(cell: &str) -> bool {
    let trimmed = cell.trim();
    let trimmed = trimmed.strip_prefix(':').unwrap_or(trimmed);
    let trimmed = trimmed.strip_suffix(':').unwrap_or(trimmed);

    !trimmed.is_empty() && trimmed.chars().all(|ch| ch == '-')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wraps_long_table_columns_with_br_tags() {
        let rows = vec!["| a very long column value that wraps cleanly | short |".to_string()];

        assert_eq!(
            fix_table_rows(&rows),
            "| a very long column value that<br>wraps cleanly | short |\n"
        );
    }

    #[test]
    fn fixes_table_blocks_inside_markdown() {
        let input = "Before\n| a very long column value that wraps cleanly |\nAfter\n";

        assert_eq!(
            fix_table_content(input),
            "Before\n| a very long column value that<br>wraps cleanly |\nAfter\n"
        );
    }

    #[test]
    fn preserves_trailing_newline_semantics() {
        assert_eq!(fix_table_content("Before"), "Before");
        assert_eq!(fix_table_content("Before\n"), "Before\n");
        assert_eq!(fix_table_content("| cell |"), "| cell |");
        assert_eq!(fix_table_content("| cell |\n"), "| cell |\n");
    }

    #[test]
    fn ignores_table_like_rows_inside_fenced_code_blocks() {
        let input = "Before\n```\n| not | a | table |\n```\nAfter\n";

        assert_eq!(fix_table_content(input), input);
    }

    // --- Additional edge-case tests ---

    #[test]
    fn short_columns_are_not_wrapped() {
        let rows = vec!["| short | also short |".to_string()];
        let result = fix_table_rows(&rows);
        assert!(!result.contains("<br>"));
        assert_eq!(result, "| short | also short |\n");
    }

    #[test]
    fn separator_rows_are_preserved_unchanged() {
        let rows = vec!["| --- | --- |".to_string(), "| :-: | ---: |".to_string()];
        let result = fix_table_rows(&rows);

        assert_eq!(result, "| --- | --- |\n| :-: | ---: |\n");
    }

    #[test]
    fn table_separator_rows_remain_unrebuilt_inside_markdown() {
        let input = "| header | other |\n| --- | :-: |\n| value | cell |\n";

        assert_eq!(fix_table_content(input), input);
    }

    #[test]
    fn fix_table_content_handles_table_at_end_of_file_without_trailing_newline() {
        // Table is the last content and not followed by a blank line or newline
        let input = "Before\n| header |\n| data |";
        let result = fix_table_content(input);
        // Table should still be emitted, content preserved
        assert!(result.contains("| header |"));
        assert!(result.contains("| data |"));
    }

    #[test]
    fn fix_table_content_handles_table_at_start_of_file() {
        let input = "| col |\n| value |\nAfter\n";
        let result = fix_table_content(input);
        assert!(result.contains("| col |"));
        assert!(result.contains("After"));
    }

    #[test]
    fn fix_table_content_preserves_text_without_any_tables() {
        let input = "No table here.\nJust regular text.\n";
        assert_eq!(fix_table_content(input), input);
    }

    #[test]
    fn fix_table_content_handles_multiple_tables_in_sequence() {
        let input = "| col1 |\n| val1 |\n\n| col2 |\n| val2 |\n";
        let result = fix_table_content(input);
        assert!(result.contains("| col1 |"));
        assert!(result.contains("| col2 |"));
    }

    #[test]
    fn fix_table_rows_wraps_exactly_at_thirty_one_chars() {
        // A column with 31 chars should be split
        let long_col = "a".repeat(31);
        let row = format!("| {} |", long_col);
        let rows = vec![row];
        let result = fix_table_rows(&rows);
        // Since it's a single 31-char word with no spaces, it cannot be split further
        // (no whitespace to split on), so it stays as is
        assert!(result.contains(&"a".repeat(31)));
    }

    #[test]
    fn fix_table_rows_wraps_long_column_with_multiple_words() {
        // Column content: "word1 word2 word3 word4 word5 w6" - ensure wrapping occurs
        let row = "| word1 word2 word3 word4 word5 word6 |".to_string();
        let result = fix_table_rows(&[row]);
        assert!(result.contains("<br>"));
    }

    #[test]
    fn fix_table_rows_handles_empty_columns() {
        let rows = vec!["| | value | |".to_string()];
        // Should not panic; empty columns produce empty output
        let result = fix_table_rows(&rows);
        assert!(result.contains('|'));
    }

    #[test]
    fn fix_table_content_table_followed_by_blank_line_is_terminated_correctly() {
        let input = "| col |\n| val |\n\nSome text after\n";
        let result = fix_table_content(input);
        assert!(result.contains("| col |"));
        assert!(result.contains("Some text after"));
    }

    #[test]
    fn fix_table_content_consecutive_table_rows_all_included() {
        let input = "| h1 | h2 |\n| -- | -- |\n| a  | b  |\n| c  | d  |\n";

        assert_eq!(
            fix_table_content(input),
            "| h1 | h2 |\n| -- | -- |\n| a | b |\n| c | d |\n"
        );
    }
}
