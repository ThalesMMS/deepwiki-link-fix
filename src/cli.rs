use std::path::PathBuf;

use clap::Parser;

use crate::directory::process_directory;
use crate::pdf::{mmdc_available, pandoc_available, process_projects_to_pdf};

#[derive(Parser, Debug)]
#[command(name = "fix-docs")]
#[command(about = "Normalize DeepWiki markdown links and mermaid diagrams.")]
struct Args {
    /// Input directory
    #[arg(default_value = "./input")]
    input_dir: PathBuf,

    /// Output directory
    #[arg(default_value = "./output")]
    output_dir: PathBuf,

    /// Rewrite files in place instead of writing to output_dir
    #[arg(long)]
    in_place: bool,

    /// Print files that would change without writing output
    #[arg(long)]
    dry_run: bool,

    /// Convert output projects to PDF (requires output folder to exist)
    #[arg(long)]
    pdf: bool,

    /// Output directory for PDF files (default: ./output-pdf)
    #[arg(long, default_value = "./output-pdf")]
    pdf_dir: PathBuf,
}

/// Entrypoint for the CLI: parses arguments and either rewrites markdown files or converts existing output to PDFs.
///
/// In normal mode, validates the input and output directories, processes the input directory to rewrite markdown into the effective output directory (which may be the input directory when run with `--in-place`), and, when run with `--dry-run`, prints the paths of files that would change. In `--pdf` mode, validates the output directory and required external tools, converts projects found in the output directory to PDF files in the configured PDF directory, and prints a summary of generated PDFs. On validation or processing errors the function prints a message to stderr and exits the process with a non-zero status.
///
/// # Examples
///
/// ```no_run
/// // Run the CLI entrypoint (parses process::args)
/// fix_docs::cli::run();
/// ```
pub(crate) fn run() {
    let args = Args::parse();

    // Handle --pdf mode: only convert existing output to PDF
    if args.pdf {
        let output_dir = args.output_dir.clone();

        if !output_dir.is_dir() {
            eprintln!(
                "Error: Output directory '{}' does not exist or is not a directory. Run without --pdf first to generate markdown.",
                output_dir.display()
            );
            std::process::exit(1);
        }

        if !pandoc_available() {
            eprintln!("Error: pandoc is required for PDF conversion but was not found.");
            eprintln!("Install with: brew install pandoc (macOS) or apt install pandoc (Linux)");
            std::process::exit(1);
        }

        if !mmdc_available() {
            eprintln!(
                "Warning: mermaid-cli (mmdc) not found. Mermaid diagrams will be shown as code blocks."
            );
            eprintln!("Install with: npm install -g @mermaid-js/mermaid-cli");
        }

        println!("Converting projects to PDF...");
        let report = match process_projects_to_pdf(&output_dir, &args.pdf_dir) {
            Ok(report) => report,
            Err(err) => {
                eprintln!("Error: {}", err);
                std::process::exit(1);
            }
        };
        println!("\nGenerated {} PDF file(s)", report.generated_count());
        return;
    }

    let output_dir = if args.in_place {
        args.input_dir.clone()
    } else {
        args.output_dir.clone()
    };

    if !args.input_dir.is_dir() {
        eprintln!(
            "Error: Input directory '{}' does not exist or is not a directory.",
            args.input_dir.display()
        );
        std::process::exit(1);
    }

    if !output_dir.is_dir() {
        eprintln!(
            "Error: Output directory '{}' does not exist or is not a directory.",
            output_dir.display()
        );
        std::process::exit(1);
    }

    let changed = match process_directory(&args.input_dir, &output_dir, args.dry_run) {
        Ok(changed) => changed,
        Err(err) => {
            eprintln!("Error: {}", err);
            std::process::exit(1);
        }
    };

    if args.dry_run {
        for path in changed {
            println!("{}", path.display());
        }
    }
}
