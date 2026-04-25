use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::markdown::sanitize_mermaid_block;

#[derive(Debug, Default)]
pub(crate) struct PdfReport {
    generated_pdfs: Vec<PathBuf>,
    errors: Vec<String>,
}

impl PdfReport {
    /// Report the number of generated PDF files recorded in this report.
    ///
    /// # Examples
    ///
    /// ```
    /// let report = crate::pdf::PdfReport::default();
    /// assert_eq!(report.generated_count(), 0);
    /// ```
    ///
    /// @returns The number of generated PDF paths stored in the report.
    pub(crate) fn generated_count(&self) -> usize {
        self.generated_pdfs.len()
    }

    /// Appends an error message to the report's error list.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut report = PdfReport::default();
    /// report.push_error("failed to convert".to_string());
    /// assert!(report.has_errors());
    /// ```
    fn push_error(&mut self, error: String) {
        self.errors.push(error);
    }

    /// Indicates whether the report contains any errors.
    ///
    /// # Returns
    ///
    /// `true` if the report has one or more error messages, `false` otherwise.
    fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }
}

#[derive(Debug)]
pub(crate) struct PdfProcessError {
    report: PdfReport,
}

impl PdfProcessError {
    /// Create a new `PdfProcessError` that wraps the given `PdfReport`.
    ///
    /// # Examples
    ///
    /// ```
    /// let report = PdfReport::default();
    /// let _err = PdfProcessError::new(report);
    /// ```
    fn new(report: PdfReport) -> Self {
        Self { report }
    }
}

impl fmt::Display for PdfProcessError {
    /// Formats the contained PdfReport into a human-readable error summary.
    ///
    /// The output starts with a single summary line containing the number of errors
    /// and the number of generated PDF files, followed by each recorded error on its
    /// own line prefixed with `- `.
    ///
    /// # Examples
    ///
    /// ```
    /// let mut report = PdfReport::default();
    /// report.push_error("conversion failed".into());
    /// let err = PdfProcessError::new(report);
    /// let s = format!("{}", err);
    /// assert!(s.contains("error(s)"));
    /// assert!(s.contains("- conversion failed"));
    /// ```
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(
            f,
            "PDF conversion failed with {} error(s); generated {} PDF file(s).",
            self.report.errors.len(),
            self.report.generated_pdfs.len()
        )?;
        for error in &self.report.errors {
            writeln!(f, "- {}", error)?;
        }
        Ok(())
    }
}

/// Checks whether the `pandoc` executable is available on PATH by running `pandoc --version`.
///
/// # Returns
///
/// `true` if invoking `pandoc --version` exits successfully, `false` otherwise.
///
/// # Examples
///
/// ```
/// let _ = pandoc_available();
/// ```
pub(crate) fn pandoc_available() -> bool {
    Command::new("pandoc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Checks whether the `mmdc` (Mermaid CLI) executable is available on the system PATH.
///
/// Returns `true` if running `mmdc --version` exits successfully, `false` otherwise.
///
/// # Examples
///
/// ```
/// let available = mmdc_available();
/// println!("mmdc available: {}", available);
/// ```
pub(crate) fn mmdc_available() -> bool {
    Command::new("mmdc")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Render a Mermaid diagram to a PNG file using the `mmdc` command-line tool.
///
/// The function writes `mermaid_code` to a temporary `.mmd` input file and invokes
/// `mmdc` to render that input to `output_path`.
///
/// # Errors
///
/// Returns `Err` with a human-readable string if writing the temp file, spawning
/// `mmdc`, or running the command fails. If `mmdc` runs but exits with a
/// non-zero status, the returned `Err` contains `mmdc`'s stderr decoded as UTF-8
/// (lossy).
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// let tmp = tempfile::tempdir().unwrap();
/// let out = tmp.path().join("diagram.png");
/// let mermaid = "graph LR; A-->B;";
/// render_mermaid_to_png(mermaid, &out).unwrap();
/// assert!(out.exists());
/// ```
fn render_mermaid_to_png(mermaid_code: &str, output_path: &Path) -> Result<(), String> {
    let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let input_file = temp_dir.path().join("diagram.mmd");

    fs::write(&input_file, mermaid_code).map_err(|e| e.to_string())?;

    let output = Command::new("mmdc")
        .arg("-i")
        .arg(&input_file)
        .arg("-o")
        .arg(output_path)
        .arg("-b")
        .arg("white")
        .arg("-t")
        .arg("default")
        .output()
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).to_string())
    }
}

