use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use walkdir::WalkDir;

use crate::markdown::process_text;
use crate::readme::apply_readme_ordinal;

/// Determines whether any component of `path`, when interpreted relative to `root`, is a hidden name (starts with a dot).
///
/// The function strips `root` from `path` when possible; if stripping fails, `path` is inspected as-is.
///
/// # Returns
///
/// `true` if any path component (after making `path` relative to `root`) starts with `.`, `false` otherwise.
///
/// # Examples
///
/// ```
/// use std::path::Path;
///
/// // Hidden component in subdirectory
/// let path = Path::new("/repo/.git/config");
/// let root = Path::new("/repo");
/// assert!(has_hidden_component(path, root));
///
/// // No hidden components
/// let path = Path::new("/repo/src/main.rs");
/// assert!(!has_hidden_component(path, root));
///
/// // When strip_prefix fails, the function inspects the original path
/// let path = Path::new(".hidden/file");
/// let root = Path::new("/other");
/// assert!(has_hidden_component(path, root));
/// ```
fn has_hidden_component(path: &Path, root: &Path) -> bool {
    let rel_path = path.strip_prefix(root).unwrap_or(path);
    rel_path.components().any(|component| {
        component
            .as_os_str()
            .to_str()
            .map_or(false, |name| name.starts_with('.'))
    })
}

