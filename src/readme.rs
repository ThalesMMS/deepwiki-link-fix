use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};

use once_cell::sync::Lazy;
use regex::Regex;
use walkdir::WalkDir;

use crate::markdown::process_text;

static MD_LINK_TARGET_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\]\(([^)]+)\)").unwrap());
static README_MULTILINE_LINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\]\([^)]*\n[^)]*\)").unwrap());
static README_LIST_ITEM_LINK_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*-\s+\[[^\]]+\]\(([^)]+)\)\s*$").unwrap());
static ORDINAL_PREFIX_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{2,}-").unwrap());

/// Validate and convert a README link target into a normalized relative path using forward slashes.
///
/// This function verifies that `target_base` is a non-empty, relative file path that does not
/// contain parent-directory components (`..`) or root/prefix components, and returns the path
/// composed only of its normal components joined with `/`.
///
/// # Parameters
///
/// - `target_base`: The README link target string to validate and normalize.
///
/// # Returns
///
/// - `Ok(String)` containing the normalized relative path with components joined by `/`.
/// - `Err(String)` with a descriptive error when `target_base` is empty, absolute, contains
///   parent-directory components, contains root/prefix components, or has no normal path components.
///
/// # Examples
///
/// ```
/// // Normalizes "./docs/intro.md" -> "docs/intro.md"
/// let ok = normalize_readme_target("./docs/intro.md").unwrap();
/// assert_eq!(ok, "docs/intro.md");
///
/// // Rejects parent-directory components
/// assert!(normalize_readme_target("../outside.md").is_err());
/// ```
fn normalize_readme_target(target_base: &str) -> Result<String, String> {
    let target_path = Path::new(target_base);
    if target_base.is_empty() {
        return Err("README target is empty".to_string());
    }
    if target_path.is_absolute() {
        return Err(format!("README target '{}' must be relative", target_base));
    }

    let mut parts = Vec::new();
    for component in target_path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                return Err(format!(
                    "README target '{}' must not contain parent-directory components",
                    target_base
                ));
            }
            Component::RootDir | Component::Prefix(_) => {
                return Err(format!("README target '{}' must be relative", target_base));
            }
        }
    }

    if parts.is_empty() {
        return Err(format!(
            "README target '{}' is not a file path",
            target_base
        ));
    }

    Ok(parts.join("/"))
}

/// Extracts Markdown list-item link targets from a README string, validating and keeping only local `.md` targets.
///
/// This scans each line for list items like `- [text](target)` (including targets containing `#anchor`),
/// ignores external `http(s)://` links and non-`.md` targets, normalizes and validates the target path (rejecting
/// absolute paths, `..` components, root/prefix components, and empty targets), and returns the collected raw
/// targets in their original form (including any `#anchor`) in encounter order.
///
/// # Parameters
///
/// * `text` - The README content to parse.
///
/// # Returns
///
/// A `Vec<String>` of the matched link targets (each possibly including a `#anchor`) in the order they appear,
/// or an `Err(String)` describing the first validation error encountered while normalizing a target.
///
/// # Examples
///
/// ```
/// let readme = r#"
/// - [Intro](intro.md)
/// - [Guide](docs/guide.md#usage)
/// - [External](https://example.com)
/// "#;
/// let items = parse_readme_index_from_str(readme).unwrap();
/// assert_eq!(items, vec!["intro.md".to_string(), "docs/guide.md#usage".to_string()]);
/// ```
fn parse_readme_index_from_str(text: &str) -> Result<Vec<String>, String> {
    // Fix broken links by removing newlines within markdown links
    let text = README_MULTILINE_LINK_RE
        .replace_all(text, |caps: &regex::Captures| caps[0].replace('\n', ""));

    let mut items = Vec::new();

    for line in text.lines() {
        if let Some(caps) = README_LIST_ITEM_LINK_RE.captures(line) {
            let target = caps.get(1).unwrap().as_str();
            if target.starts_with("http://") || target.starts_with("https://") {
                continue;
            }
            let base_target = target.split_once('#').map_or(target, |(base, _)| base);
            if !base_target.to_lowercase().ends_with(".md") {
                continue;
            }
            normalize_readme_target(base_target)?;
            items.push(target.to_string());
        }
    }

    Ok(items)
}

