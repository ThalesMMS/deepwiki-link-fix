use super::*;

#[test]
fn keeps_unclosed_mermaid_block_for_pdf_fallback() {
    let images = tempfile::tempdir().unwrap();
    let input = "Intro\n```mermaid\nflowchart TD\nA[\"[docs](url)\"]\n";

    let output = process_mermaid_for_pdf_with_mmdc(input, images.path(), "test", false);

    assert!(output.contains("Intro\n"));
    assert!(output.contains("```\nflowchart TD\nA[\"Unsupported markdown: link\"]\n```\n"));
}

#[test]
fn process_projects_to_pdf_reports_conversion_failures() {
    let output = tempfile::tempdir().unwrap();
    let pdf = tempfile::tempdir().unwrap();
    let project = output.path().join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("README.md"), "# Project\n").unwrap();

    let err = process_projects_to_pdf_with_converter(output.path(), pdf.path(), |project, _| {
        Err(format!("conversion failed for {}", project.display()))
    })
    .unwrap_err();
    let message = err.to_string();

    assert!(message.contains("PDF conversion failed with 1 error(s)"));
    assert!(message.contains("project: conversion failed"));
}

#[test]
fn process_projects_to_pdf_returns_success_report() {
    let output = tempfile::tempdir().unwrap();
    let pdf = tempfile::tempdir().unwrap();
    let project = output.path().join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("README.md"), "# Project\n").unwrap();

    let report = process_projects_to_pdf_with_converter(output.path(), pdf.path(), |_, pdf_dir| {
        Ok(pdf_dir.join("project.pdf"))
    })
    .unwrap();

    assert_eq!(report.generated_count(), 1);
}

// --- Additional edge-case tests ---

#[test]
fn pdf_report_default_has_zero_count_and_no_errors() {
    let report = PdfReport::default();
    assert_eq!(report.generated_count(), 0);
    assert!(!report.has_errors());
}

#[test]
fn pdf_report_push_error_sets_has_errors_true() {
    let mut report = PdfReport::default();
    report.push_error("something went wrong".to_string());
    assert!(report.has_errors());
}

#[test]
fn pdf_process_error_display_includes_error_count_and_messages() {
    let mut report = PdfReport::default();
    report.push_error("conversion failed".to_string());
    let err = PdfProcessError::new(report);
    let s = format!("{}", err);
    assert!(s.contains("1 error(s)"));
    assert!(s.contains("- conversion failed"));
}

#[test]
fn pdf_process_error_display_includes_generated_count() {
    let report = PdfReport::default();
    let err = PdfProcessError::new(report);
    let s = format!("{}", err);
    assert!(s.contains("0 PDF file(s)"));
}

#[test]
fn process_mermaid_for_pdf_without_mmdc_emits_code_block() {
    let images = tempfile::tempdir().unwrap();
    let input = "Intro\n```mermaid\ngraph LR\nA-->B\n```\nOutro\n";
    let output = process_mermaid_for_pdf_with_mmdc(input, images.path(), "proj", false);

    assert!(output.contains("Intro\n"));
    assert!(output.contains("Outro\n"));
    assert!(output.contains("```\n"));
    assert!(output.contains("A-->B"));
}

#[test]
fn process_mermaid_for_pdf_preserves_non_mermaid_content() {
    let images = tempfile::tempdir().unwrap();
    let input = "# Header\nNormal text\n";
    let output = process_mermaid_for_pdf_with_mmdc(input, images.path(), "proj", false);
    assert_eq!(output, input);
}

#[test]
fn process_mermaid_for_pdf_handles_multiple_mermaid_blocks() {
    let images = tempfile::tempdir().unwrap();
    let input = "```mermaid\ngraph LR\nA-->B\n```\nMiddle\n```mermaid\nflowchart TD\nX-->Y\n```\n";
    let output = process_mermaid_for_pdf_with_mmdc(input, images.path(), "proj", false);
    // Both blocks should be converted to code blocks (no mmdc)
    assert!(output.contains("A-->B"));
    assert!(output.contains("X-->Y"));
    assert!(output.contains("Middle\n"));
}

#[test]
fn process_mermaid_for_pdf_sanitizes_node_labels() {
    let images = tempfile::tempdir().unwrap();
    let input = "```mermaid\nflowchart TD\nA[\"[bad link](url)\"]\n```\n";
    let output = process_mermaid_for_pdf_with_mmdc(input, images.path(), "proj", false);
    assert!(output.contains("Unsupported markdown: link"));
    assert!(!output.contains("[bad link](url)"));
}

#[test]
fn process_projects_skips_dirs_without_markdown_files() {
    let output = tempfile::tempdir().unwrap();
    let pdf = tempfile::tempdir().unwrap();
    // A project directory with only non-md files
    let project = output.path().join("project");
    fs::create_dir_all(&project).unwrap();
    fs::write(project.join("data.txt"), "plain text").unwrap();

    let report = process_projects_to_pdf_with_converter(output.path(), pdf.path(), |_, pdf_dir| {
        Ok(pdf_dir.join("project.pdf"))
    })
    .unwrap();

    // No markdown files → converter should not be called → 0 generated
    assert_eq!(report.generated_count(), 0);
}