/// Appends a sanitized Mermaid diagram to `result` as either a generated image reference or a fenced code block.
///
/// When `has_mmdc` is true the function attempts to render `mermaid_content` to a PNG file located in
/// `images_dir` named `{prefix}-diagram-{diagram_count}.png`; on success it appends a Markdown image
/// reference for that file to `result`. If rendering fails, or if `has_mmdc` is false, it appends the
/// sanitized Mermaid source as a fenced code block instead.
///
/// # Parameters
///
/// - `result`: accumulator to which the function appends the output.
/// - `mermaid_content`: the raw Mermaid source for the diagram; it is sanitized before any output or render attempt.
/// - `images_dir`: directory where generated diagram images should be written when `has_mmdc` is true.
/// - `prefix`: filename prefix used when naming generated image files.
/// - `diagram_count`: index used to build a unique image filename and to label the image in the Markdown.
/// - `has_mmdc`: if `true`, attempt to render the diagram with `mmdc` and emit an image reference on success;
///   if `false`, always emit a fenced code block with the sanitized source.
///
/// # Examples
///
/// ```
/// # use std::path::Path;
/// let mut out = String::new();
/// let mermaid = "graph LR\nA-->B";
/// // when mmdc is unavailable, the function emits a fenced code block with the sanitized source
/// push_mermaid_pdf_block(&mut out, mermaid, Path::new("/tmp"), "sec1", 1, false);
/// assert!(out.contains("```"));
/// assert!(out.contains("graph LR"));
/// ```
fn push_mermaid_pdf_block(
    result: &mut String,
    mermaid_content: &str,
    images_dir: &Path,
    prefix: &str,
    diagram_count: usize,
    has_mmdc: bool,
) {
    // Sanitize the mermaid content before rendering
    let mermaid_lines: Vec<String> = mermaid_content.lines().map(|s| s.to_string()).collect();
    let sanitized = sanitize_mermaid_block(&mermaid_lines);
    let sanitized_content = sanitized.join("\n");

    if has_mmdc {
        let img_name = format!("{}-diagram-{}.png", prefix, diagram_count);
        let img_path = images_dir.join(&img_name);

        match render_mermaid_to_png(&sanitized_content, &img_path) {
            Ok(_) => {
                result.push_str(&format!(
                    "\n![Diagram {}]({})\n\n",
                    diagram_count,
                    img_path.display()
                ));
            }
            Err(e) => {
                eprintln!("Warning: Failed to render mermaid diagram: {}", e);
                result.push_str("\n```\n");
                result.push_str(&sanitized_content);
                result.push_str("\n```\n\n");
            }
        }
    } else {
        // No mmdc available, keep as code block
        result.push_str("\n```\n");
        result.push_str(&sanitized_content);
        result.push_str("\n```\n\n");
    }
}

/// Transforms Markdown by converting fenced Mermaid blocks into image references when rendering
/// is available, or by preserving sanitized Mermaid fenced code blocks otherwise.
///
/// The returned string is the input Markdown with each Mermaid block either replaced by a
/// generated image link (written under `images_dir` using `prefix`) or emitted as a sanitized
/// fenced Mermaid code block if rendering is not available or fails.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let md = "Intro\n```mermaid\nA-->B\n```\n";
/// let out = process_mermaid_for_pdf(md, Path::new("."), "p");
/// // Output either contains the raw Mermaid content (sanitized) or an image reference.
/// assert!(out.contains("A-->B") || out.contains("!["));
/// ```
fn process_mermaid_for_pdf(text: &str, images_dir: &Path, prefix: &str) -> String {
    process_mermaid_for_pdf_with_mmdc(text, images_dir, prefix, mmdc_available())
}