/// Extracts ordered markdown link targets from a README file.
///
/// Reads the file at `readme_path`, parses list-item markdown links, and returns the captured
/// link targets in the order they appear (each target may include an anchor fragment).
///
/// Returns `Err` if the file cannot be read or if parsing/validation of README link targets fails.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// // Assuming "tests/fixtures/README.md" exists for the test environment.
/// let targets = parse_readme_index(Path::new("tests/fixtures/README.md")).unwrap();
/// assert!(targets.iter().any(|t| t.ends_with(".md")));
/// ```
#[cfg(test)]
fn parse_readme_index(readme_path: &Path) -> Result<Vec<String>, String> {
    let text = fs::read_to_string(readme_path)
        .map_err(|e| format!("Failed to read '{}': {}", readme_path.display(), e))?;
    parse_readme_index_from_str(&text)
}

/// Builds a mapping from normalized README link targets to new filenames prefixed with two-digit ordinals.
///
/// The function parses ordered markdown list-item link targets from `readme_text`, normalizes each target
/// (removing `./` and directory components, rejecting absolute or parent references), and assigns a new
/// filename of the form `NN-<original-filename>` where `NN` is a two-digit ordinal based on the target's
/// position in the README (starting at 01). Directory structure is preserved (e.g. `docs/intro.md` -> `docs/01-intro.md`).
/// Targets whose basename already begins with a multi-digit ordinal prefix (matching `ORDINAL_PREFIX_RE`) are skipped.
///
/// # Parameters
///
/// - `readme_text`: the full contents of a README file to parse for list-item markdown links.
///
/// # Returns
///
/// A `HashMap` where each key is the normalized original target path (forward-slash separated, without `#` anchors)
/// and each value is the corresponding new path with an ordinal-prefixed filename. Returns `Err` if parsing or
/// normalization of any referenced target fails.
///
/// # Examples
///
/// ```
/// use std::collections::HashMap;
///
/// let md = "- [Intro](intro.md)\n- [Guide](guide/usage.md)\n";
/// let mapping = build_ordinal_mapping_from_str(md).unwrap();
/// assert_eq!(mapping.get("intro.md"), Some(&"01-intro.md".to_string()));
/// assert_eq!(mapping.get("guide/usage.md"), Some(&"guide/02-usage.md".to_string()));
/// ```
fn build_ordinal_mapping_from_str(readme_text: &str) -> Result<HashMap<String, String>, String> {
    let items = parse_readme_index_from_str(readme_text)?;
    let mut mapping = HashMap::new();

    for (idx, target) in items.iter().enumerate() {
        let target_base = target
            .split_once('#')
            .map_or(target.as_str(), |(base, _)| base);
        let normalized_target = normalize_readme_target(target_base)?;
        let target_path = Path::new(&normalized_target);
        let filename = target_path
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("");

        // Skip if already numbered
        if ORDINAL_PREFIX_RE.is_match(filename) {
            continue;
        }

        let new_name = format!("{:02}-{}", idx + 1, filename);
        let new_path = if let Some(parent) = target_path.parent() {
            if parent.as_os_str().is_empty() {
                new_name
            } else {
                format!("{}/{}", parent.display(), new_name)
            }
        } else {
            new_name
        };

        mapping.insert(normalized_target, new_path);
    }

    Ok(mapping)
}

#[cfg(test)]
fn build_ordinal_mapping(readme_path: &Path) -> Result<HashMap<String, String>, String> {
    let text = fs::read_to_string(readme_path)
        .map_err(|e| format!("Failed to read '{}': {}", readme_path.display(), e))?;
    build_ordinal_mapping_from_str(&text)
}

