# Markdown Cleanup Script

This repository contains tools (Python and Rust) that clean up Markdown files
exported from deepwiki.com and devin.ai. They focus on formatting artifacts,
link normalization, and Mermaid diagram safety.

## What it fixes

- Removes any lines that appear before the first Markdown header.
- Strips "Link copied!" artifacts wherever they appear.
- Converts deepwiki-style internal links like `/owner/repo/...` to
  `https://github.com/owner/repo/...`.
- Removes `blob/<sha>/` segments from GitHub links to point at repo paths.
- Replaces placeholder "Section" links with README anchors when they follow
  the known pattern.
- Sanitizes Mermaid node labels by removing unsupported markdown lists/links.
- Conservatively moves Yes/No branch labels to the correct edge when there is
  a real branching node and the target edge is unlabeled.
- Applies ordinal numbering based on the README list and updates links to the
  renamed files.

## Usage

### Rust (recommended)

```bash
# Default: ./input → ./output
cargo run

# Custom directories
cargo run -- ./input_dir ./output_dir

# Rewrite files in place
cargo run -- --in-place ./input_dir

# See what would change without writing
cargo run -- --dry-run ./input_dir ./output_dir
```

### PDF Conversion (Rust only)

Convert processed markdown projects to consolidated PDFs:

```bash
# Convert all projects in ./output to PDFs in ./output-pdf
cargo run -- --pdf

# Custom PDF output directory
cargo run -- --pdf --pdf-dir ./my-pdfs
```

**Requirements:**
- [pandoc](https://pandoc.org/) - for markdown to PDF conversion
- [mermaid-cli](https://github.com/mermaid-js/mermaid-cli) (optional) - for rendering mermaid diagrams

```bash
# Install pandoc (macOS)
brew install pandoc

# Install mermaid-cli (optional, for diagram rendering)
npm install -g @mermaid-js/mermaid-cli
```

Each project folder in `output/` becomes a single PDF with:
- Title page with project name
- Table of contents
- All markdown files consolidated in order
- Mermaid diagrams rendered as images (if mermaid-cli is installed)

### Python

```bash
# Write output to a new directory
python3 scripts/fix_docs.py input_dir output_dir

# Rewrite files in place
python3 scripts/fix_docs.py --in-place input_dir

# See what would change without writing
python3 scripts/fix_docs.py --dry-run input_dir output_dir
```

## Testing

Run the parity test to verify Python and Rust produce identical output:

```bash
./tests/test_parity.sh
```

## Notes

- Only `.md` files are modified; other files are copied as-is.
- Mermaid changes are intentionally conservative to avoid altering correct
  diagrams.
- Both implementations produce byte-identical output.

## License

MIT. See `LICENSE`.