#[test]
fn process_projects_skips_files_at_top_level() {
    let output = tempfile::tempdir().unwrap();
    let pdf = tempfile::tempdir().unwrap();
    // A file at the top level (not a directory) should be skipped
    fs::write(output.path().join("README.md"), "# Root\n").unwrap();

    let report = process_projects_to_pdf_with_converter(output.path(), pdf.path(), |_, pdf_dir| {
        Ok(pdf_dir.join("project.pdf"))
    })
    .unwrap();

    assert_eq!(report.generated_count(), 0);
}

#[test]
fn process_projects_handles_multiple_projects() {
    let output = tempfile::tempdir().unwrap();
    let pdf = tempfile::tempdir().unwrap();

    for name in &["alpha", "beta"] {
        let project = output.path().join(name);
        fs::create_dir_all(&project).unwrap();
        fs::write(project.join("README.md"), "# Doc\n").unwrap();
    }

    let report = process_projects_to_pdf_with_converter(output.path(), pdf.path(), |_, pdf_dir| {
        Ok(pdf_dir.join("project.pdf"))
    })
    .unwrap();

    assert_eq!(report.generated_count(), 2);
}

#[test]
fn get_sorted_markdown_files_excludes_readme_and_sorts_alphabetically() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("README.md"), "readme").unwrap();
    fs::write(dir.path().join("b_doc.md"), "b doc").unwrap();
    fs::write(dir.path().join("a_doc.md"), "a doc").unwrap();

    let files = get_sorted_markdown_files(dir.path()).unwrap();

    let names: Vec<&str> = files
        .iter()
        .map(|p| p.file_name().unwrap().to_str().unwrap())
        .collect();
    assert_eq!(names, vec!["a_doc.md", "b_doc.md"]);
    assert!(!names.contains(&"README.md"));
}

#[test]
fn get_sorted_markdown_files_returns_empty_for_no_md_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("notes.txt"), "text").unwrap();

    let files = get_sorted_markdown_files(dir.path()).unwrap();
    assert!(files.is_empty());
}

#[test]
fn get_sorted_markdown_files_returns_error_for_missing_dir() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nonexistent");

    let err = get_sorted_markdown_files(&missing).unwrap_err();
    assert!(err.contains("Failed to read directory"));
}

#[test]
fn consolidate_project_markdown_includes_readme_and_section_separators() {
    let dir = tempfile::tempdir().unwrap();
    let images = tempfile::tempdir().unwrap();

    fs::write(dir.path().join("README.md"), "# Project\nIntro text\n").unwrap();
    fs::write(dir.path().join("01-section.md"), "# Section\nContent\n").unwrap();

    let result = consolidate_project_markdown(dir.path(), images.path()).unwrap();

    assert!(result.contains("Intro text"));
    assert!(result.contains("Section"));
    assert!(result.contains("\n\n---\n\n"));
}

#[test]
fn consolidate_project_markdown_skips_readme_first_header_line() {
    let dir = tempfile::tempdir().unwrap();
    let images = tempfile::tempdir().unwrap();

    fs::write(dir.path().join("README.md"), "# Title\nBody text\n").unwrap();

    let result = consolidate_project_markdown(dir.path(), images.path()).unwrap();

    // The first header "# Title" should be stripped
    assert!(!result.starts_with("# Title"));
    assert!(result.contains("Body text"));
}

#[test]
fn readme_title_removal_skips_first_visible_header_only() {
    let content = "\n\n# Project\nIntro\n## Keep me\n".to_string();

    assert_eq!(
        readme_content_without_title(content),
        "\n\nIntro\n## Keep me"
    );
}

#[test]
fn escapes_latex_special_characters_in_project_titles() {
    let escaped = escape_latex(r"a\b&%$#_{}~^");

    assert_eq!(
        escaped,
        r"a\textbackslash{}b\&\%\$\#\_\{\}\textasciitilde{}\textasciicircum{}"
    );
}

#[test]
fn consolidate_project_markdown_works_without_readme() {
    let dir = tempfile::tempdir().unwrap();
    let images = tempfile::tempdir().unwrap();

    fs::write(dir.path().join("doc.md"), "# Doc\nContent\n").unwrap();

    let result = consolidate_project_markdown(dir.path(), images.path()).unwrap();
    assert!(result.contains("Content"));
}

#[test]
fn has_markdown_files_returns_true_when_md_exists() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("doc.md"), "# Doc\n").unwrap();
    assert!(has_markdown_files(dir.path()).unwrap());
}

#[test]
fn has_markdown_files_returns_false_when_no_md_files() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("notes.txt"), "text").unwrap();
    assert!(!has_markdown_files(dir.path()).unwrap());
}

#[test]
fn has_markdown_files_returns_error_for_missing_dir() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    let err = has_markdown_files(&missing).unwrap_err();
    assert!(err.contains("Failed to read directory"));
}