fn normalize_markdown_link_target(base_dir: &Path, target_part: &str) -> Option<String> {
    if target_part.is_empty() {
        return None;
    }

    let target_path = Path::new(target_part);
    if target_path.is_absolute() {
        return None;
    }

    let mut parts: Vec<String> = base_dir
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();

    for component in target_path.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().to_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("/"))
    }
}

fn relative_link_from_base(base_dir: &Path, target: &str) -> String {
    let base_parts: Vec<String> = base_dir
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();
    let target_parts: Vec<String> = Path::new(target)
        .components()
        .filter_map(|component| match component {
            Component::Normal(part) => Some(part.to_string_lossy().to_string()),
            _ => None,
        })
        .collect();

    let common_len = base_parts
        .iter()
        .zip(target_parts.iter())
        .take_while(|(left, right)| left == right)
        .count();

    let mut rel_parts = Vec::new();
    rel_parts.extend(std::iter::repeat("..".to_string()).take(base_parts.len() - common_len));
    rel_parts.extend(target_parts[common_len..].iter().cloned());
    rel_parts.join("/")
}

/// Rewrites markdown link targets in `text` using `mapping`, resolving links relative to `current_rel_path`.
fn rewrite_markdown_links(
    text: &str,
    mapping: &HashMap<String, String>,
    current_rel_path: &Path,
) -> String {
    let base_dir = current_rel_path.parent().unwrap_or_else(|| Path::new(""));

    MD_LINK_TARGET_RE
        .replace_all(text, |caps: &regex::Captures| {
            let target = caps.get(1).unwrap().as_str();

            if target.starts_with("http://") || target.starts_with("https://") {
                return caps[0].to_string();
            }

            let (target_part, anchor) = if let Some(pos) = target.find('#') {
                (&target[..pos], &target[pos..])
            } else {
                (target, "")
            };

            let Some(normalized_target) = normalize_markdown_link_target(base_dir, target_part)
            else {
                return caps[0].to_string();
            };

            if let Some(new_target) = mapping.get(&normalized_target) {
                let relative_target = relative_link_from_base(base_dir, new_target);
                format!("]({}{})", relative_target, anchor)
            } else {
                caps[0].to_string()
            }
        })
        .to_string()
}

fn validate_rename_plan(
    read_dir: &Path,
    output_dir: &Path,
    mapping: &HashMap<String, String>,
    dry_run: bool,
) -> Result<(), String> {
    let mut new_targets = HashSet::new();

    for (old, new) in mapping {
        if !new_targets.insert(new) {
            return Err(format!("Rename plan has duplicate target '{}'", new));
        }

        let old_path = if dry_run {
            read_dir.join(old)
        } else {
            output_dir.join(old)
        };
        let new_path = output_dir.join(new);
        if new_path.exists() && old_path != new_path {
            return Err(format!(
                "Rename target '{}' already exists and would conflict with '{}'",
                new_path.display(),
                old_path.display()
            ));
        }
    }

    Ok(())
}