/// Processes Markdown text and converts fenced Mermaid blocks into PDF-friendly content.
///
/// When a Mermaid fenced block is found, the block is either replaced with a generated
/// image reference (if `has_mmdc` is true and rendering succeeds) or retained as a
/// sanitized Mermaid fenced code block. Non-Mermaid content is preserved with original line breaks.
///
/// # Parameters
///
/// - `text`: input Markdown to scan for Mermaid fenced blocks.
/// - `images_dir`: directory where generated diagram images should be written (used to build image paths).
/// - `prefix`: filename prefix used when naming generated diagram images.
/// - `has_mmdc`: if `true`, attempts to render Mermaid diagrams to PNG and insert image links; if `false`, emits sanitized Mermaid code blocks.
///
/// # Returns
///
/// The transformed Markdown with Mermaid blocks replaced by image references or sanitized fenced code blocks.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let md = "Intro\n```mermaid\ngraph LR\nA-->B\n```\nEnd";
/// let out = process_mermaid_for_pdf_with_mmdc(md, Path::new("/tmp"), "proj", false);
/// assert!(out.contains("```mermaid"));
/// ```
fn process_mermaid_for_pdf_with_mmdc(
    text: &str,
    images_dir: &Path,
    prefix: &str,
    has_mmdc: bool,
) -> String {
    let mut result = String::new();
    let mut diagram_count = 0;
    let mut in_mermaid = false;
    let mut mermaid_content = String::new();

    for line in text.lines() {
        if line.starts_with("```") && line.contains("mermaid") {
            in_mermaid = true;
            mermaid_content.clear();
            continue;
        }

        if in_mermaid {
            if line.starts_with("```") {
                in_mermaid = false;
                diagram_count += 1;
                push_mermaid_pdf_block(
                    &mut result,
                    &mermaid_content,
                    images_dir,
                    prefix,
                    diagram_count,
                    has_mmdc,
                );
                mermaid_content.clear();
            } else {
                mermaid_content.push_str(line);
                mermaid_content.push('\n');
            }
            continue;
        }

        result.push_str(line);
        result.push('\n');
    }

    if in_mermaid {
        diagram_count += 1;
        push_mermaid_pdf_block(
            &mut result,
            &mermaid_content,
            images_dir,
            prefix,
            diagram_count,
            has_mmdc,
        );
    }

    result
}

/// Collects markdown files (excluding `README.md`) from a project directory and returns them sorted by filename.
///
/// Scans the provided `project_dir` for entries with the `.md` extension, omits `README.md`, sorts the remaining files by their file name, and returns their paths.
///
/// # Errors
///
/// Returns an `Err` with a human-readable message if the directory cannot be read or an entry cannot be accessed.
///
/// # Returns
///
/// A `Vec<PathBuf>` containing the paths to the discovered markdown files in filename-sorted order.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let files = get_sorted_markdown_files(Path::new("examples/project")).unwrap();
/// assert!(files.iter().all(|p| p.extension().and_then(|e| e.to_str()) == Some("md")));
/// ```
fn get_sorted_markdown_files(project_dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();

    for entry in fs::read_dir(project_dir).map_err(|e| {
        format!(
            "Failed to read directory '{}': {}",
            project_dir.display(),
            e
        )
    })? {
        let path = entry
            .map_err(|e| format!("Failed to read entry in '{}': {}", project_dir.display(), e))?
            .path();
        if path.extension().map_or(false, |e| e == "md")
            && path.file_name().map_or(false, |f| f != "README.md")
        {
            files.push(path);
        }
    }

    files.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    Ok(files)
}

fn readme_content_without_title(readme_content: String) -> String {
    let lines: Vec<&str> = readme_content.lines().collect();
    if let Some(header_idx) = lines
        .iter()
        .position(|line| !line.trim().is_empty() && line.trim_start().starts_with('#'))
    {
        let mut content_lines = lines;
        content_lines.remove(header_idx);
        content_lines.join("\n")
    } else {
        readme_content
    }
}

