# Markdown Cleanup Script

This repository contains a small Python script that cleans up Markdown files
exported from deepwiki.com and devin.ai. It focuses on formatting artifacts,
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

```bash
# Write output to a new directory
python3 scripts/fix_docs.py input_dir output_dir

# Rewrite files in place
python3 scripts/fix_docs.py --in-place input_dir

# See what would change without writing
python3 scripts/fix_docs.py --dry-run input_dir output_dir
```

## Notes

- Only `.md` files are modified; other files are copied as-is.
- Mermaid changes are intentionally conservative to avoid altering correct
  diagrams.

## License

MIT. See `LICENSE`.