/// Applies ordinal prefixes to markdown files referenced by the repository README and updates links.
///
/// Reads `output_dir/README.md`, normalizes and validates list-item markdown link targets, builds
/// a mapping from those targets to new names prefixed with two-digit ordinals (e.g. `01-name.md`),
/// rewrites markdown link targets throughout `output_dir` to use the new names (preserving `../`, `./`,
/// and `#anchor` fragments), and renames the referenced files on disk unless `dry_run` is true.
///
/// # Errors
///
/// Returns `Err(String)` with a descriptive message if reading or writing files, walking the output
/// directory, creating directories, or renaming files fails, or if README parsing/validation fails.
///
/// # Returns
///
/// A `Vec<PathBuf>` containing paths that were modified or would be modified: rewritten markdown file
/// paths (when content changed) and destination paths for renamed files. When `dry_run` is true, no
/// filesystem mutations are performed but the planned destination paths are still reported.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// // Run in dry-run mode to compute planned changes without mutating the filesystem.
/// let _ = apply_readme_ordinal(Path::new("output"), Path::new("output"), true);
/// ```
pub(crate) fn apply_readme_ordinal(
    read_dir: &Path,
    output_dir: &Path,
    dry_run: bool,
) -> Result<Vec<PathBuf>, String> {
    let readme_path = read_dir.join("README.md");
    if !readme_path.exists() {
        return Ok(Vec::new());
    }

    // Clean up the README first to fix broken links
    let original_readme = fs::read_to_string(&readme_path)
        .map_err(|e| format!("Failed to read '{}': {}", readme_path.display(), e))?;
    let cleaned_readme = process_text(&original_readme);
    let output_readme_path = output_dir.join("README.md");
    if cleaned_readme != original_readme && !dry_run {
        fs::write(&output_readme_path, &cleaned_readme)
            .map_err(|e| format!("Failed to write '{}': {}", output_readme_path.display(), e))?;
    }

    let mapping = build_ordinal_mapping_from_str(&cleaned_readme)?;
    if mapping.is_empty() {
        return Ok(Vec::new());
    }
    validate_rename_plan(read_dir, output_dir, &mapping, dry_run)?;

    let mut changed: Vec<PathBuf> = Vec::new();

    // Update links in all markdown files
    for entry in WalkDir::new(read_dir) {
        let entry = entry.map_err(|e| format!("Failed to walk README directory: {}", e))?;
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "md") {
            let rel_path = path.strip_prefix(read_dir).unwrap_or(path);
            let original = if rel_path == Path::new("README.md") {
                cleaned_readme.clone()
            } else {
                fs::read_to_string(path)
                    .map_err(|e| format!("Failed to read '{}': {}", path.display(), e))?
            };
            let out_path = output_dir.join(rel_path);
            let updated = rewrite_markdown_links(&original, &mapping, rel_path);
            if updated != original {
                if !dry_run {
                    if let Some(parent) = out_path.parent() {
                        fs::create_dir_all(parent).map_err(|e| {
                            format!("Failed to create directory '{}': {}", parent.display(), e)
                        })?;
                    }
                    fs::write(&out_path, &updated)
                        .map_err(|e| format!("Failed to write '{}': {}", out_path.display(), e))?;
                }
                changed.push(out_path);
            }
        }
    }

    // Rename files
    for (old, new) in &mapping {
        let old_path = if dry_run {
            read_dir.join(old)
        } else {
            output_dir.join(old)
        };
        let new_path = output_dir.join(new);

        if old_path == new_path {
            continue;
        }
        if old_path.exists() {
            if !dry_run {
                if let Some(parent) = new_path.parent() {
                    fs::create_dir_all(parent).map_err(|e| {
                        format!("Failed to create directory '{}': {}", parent.display(), e)
                    })?;
                }
                fs::rename(&old_path, &new_path).map_err(|e| {
                    format!(
                        "Failed to rename '{}' to '{}': {}",
                        old_path.display(),
                        new_path.display(),
                        e
                    )
                })?;
            }
            changed.push(new_path.clone());
        }
    }

    Ok(changed)
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn parses_readme_index_and_skips_external_non_markdown_and_numbered_items() {
        let dir = tempfile::tempdir().unwrap();
        let readme = dir.path().join("README.md");
        fs::write(
            &readme,
            "- [Intro](intro.md)\n- [External](https://example.com/doc.md)\n- [Image](image.png)\n- [Done](01-done.md)\n",
        )
        .unwrap();

        assert_eq!(
            parse_readme_index(&readme).unwrap(),
            vec!["intro.md".to_string(), "01-done.md".to_string()]
        );

        let mapping = build_ordinal_mapping(&readme).unwrap();
        assert_eq!(mapping.get("intro.md"), Some(&"01-intro.md".to_string()));
        assert!(!mapping.contains_key("01-done.md"));
    }

    #[test]
    fn readme_index_accepts_anchors_and_multi_digit_ordinals() {
        let dir = tempfile::tempdir().unwrap();
        let readme = dir.path().join("README.md");
        fs::write(
            &readme,
            "- [Intro](intro.md#top)\n- [Large](100-large.md#section)\n",
        )
        .unwrap();

        assert_eq!(
            parse_readme_index(&readme).unwrap(),
            vec![
                "intro.md#top".to_string(),
                "100-large.md#section".to_string()
            ]
        );

        let mapping = build_ordinal_mapping(&readme).unwrap();
        assert_eq!(mapping.get("intro.md"), Some(&"01-intro.md".to_string()));
        assert!(!mapping.contains_key("100-large.md"));
    }

    #[test]
    fn rejects_absolute_and_parent_dir_readme_targets() {
        let absolute_err = build_ordinal_mapping_from_str("- [Abs](/tmp/abs.md)\n").unwrap_err();
        let parent_err = build_ordinal_mapping_from_str("- [Parent](../shared.md)\n").unwrap_err();

        assert!(absolute_err.contains("must be relative"));
        assert!(parent_err.contains("must not contain parent-directory"));
    }

    #[test]
    fn normalizes_current_dir_components_in_readme_targets() {
        let mapping = build_ordinal_mapping_from_str("- [Intro](./docs/intro.md#top)\n").unwrap();

        assert_eq!(
            mapping.get("docs/intro.md"),
            Some(&"docs/01-intro.md".to_string())
        );
    }

    #[test]
    fn rewrites_relative_markdown_links_with_prefixes_and_anchors() {
        let mapping =
            HashMap::from([("docs/intro.md".to_string(), "docs/01-intro.md".to_string())]);
        let text = "[Intro](./docs/intro.md#top) [Parent](../docs/intro.md) [Web](https://example.com/docs/intro.md)";

        assert_eq!(
            rewrite_markdown_links(text, &mapping, Path::new("README.md")),
            "[Intro](docs/01-intro.md#top) [Parent](../docs/intro.md) [Web](https://example.com/docs/intro.md)"
        );
    }

    #[test]
    fn apply_readme_ordinal_rewrites_links_and_renames_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("README.md"),
            "# Docs\n- [Intro](intro.md)\n",
        )
        .unwrap();
        fs::write(dir.path().join("intro.md"), "# Intro\n").unwrap();
        fs::write(dir.path().join("other.md"), "[Intro](intro.md#hello)\n").unwrap();

        let changed = apply_readme_ordinal(dir.path(), dir.path(), false).unwrap();

        assert!(changed.contains(&dir.path().join("README.md")));
        assert!(changed.contains(&dir.path().join("other.md")));
        assert!(changed.contains(&dir.path().join("01-intro.md")));
        assert!(!dir.path().join("intro.md").exists());
        assert!(dir.path().join("01-intro.md").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("other.md")).unwrap(),
            "[Intro](01-intro.md#hello)\n"
        );
    }

    #[test]
    fn apply_readme_ordinal_preserves_readme_link_anchors() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("README.md"),
            "# Docs\n- [Intro](intro.md#top)\n",
        )
        .unwrap();
        fs::write(dir.path().join("intro.md"), "# Intro\n").unwrap();

        apply_readme_ordinal(dir.path(), dir.path(), false).unwrap();

        assert_eq!(
            fs::read_to_string(dir.path().join("README.md")).unwrap(),
            "# Docs\n- [Intro](01-intro.md#top)\n"
        );
        assert!(dir.path().join("01-intro.md").exists());
    }

    #[test]
    fn dry_run_builds_mapping_from_cleaned_readme_text() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("README.md"),
            "# Docs\\n- [Intro](intro.md)\\n",
        )
        .unwrap();
        fs::write(dir.path().join("intro.md"), "# Intro\n").unwrap();

        let changed = apply_readme_ordinal(dir.path(), dir.path(), true).unwrap();

        assert!(changed.contains(&dir.path().join("01-intro.md")));
        assert!(dir.path().join("intro.md").exists());
        assert_eq!(
            fs::read_to_string(dir.path().join("README.md")).unwrap(),
            "# Docs\\n- [Intro](intro.md)\\n"
        );
    }

    // --- Additional edge-case tests ---

    #[test]
    fn normalize_readme_target_rejects_empty_string() {
        let err = normalize_readme_target("").unwrap_err();
        assert!(err.contains("empty"));
    }

    #[test]
    fn normalize_readme_target_rejects_absolute_paths() {
        let err = normalize_readme_target("/absolute/path.md").unwrap_err();
        assert!(err.contains("must be relative"));
    }

    #[test]
    fn normalize_readme_target_rejects_parent_dir_traversal() {
        let err = normalize_readme_target("../outside.md").unwrap_err();
        assert!(err.contains("parent-directory"));
    }

    #[test]
    fn normalize_readme_target_strips_current_dir_components() {
        let result = normalize_readme_target("./docs/intro.md").unwrap();
        assert_eq!(result, "docs/intro.md");
    }

    #[test]
    fn normalize_readme_target_handles_plain_filename() {
        let result = normalize_readme_target("intro.md").unwrap();
        assert_eq!(result, "intro.md");
    }

    #[test]
    fn normalize_readme_target_handles_nested_paths() {
        let result = normalize_readme_target("a/b/c.md").unwrap();
        assert_eq!(result, "a/b/c.md");
    }

    #[test]
    fn parse_readme_index_from_str_returns_empty_for_no_list_items() {
        let result = parse_readme_index_from_str("# Header\nJust text\n").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_readme_index_from_str_skips_http_links() {
        let text = "- [External](https://example.com/file.md)\n";
        let result = parse_readme_index_from_str(text).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_readme_index_from_str_skips_non_md_targets() {
        let text = "- [Image](image.png)\n- [Text](file.txt)\n";
        let result = parse_readme_index_from_str(text).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn parse_readme_index_from_str_handles_multiline_link() {
        // Multiline links are fixed by removing the embedded newline
        let text = "- [Broken](\nbroken.md)\n";
        // After repair, "(\nbroken.md)" becomes "(broken.md)"
        let result = parse_readme_index_from_str(text).unwrap();
        assert_eq!(result, vec!["broken.md".to_string()]);
    }

    #[test]
    fn build_ordinal_mapping_from_str_assigns_sequential_ordinals() {
        let text = "- [A](a.md)\n- [B](b.md)\n- [C](c.md)\n";
        let mapping = build_ordinal_mapping_from_str(text).unwrap();
        assert_eq!(mapping.get("a.md"), Some(&"01-a.md".to_string()));
        assert_eq!(mapping.get("b.md"), Some(&"02-b.md".to_string()));
        assert_eq!(mapping.get("c.md"), Some(&"03-c.md".to_string()));
    }

    #[test]
    fn build_ordinal_mapping_from_str_skips_already_numbered_filenames() {
        let text = "- [Done](01-done.md)\n- [New](new.md)\n";
        let mapping = build_ordinal_mapping_from_str(text).unwrap();
        // "01-done.md" already has an ordinal prefix matching ORDINAL_PREFIX_RE (^\d{2,}-)
        assert!(!mapping.contains_key("01-done.md"));
        // "new.md" is at position 2 in the list
        assert_eq!(mapping.get("new.md"), Some(&"02-new.md".to_string()));
    }

    #[test]
    fn build_ordinal_mapping_from_str_preserves_subdirectory_structure() {
        let text = "- [Guide](docs/guide.md)\n";
        let mapping = build_ordinal_mapping_from_str(text).unwrap();
        assert_eq!(
            mapping.get("docs/guide.md"),
            Some(&"docs/01-guide.md".to_string())
        );
    }

    #[test]
    fn rewrite_markdown_links_preserves_external_links() {
        let mapping = HashMap::from([("intro.md".to_string(), "01-intro.md".to_string())]);
        let text = "[Web](https://example.com/intro.md)";
        assert_eq!(
            rewrite_markdown_links(text, &mapping, Path::new("README.md")),
            text
        );
    }

    #[test]
    fn rewrite_markdown_links_preserves_unmatched_targets() {
        let mapping = HashMap::from([("intro.md".to_string(), "01-intro.md".to_string())]);
        let text = "[Other](other.md)";
        assert_eq!(
            rewrite_markdown_links(text, &mapping, Path::new("README.md")),
            text
        );
    }

    #[test]
    fn rewrite_markdown_links_handles_multiple_parent_dir_prefixes() {
        let mapping = HashMap::from([("intro.md".to_string(), "01-intro.md".to_string())]);
        let text = "[Link](../../intro.md)";
        let result = rewrite_markdown_links(text, &mapping, Path::new("sub/deep/file.md"));
        assert_eq!(result, "[Link](../../01-intro.md)");
    }

    #[test]
    fn rewrites_links_relative_to_current_markdown_file() {
        let mapping =
            HashMap::from([("docs/intro.md".to_string(), "docs/01-intro.md".to_string())]);
        let text = "[Intro](intro.md#top)";

        let result = rewrite_markdown_links(text, &mapping, Path::new("docs/other.md"));

        assert_eq!(result, "[Intro](01-intro.md#top)");
    }

    #[test]
    fn aborts_before_rewriting_when_rename_target_exists() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("README.md"),
            "# Docs\n- [Intro](intro.md)\n",
        )
        .unwrap();
        fs::write(dir.path().join("intro.md"), "# Intro\n").unwrap();
        fs::write(dir.path().join("01-intro.md"), "# Existing\n").unwrap();
        fs::write(dir.path().join("other.md"), "[Intro](intro.md)\n").unwrap();

        let err = apply_readme_ordinal(dir.path(), dir.path(), false).unwrap_err();

        assert!(err.contains("already exists"));
        assert_eq!(
            fs::read_to_string(dir.path().join("other.md")).unwrap(),
            "[Intro](intro.md)\n"
        );
    }

    #[test]
    fn apply_readme_ordinal_returns_empty_when_no_readme_exists() {
        let dir = tempfile::tempdir().unwrap();
        let result = apply_readme_ordinal(dir.path(), dir.path(), false).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn apply_readme_ordinal_returns_empty_when_readme_has_no_links() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("README.md"), "# No links here\n").unwrap();
        let result = apply_readme_ordinal(dir.path(), dir.path(), false).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn apply_readme_ordinal_file_not_renamed_when_already_numbered() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("README.md"),
            "# Docs\n- [Done](01-done.md)\n",
        )
        .unwrap();
        fs::write(dir.path().join("01-done.md"), "# Done\n").unwrap();

        let changed = apply_readme_ordinal(dir.path(), dir.path(), false).unwrap();

        // File is already numbered; no rename should happen
        assert!(dir.path().join("01-done.md").exists());
        // No rename path should be in changed for a new name (it could appear as link rewrite)
        assert!(!changed.contains(&dir.path().join("02-01-done.md")));
    }

    #[test]
    fn apply_readme_ordinal_dry_run_does_not_rename_files() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(
            dir.path().join("README.md"),
            "# Docs\n- [Intro](intro.md)\n",
        )
        .unwrap();
        fs::write(dir.path().join("intro.md"), "# Intro\n").unwrap();

        let changed = apply_readme_ordinal(dir.path(), dir.path(), true).unwrap();

        // File should NOT be renamed in dry_run mode
        assert!(dir.path().join("intro.md").exists());
        // The planned destination should appear in changed list
        assert!(changed.contains(&dir.path().join("01-intro.md")));
    }

    #[test]
    fn apply_readme_ordinal_cleans_up_readme_before_parsing() {
        let dir = tempfile::tempdir().unwrap();
        // README has a "Link copied!" artifact that gets cleaned
        fs::write(
            dir.path().join("README.md"),
            "# Docs Link copied!\n- [Intro](intro.md)\n",
        )
        .unwrap();
        fs::write(dir.path().join("intro.md"), "# Intro\n").unwrap();

        let changed = apply_readme_ordinal(dir.path(), dir.path(), false).unwrap();

        // Cleanup should have run and files renamed
        assert!(changed.contains(&dir.path().join("01-intro.md")));
        // README should have artifact removed
        let readme = fs::read_to_string(dir.path().join("README.md")).unwrap();
        assert!(!readme.contains("Link copied!"));
    }
}