/// Consolidates a project's Markdown files into a single Markdown document suitable for PDF generation.
///
/// This reads `README.md` (if present) and then all other `*.md` files sorted by filename, processes Mermaid fenced blocks
/// (writing any generated images to `images_dir` and using per-file prefixes), and joins each file's content with a
/// `---` section separator.
///
/// If `README.md` begins with a leading header line (`# ...`), that first header line is removed to avoid duplicate titles.
/// Returns an `Err(String)` when reading files or enumerating markdown files fails; the error string describes the failure.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// let out = consolidate_project_markdown(Path::new("examples/project"), Path::new("/tmp/images")).unwrap();
/// // consolidated document should contain section separators
/// assert!(out.contains("\n\n---\n\n"));
/// ```
fn consolidate_project_markdown(project_dir: &Path, images_dir: &Path) -> Result<String, String> {
    let mut consolidated = String::new();

    // Read README if exists and add as intro
    let readme_path = project_dir.join("README.md");
    if readme_path.exists() {
        let readme_content = fs::read_to_string(&readme_path)
            .map_err(|e| format!("Failed to read '{}': {}", readme_path.display(), e))?;
        // Skip the first visible header if it exists (to avoid duplicate titles)
        let content = readme_content_without_title(readme_content);

        let processed = process_mermaid_for_pdf(&content, images_dir, "readme");
        consolidated.push_str(&processed);
        consolidated.push_str("\n\n---\n\n");
    }

    // Process each markdown file in order
    let files = get_sorted_markdown_files(project_dir)?;

    for (idx, file_path) in files.iter().enumerate() {
        let content = fs::read_to_string(file_path)
            .map_err(|e| format!("Failed to read '{}': {}", file_path.display(), e))?;
        let file_stem = file_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("section");

        let prefix = format!("sec{}-{}", idx + 1, file_stem);
        let processed = process_mermaid_for_pdf(&content, images_dir, &prefix);

        consolidated.push_str(&processed);
        consolidated.push_str("\n\n---\n\n");
    }

    Ok(consolidated)
}