/// Process an input directory tree into an output directory, transforming Markdown files and copying other files while optionally performing a dry run.
///
/// On success, returns a `Vec<PathBuf>` containing the output paths for Markdown files whose processed content differs from the originals.
/// Returns `Err(String)` if walking the input or output directories, reading or writing files, copying files, or creating directories fails.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use tempfile::tempdir;
/// // prepare temp input/output directories and call with dry_run = true
/// let input = tempdir().unwrap();
/// let output = tempdir().unwrap();
/// let changed = crate::directory::process_directory(input.path(), output.path(), true).unwrap();
/// assert!(changed.is_empty() || changed.iter().all(|p| p.as_path().starts_with(output.path())));
/// ```
pub(crate) fn process_directory(
    input_dir: &Path,
    output_dir: &Path,
    dry_run: bool,
) -> Result<Vec<PathBuf>, String> {
    let mut changed_files: Vec<PathBuf> = Vec::new();
    let mut readme_parent_dirs: HashSet<PathBuf> = HashSet::new();

    for entry in WalkDir::new(input_dir)
        .into_iter()
        .filter_entry(|entry| !has_hidden_component(entry.path(), input_dir))
    {
        let entry = entry.map_err(|e| format!("Failed to walk input directory: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            continue;
        }

        let rel_path = path.strip_prefix(input_dir).unwrap_or(path);
        let out_path = output_dir.join(rel_path);

        if path.file_name().map_or(false, |f| f == "README.md") {
            readme_parent_dirs.insert(
                rel_path
                    .parent()
                    .map_or_else(PathBuf::new, Path::to_path_buf),
            );
        }

        if path.extension().map_or(true, |e| e != "md") {
            // Copy non-markdown files
            if !dry_run {
                if let Some(parent) = out_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!("Failed to create directory '{}': {}", parent.display(), e)
                    })?;
                }
                fs::copy(path, &out_path).map_err(|e| {
                    format!(
                        "Failed to copy '{}' to '{}': {}",
                        path.display(),
                        out_path.display(),
                        e
                    )
                })?;
            }
            continue;
        }

        // Process markdown files
        let original = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read '{}': {}", path.display(), e))?;
        let updated = process_text(&original);
        let changed = updated != original;
        if !dry_run {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent).map_err(|e| {
                    format!("Failed to create directory '{}': {}", parent.display(), e)
                })?;
            }
            fs::write(&out_path, &updated)
                .map_err(|e| format!("Failed to write '{}': {}", out_path.display(), e))?;
        }
        if changed {
            changed_files.push(out_path.clone());
        }
    }

    // Apply ordinal renaming only for README parents discovered in the filtered input tree.
    for parent_rel_path in readme_parent_dirs {
        let output_parent = output_dir.join(&parent_rel_path);
        let read_parent = if dry_run {
            input_dir.join(&parent_rel_path)
        } else {
            output_parent.clone()
        };
        changed_files.extend(apply_readme_ordinal(&read_parent, &output_parent, dry_run)?);
    }

    Ok(changed_files)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn processes_markdown_and_copies_non_markdown_files() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();

        fs::write(
            input.path().join("doc.md"),
            "preamble\n# Title\nSee [repo](/owner/repo/path).\n",
        )
        .unwrap();
        fs::write(input.path().join("asset.txt"), "plain").unwrap();

        let changed = process_directory(input.path(), output.path(), false).unwrap();

        assert_eq!(changed, vec![output.path().join("doc.md")]);
        assert_eq!(
            fs::read_to_string(output.path().join("doc.md")).unwrap(),
            "# Title\nSee [repo](https://github.com/owner/repo/path).\n"
        );
        assert_eq!(
            fs::read_to_string(output.path().join("asset.txt")).unwrap(),
            "plain"
        );
    }

    #[test]
    fn dry_run_reports_markdown_changes_without_writing_output() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();

        fs::write(input.path().join("doc.md"), "# Title Link copied!\n").unwrap();

        let changed = process_directory(input.path(), output.path(), true).unwrap();

        assert_eq!(changed, vec![output.path().join("doc.md")]);
        assert!(!output.path().join("doc.md").exists());
    }

    #[test]
    fn applies_readme_ordinals_after_processing_output() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();

        fs::write(
            input.path().join("README.md"),
            "# Docs\n- [First](first.md)\n",
        )
        .unwrap();
        fs::write(input.path().join("first.md"), "# First\n").unwrap();

        let changed = process_directory(input.path(), output.path(), false).unwrap();

        assert!(changed.contains(&output.path().join("01-first.md")));
        assert!(!output.path().join("first.md").exists());
        assert!(output.path().join("01-first.md").exists());
        assert_eq!(
            fs::read_to_string(output.path().join("README.md")).unwrap(),
            "# Docs\n- [First](01-first.md)\n"
        );
    }

    #[test]
    fn dry_run_reports_readme_ordinals_from_input_tree() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();

        fs::write(
            input.path().join("README.md"),
            "# Docs\n- [First](first.md)\n",
        )
        .unwrap();
        fs::write(input.path().join("first.md"), "# First\n").unwrap();

        let changed = process_directory(input.path(), output.path(), true).unwrap();

        assert!(changed.contains(&output.path().join("01-first.md")));
        assert!(!output.path().join("README.md").exists());
        assert!(!output.path().join("01-first.md").exists());
    }

    #[test]
    fn skips_hidden_directories_and_their_descendants() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let hidden_dir = input.path().join(".git");
        fs::create_dir_all(&hidden_dir).unwrap();
        fs::write(hidden_dir.join("config.md"), "# Hidden Link copied!\n").unwrap();

        let changed = process_directory(input.path(), output.path(), false).unwrap();

        assert!(changed.is_empty());
        assert!(!output.path().join(".git").exists());
    }

    #[test]
    fn returns_error_when_input_walk_fails() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let missing = input.path().join("missing");

        let err = process_directory(&missing, output.path(), false).unwrap_err();

        assert!(err.contains("Failed to walk input directory"));
    }

    #[test]
    fn returns_error_when_write_destination_fails() {
        let input = tempfile::tempdir().unwrap();
        let output_file = tempfile::NamedTempFile::new().unwrap();
        fs::write(input.path().join("doc.md"), "# Title Link copied!\n").unwrap();

        let err = process_directory(input.path(), output_file.path(), false).unwrap_err();

        assert!(err.contains("Failed to create directory"));
    }

    // --- Additional edge-case tests ---

    #[test]
    fn empty_input_directory_produces_no_changes() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();

        let changed = process_directory(input.path(), output.path(), false).unwrap();

        assert!(changed.is_empty());
    }

    #[test]
    fn unchanged_markdown_file_is_not_reported_as_changed() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();

        // This markdown has no preamble, no artifacts, and no internal links — it
        // will pass through process_text unchanged
        let content = "# Title\n\nSome body text.\n";
        fs::write(input.path().join("doc.md"), content).unwrap();

        let changed = process_directory(input.path(), output.path(), false).unwrap();

        // Output file should exist
        assert!(output.path().join("doc.md").exists());
        // File content unchanged → not in "changed" list
        assert!(!changed.contains(&output.path().join("doc.md")));
    }

    #[test]
    fn skips_dot_files_at_root_level() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();

        // A hidden file (starts with '.') at the root level
        fs::write(input.path().join(".hidden.md"), "# Hidden\n").unwrap();

        let changed = process_directory(input.path(), output.path(), false).unwrap();

        assert!(changed.is_empty());
        assert!(!output.path().join(".hidden.md").exists());
    }

    #[test]
    fn nested_subdirectory_files_are_processed() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let sub = input.path().join("sub");
        fs::create_dir_all(&sub).unwrap();

        fs::write(sub.join("nested.md"), "preamble\n# Nested\n").unwrap();

        let changed = process_directory(input.path(), output.path(), false).unwrap();

        let expected_out = output.path().join("sub").join("nested.md");
        assert!(expected_out.exists());
        assert!(changed.contains(&expected_out));
        // Preamble stripped
        assert_eq!(fs::read_to_string(&expected_out).unwrap(), "# Nested\n");
    }

    #[test]
    fn dry_run_does_not_copy_non_markdown_files() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();

        fs::write(input.path().join("image.png"), &[0u8; 4]).unwrap();

        let changed = process_directory(input.path(), output.path(), true).unwrap();

        // Non-markdown file should not appear as changed
        assert!(changed.is_empty());
        // And should not have been copied
        assert!(!output.path().join("image.png").exists());
    }

    #[test]
    fn non_markdown_files_in_subdirs_are_copied() {
        let input = tempfile::tempdir().unwrap();
        let output = tempfile::tempdir().unwrap();
        let sub = input.path().join("assets");
        fs::create_dir_all(&sub).unwrap();

        fs::write(sub.join("style.css"), "body { color: red; }").unwrap();

        process_directory(input.path(), output.path(), false).unwrap();

        let out_css = output.path().join("assets").join("style.css");
        assert!(out_css.exists());
        assert_eq!(
            fs::read_to_string(&out_css).unwrap(),
            "body { color: red; }"
        );
    }

    #[test]
    fn has_hidden_component_detects_hidden_root_path() {
        let path = std::path::Path::new(".hidden/file.md");
        let root = std::path::Path::new("/other");
        // strip_prefix will fail, so path is inspected as-is
        assert!(has_hidden_component(path, root));
    }

    #[test]
    fn has_hidden_component_detects_hidden_in_subdirectory() {
        let path = std::path::Path::new("/repo/.git/config");
        let root = std::path::Path::new("/repo");
        assert!(has_hidden_component(path, root));
    }

    #[test]
    fn has_hidden_component_returns_false_for_visible_paths() {
        let path = std::path::Path::new("/repo/src/main.rs");
        let root = std::path::Path::new("/repo");
        assert!(!has_hidden_component(path, root));
    }
}