fn escape_latex(s: &str) -> String {
    let mut escaped = String::new();
    for ch in s.chars() {
        match ch {
            '\\' => escaped.push_str(r"\textbackslash{}"),
            '&' => escaped.push_str(r"\&"),
            '%' => escaped.push_str(r"\%"),
            '$' => escaped.push_str(r"\$"),
            '#' => escaped.push_str(r"\#"),
            '_' => escaped.push_str(r"\_"),
            '{' => escaped.push_str(r"\{"),
            '}' => escaped.push_str(r"\}"),
            '~' => escaped.push_str(r"\textasciitilde{}"),
            '^' => escaped.push_str(r"\textasciicircum{}"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Generate a PDF for a project directory by consolidating its Markdown and invoking pandoc.
///
/// Consolidates the project's README and other Markdown files (processing Mermaid blocks and
/// writing images to a temporary directory), writes a small LaTeX title page, and runs `pandoc`
/// (xelatex) to produce `<project_name>.pdf` inside `pdf_dir`.
///
/// Returns `Ok(PathBuf)` with the created PDF path on success. Returns `Err(String)` with a
/// human-readable message if the project directory name is invalid, IO or temp-dir operations fail,
/// or if `pandoc` exits with a non-zero status (its stderr is included in the message).
///
/// # Examples
///
/// ```no_run
/// use std::path::Path;
///
/// // Attempt to convert the project at "output/my-project" and place the PDF in "pdfs"
/// let result = convert_project_to_pdf(Path::new("output/my-project"), Path::new("pdfs"));
/// match result {
///     Ok(path) => println!("PDF created at {:?}", path),
///     Err(err) => eprintln!("Conversion failed: {}", err),
/// }
/// ```
fn convert_project_to_pdf(project_dir: &Path, pdf_dir: &Path) -> Result<PathBuf, String> {
    let project_name = project_dir
        .file_name()
        .and_then(|f| f.to_str())
        .ok_or("Invalid project directory name")?;

    // Create temporary directory for images and markdown
    let temp_dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let images_dir = temp_dir.path();

    // Consolidate all markdown files
    let consolidated = consolidate_project_markdown(project_dir, images_dir)?;

    // Write consolidated markdown to temp file
    let md_path = temp_dir.path().join("consolidated.md");
    fs::write(&md_path, &consolidated).map_err(|e| e.to_string())?;

    // Create output PDF path
    let pdf_path = pdf_dir.join(format!("{}.pdf", project_name));

    // Write custom title page template
    let title_path = temp_dir.path().join("title.tex");
    let title_template = format!(
        r#"\begin{{titlepage}}
\centering
\vspace*{{3cm}}
{{\fontsize{{32}}{{40}}\selectfont\bfseries {} \par}}
\vfill
\end{{titlepage}}
"#,
        escape_latex(project_name)
    );
    fs::write(&title_path, &title_template).map_err(|e| e.to_string())?;

    let main_font = std::env::var("PDF_MAIN_FONT").unwrap_or_else(|_| "DejaVu Sans".to_string());
    let mono_font =
        std::env::var("PDF_MONO_FONT").unwrap_or_else(|_| "DejaVu Sans Mono".to_string());
    let mainfont_var = format!("mainfont:{}", main_font);
    let monofont_var = format!("monofont:{}", mono_font);

    // Convert to PDF using pandoc
    let output = Command::new("pandoc")
        .arg(&md_path)
        .arg("-o")
        .arg(&pdf_path)
        .arg("--pdf-engine=xelatex")
        .arg("-V")
        .arg("geometry:margin=0.7in")
        .arg("-V")
        .arg(&mainfont_var)
        .arg("-V")
        .arg(&monofont_var)
        .arg("-V")
        .arg("fontsize=9pt")
        .arg("-V")
        .arg("papersize=a4")
        .arg("-V")
        .arg("verbatim-font-size=8pt")
        .arg("-V")
        .arg("fancyhdr=false")
        .arg("-V")
        .arg("table-use-line-widths=true")
        .arg("-V")
        .arg("tables=true")
        .arg("-B")
        .arg(&title_path)
        .arg("--toc")
        .arg("--toc-depth=2")
        .arg("-f")
        .arg("markdown+emoji")
        .output()
        .map_err(|e| format!("Failed to run pandoc: {}", e))?;

    if output.status.success() {
        Ok(pdf_path)
    } else {
        Err(format!(
            "Pandoc failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ))
    }
}

/// Convert each project subdirectory under `output_dir` that contains Markdown files into PDFs placed in `pdf_dir`.
///
/// The function scans `output_dir` for project directories, consolidates Markdown (including Mermaid handling),
/// invokes the PDF converter for each project, and accumulates successes and errors into a `PdfReport`.
///
/// # Returns
///
/// `Ok(PdfReport)` with paths to created PDFs when all projects converted without unrecoverable setup errors;
/// `Err(PdfProcessError)` when one or more projects failed conversion or when an early setup error (like failing to create `pdf_dir`) occurs.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// // Convert projects found in "./output" into PDFs written to "./pdfs"
/// let report = process_projects_to_pdf(Path::new("./output"), Path::new("./pdfs"));
/// match report {
///     Ok(r) => println!("Generated {} PDFs", r.generated_count()),
///     Err(e) => eprintln!("Conversion failed: {}", e),
/// }
/// ```
pub(crate) fn process_projects_to_pdf(
    output_dir: &Path,
    pdf_dir: &Path,
) -> Result<PdfReport, PdfProcessError> {
    process_projects_to_pdf_with_converter(output_dir, pdf_dir, convert_project_to_pdf)
}

/// Checks whether the given directory contains any Markdown (`*.md`) files.
///
/// Returns `Ok(true)` if at least one entry in `project_dir` has an `md` extension,
/// `Ok(false)` if none are found, or `Err(String)` if the directory or an entry
/// cannot be read.
///
/// # Examples
///
/// ```ignore
/// use std::path::Path;
/// use std::fs::{self, File};
/// // create a temporary directory for the example
/// let dir = std::env::temp_dir().join("example_has_md");
/// let _ = fs::remove_dir_all(&dir);
/// fs::create_dir_all(&dir).unwrap();
/// File::create(dir.join("README.md")).unwrap();
///
/// assert!(has_markdown_files(Path::new(&dir)).unwrap());
/// ```
fn has_markdown_files(project_dir: &Path) -> Result<bool, String> {
    for entry in fs::read_dir(project_dir).map_err(|e| {
        format!(
            "Failed to read directory '{}': {}",
            project_dir.display(),
            e
        )
    })? {
        let path = entry
            .map_err(|e| format!("Failed to read entry in '{}': {}", project_dir.display(), e))?
            .path();
        if path.extension().map_or(false, |ext| ext == "md") {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Convert each project subdirectory under `output_dir` to a PDF using `convert`, collecting
/// successfully generated PDF paths and any errors into a `PdfReport`.
///
/// The function iterates over entries in `output_dir`, skips non-directories and directories
/// without Markdown files, invokes `convert(project_path, pdf_dir)` for each candidate, and
/// ensures `pdf_dir` exists. Successful conversions are recorded in `report.generated_pdfs`;
/// conversion failures are recorded as error messages inside the same report. If any errors
/// were recorded, a `PdfProcessError` wrapping the report is returned.
///
/// `convert` is a closure that receives the project directory path and the destination PDF
/// directory and should return the produced PDF path on success or a `String` error message.
///
/// # Returns
///
/// `Ok(PdfReport)` when no conversion errors were recorded; `Err(PdfProcessError)` when one or
/// more projects failed to convert (the wrapped report contains both successes and error messages).
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use tempfile::tempdir;
///
/// // Converter that always succeeds and returns a fixed PDF path in pdf_dir.
/// let converter = |_: &Path, pdf_dir: &Path| -> Result<std::path::PathBuf, String> {
///     Ok(pdf_dir.join("project.pdf"))
/// };
///
/// let out = tempdir().unwrap();
/// let pdf_dir = tempdir().unwrap();
///
/// // Assuming `process_projects_to_pdf_with_converter` is visible here:
/// // let result = process_projects_to_pdf_with_converter(out.path(), pdf_dir.path(), converter);
/// // (This example shows the simplest converter usage; test harnesses should create
/// // a suitable project layout under `out` before calling the function.)
/// ```
fn process_projects_to_pdf_with_converter<F>(
    output_dir: &Path,
    pdf_dir: &Path,
    mut convert: F,
) -> Result<PdfReport, PdfProcessError>
where
    F: FnMut(&Path, &Path) -> Result<PathBuf, String>,
{
    let mut report = PdfReport::default();

    // Create pdf_dir if it doesn't exist
    if let Err(e) = fs::create_dir_all(pdf_dir) {
        report.push_error(format!(
            "Failed to create PDF output directory '{}': {}",
            pdf_dir.display(),
            e
        ));
        return Err(PdfProcessError::new(report));
    }

    // Find all project directories (directories containing README.md or .md files)
    let entries = match fs::read_dir(output_dir) {
        Ok(entries) => entries,
        Err(e) => {
            report.push_error(format!(
                "Failed to read output directory '{}': {}",
                output_dir.display(),
                e
            ));
            return Err(PdfProcessError::new(report));
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                report.push_error(format!(
                    "Failed to read entry in '{}': {}",
                    output_dir.display(),
                    e
                ));
                continue;
            }
        };
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        // Check if it's a project directory (has markdown files)
        let has_md_files = match has_markdown_files(&path) {
            Ok(has_md_files) => has_md_files,
            Err(e) => {
                report.push_error(e);
                continue;
            }
        };

        if !has_md_files {
            continue;
        }

        let project_name = path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("unknown");

        println!("Converting project: {}", project_name);

        match convert(&path, pdf_dir) {
            Ok(pdf_path) => {
                println!("  Created: {}", pdf_path.display());
                report.generated_pdfs.push(pdf_path);
            }
            Err(e) => {
                report.push_error(format!("{}: {}", project_name, e));
            }
        }
    }

    if report.has_errors() {
        Err(PdfProcessError::new(report))
    } else {
        Ok(report)
    }
}

#[cfg(test)]
mod tests;
