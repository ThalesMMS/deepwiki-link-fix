use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Parser, Debug)]
#[command(name = "fix-docs")]
#[command(about = "Normalize DeepWiki markdown links and mermaid diagrams.")]
struct Args {
    /// Input directory
    #[arg(default_value = "./input")]
    input_dir: PathBuf,

    /// Output directory
    #[arg(default_value = "./output")]
    output_dir: Option<PathBuf>,

    /// Rewrite files in place instead of writing to output_dir
    #[arg(long)]
    in_place: bool,

    /// Print files that would change without writing output
    #[arg(long)]
    dry_run: bool,
}

// Section anchors mapping
fn get_section_anchors() -> HashMap<&'static str, &'static str> {
    let mut map = HashMap::new();
    map.insert("Networking Section", "networking-configuration");
    map.insert("Virtual Environment Section", "virtual-environment-setup");
    map.insert("Module Import Section", "module-import-issues");
    map.insert("WSL.exe Section", "wslexe-issues");
    map.insert("Path Translation Section", "path-translation-issues");
    map.insert("Performance Section", "performance-optimization");
    map.insert("Line Ending Section", "line-ending-issues");
    map.insert("Distribution Section", "distribution-selection");
    map
}

const BRANCH_LABELS: &[&str] = &["yes", "no", "true", "false"];
const POSITIVE_HINTS: &[&str] = &[
    "add", "use", "enable", "create", "remove", "success",
    "ready", "connected", "established", "proceed", "continue",
];
const NEGATIVE_HINTS: &[&str] = &[
    "fail", "error", "invalid", "reject", "timeout", "blocked",
    "missing", "not", "false", "empty", "return", "skip", "default",
];

#[derive(Clone)]
struct Edge {
    line_idx: usize,
    indent: String,
    src: String,
    arrow: String,
    label: Option<String>,
    dst: String,
}

fn fix_internal_links(text: &str) -> String {
    let internal_link_re = Regex::new(r"\]\((/[^)/\s]+/[^)/\s]+(?:/[^\s)]*)?)\)").unwrap();
    let ref_style_re = Regex::new(r"(?m)(^\s*\[[^\]]+\]:\s*)(/[^)/\s]+/[^)/\s]+(?:/[^\s)]*)?)").unwrap();
    
    let text = internal_link_re.replace_all(text, |caps: &regex::Captures| {
        format!("](https://github.com{})", &caps[1])
    });
    
    let text = ref_style_re.replace_all(&text, |caps: &regex::Captures| {
        format!("{}https://github.com{}", &caps[1], &caps[2])
    });
    
    text.to_string()
}

fn fix_section_links(text: &str) -> String {
    let section_anchors = get_section_anchors();
    let mut result = text.to_string();
    
    for (section, anchor) in section_anchors {
        let pattern = format!(
            r"https://github\.com/(?P<owner>[^/]+)/(?P<repo>[^/]+)/blob/(?P<sha>[0-9a-f]{{7,40}})/{}",
            regex::escape(section)
        );
        let re = Regex::new(&pattern).unwrap();
        result = re.replace_all(&result, |caps: &regex::Captures| {
            format!(
                "https://github.com/{}/{}/blob/{}/README.md#{}",
                &caps["owner"], &caps["repo"], &caps["sha"], anchor
            )
        }).to_string();
    }
    
    result
}

fn strip_github_blob_sha(text: &str) -> String {
    let re = Regex::new(r"https://github\.com/([^/]+)/([^/]+)/blob/([0-9a-f]{7,40})/").unwrap();
    re.replace_all(text, "https://github.com/$1/$2/").to_string()
}

fn strip_preamble(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut first_header_idx = None;
    
    for (idx, line) in lines.iter().enumerate() {
        if line.trim_start().starts_with('#') {
            first_header_idx = Some(idx);
            break;
        }
    }
    
    match first_header_idx {
        None | Some(0) => text.to_string(),
        Some(idx) => {
            let mut remaining = lines[idx..].join("\n");
            if text.ends_with('\n') {
                remaining.push('\n');
            }
            remaining
        }
    }
}

fn remove_link_copied(text: &str) -> String {
    let re = Regex::new(r"\s*Link copied!").unwrap();
    re.replace_all(text, "").to_string()
}

fn remove_ask_devin_lines(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let filtered: Vec<&str> = lines
        .into_iter()
        .filter(|line| !line.trim().starts_with("Ask Devin about"))
        .collect();
    let mut result = filtered.join("\n");
    if text.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn contains_list_marker(label: &str) -> bool {
    let list_item_re = Regex::new(r"^\s*(?:\d+\.\s+|[-*+]\s+)").unwrap();
    let parts: Vec<&str> = Regex::new(r"<br\s*/?>")
        .unwrap()
        .split(label)
        .collect();
    for part in parts {
        if list_item_re.is_match(part.trim()) {
            return true;
        }
    }
    false
}

fn sanitize_label(label: &str) -> String {
    if contains_list_marker(label) {
        return "Unsupported markdown: list".to_string();
    }
    let md_link_re = Regex::new(r"\[[^\]]+\]\([^)]+\)").unwrap();
    let url_re = Regex::new(r"https?://\S+").unwrap();
    let result = md_link_re.replace_all(label, "Unsupported markdown: link");
    let result = url_re.replace_all(&result, "Unsupported markdown: link");
    result.to_string()
}

fn sanitize_node_labels(lines: &[String]) -> Vec<String> {
    let node_label_re = Regex::new(r#"\["(.*?)"\]"#).unwrap();
    lines
        .iter()
        .map(|line| {
            node_label_re.replace_all(line, |caps: &regex::Captures| {
                let label = &caps[1];
                format!("[\"{}\"]", sanitize_label(label))
            }).to_string()
        })
        .collect()
}

fn is_branch_label(label: &Option<String>) -> bool {
    match label {
        None => false,
        Some(l) => BRANCH_LABELS.contains(&l.trim().to_lowercase().as_str()),
    }
}

fn label_polarity(label: &str) -> Option<&'static str> {
    let lowered = label.trim().to_lowercase();
    if lowered == "yes" || lowered == "true" {
        Some("positive")
    } else if lowered == "no" || lowered == "false" {
        Some("negative")
    } else {
        None
    }
}

fn edge_score(text: &str, polarity: Option<&str>) -> i32 {
    match polarity {
        None => 0,
        Some(pol) => {
            let text_lower = text.to_lowercase();
            let pos_hits: i32 = POSITIVE_HINTS.iter()
                .filter(|hint| text_lower.contains(*hint))
                .count() as i32;
            let neg_hits: i32 = NEGATIVE_HINTS.iter()
                .filter(|hint| text_lower.contains(*hint))
                .count() as i32;
            if pol == "positive" {
                pos_hits - neg_hits
            } else {
                neg_hits - pos_hits
            }
        }
    }
}

fn choose_edge(
    edges: &[Edge],
    node_labels: &HashMap<String, String>,
    candidates: &[usize],
    polarity: Option<&str>,
) -> Option<usize> {
    if candidates.is_empty() {
        return None;
    }
    if candidates.len() == 1 {
        return Some(candidates[0]);
    }
    if polarity.is_none() {
        let unlabeled: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|&idx| edges[idx].label.is_none())
            .collect();
        return if unlabeled.len() == 1 { Some(unlabeled[0]) } else { None };
    }

    let scored: Vec<(i32, usize)> = candidates
        .iter()
        .map(|&idx| {
            let target_label = node_labels
                .get(&edges[idx].dst)
                .cloned()
                .unwrap_or_else(|| edges[idx].dst.clone());
            (edge_score(&target_label, polarity), idx)
        })
        .collect();
    
    let max_score = scored.iter().map(|(s, _)| *s).max().unwrap_or(0);
    if max_score <= 0 {
        let unlabeled: Vec<usize> = candidates
            .iter()
            .copied()
            .filter(|&idx| edges[idx].label.is_none())
            .collect();
        return if unlabeled.len() == 1 { Some(unlabeled[0]) } else { None };
    }
    
    let winners: Vec<usize> = scored
        .iter()
        .filter(|(s, _)| *s == max_score)
        .map(|(_, idx)| *idx)
        .collect();
    if winners.len() == 1 { Some(winners[0]) } else { None }
}

fn move_branch_labels(lines: &mut Vec<String>) {
    let edge_re = Regex::new(
        r#"^(?P<indent>\s*)(?P<src>[A-Za-z0-9_]+)\s*(?P<arrow>[-.=]+>)\s*(?:\|"(?P<label>[^"]*)"\|\s*)?(?P<dst>[A-Za-z0-9_]+)\s*$"#
    ).unwrap();
    let node_label_re = Regex::new(r#"\["(.*?)"\]"#).unwrap();
    
    let mut edges: Vec<Edge> = Vec::new();
    let mut node_labels: HashMap<String, String> = HashMap::new();
    
    for (idx, line) in lines.iter().enumerate() {
        if let Some(label_match) = node_label_re.captures(line) {
            let label = label_match.get(1).unwrap().as_str();
            let prefix = line.split("[\"").next().unwrap_or("").trim();
            if !prefix.is_empty() {
                node_labels.insert(prefix.to_string(), label.to_string());
            }
        }
        
        if let Some(caps) = edge_re.captures(line) {
            edges.push(Edge {
                line_idx: idx,
                indent: caps.name("indent").map_or("", |m| m.as_str()).to_string(),
                src: caps.name("src").unwrap().as_str().to_string(),
                arrow: caps.name("arrow").unwrap().as_str().to_string(),
                label: caps.name("label").map(|m| m.as_str().to_string()),
                dst: caps.name("dst").unwrap().as_str().to_string(),
            });
        }
    }
    
    let mut outgoing: HashMap<String, Vec<usize>> = HashMap::new();
    for (edge_idx, edge) in edges.iter().enumerate() {
        outgoing.entry(edge.src.clone()).or_default().push(edge_idx);
    }
    
    let mut moved_targets: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut label_moves: Vec<(usize, usize)> = Vec::new(); // (from_edge_idx, to_edge_idx)
    
    for edge_idx in 0..edges.len() {
        if moved_targets.contains(&edge_idx) {
            continue;
        }
        if !is_branch_label(&edges[edge_idx].label) {
            continue;
        }
        if outgoing.get(&edges[edge_idx].src).map_or(0, |v| v.len()) != 1 {
            continue;
        }
        
        let candidates_all = outgoing.get(&edges[edge_idx].dst).cloned().unwrap_or_default();
        if candidates_all.len() < 2 {
            continue;
        }
        
        let candidates: Vec<usize> = candidates_all
            .into_iter()
            .filter(|&idx| edges[idx].label.is_none())
            .collect();
        if candidates.is_empty() {
            continue;
        }
        
        let polarity = edges[edge_idx].label.as_ref().and_then(|l| label_polarity(l));
        if let Some(target_idx) = choose_edge(&edges, &node_labels, &candidates, polarity) {
            label_moves.push((edge_idx, target_idx));
            moved_targets.insert(target_idx);
        }
    }
    
    // Apply the label moves
    for (from_idx, to_idx) in label_moves {
        let label = edges[from_idx].label.take();
        edges[to_idx].label = label;
    }
    
    // Update lines
    for edge in &edges {
        let new_line = if let Some(ref label) = edge.label {
            format!("{}{} {}|\"{}\"| {}", edge.indent, edge.src, edge.arrow, label, edge.dst)
        } else {
            format!("{}{} {} {}", edge.indent, edge.src, edge.arrow, edge.dst)
        };
        lines[edge.line_idx] = new_line;
    }
}

fn sanitize_mermaid_block(lines: &[String]) -> Vec<String> {
    let mut lines = sanitize_node_labels(lines);
    
    let mut block_type = None;
    for line in &lines {
        let stripped = line.trim();
        if stripped.is_empty() {
            continue;
        }
        if stripped.starts_with("flowchart") || stripped.starts_with("graph") {
            block_type = Some("flowchart");
        }
        break;
    }
    
    if block_type == Some("flowchart") {
        move_branch_labels(&mut lines);
    }
    
    lines
}

fn sanitize_mermaid(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut in_block = false;
    let mut block_lines: Vec<String> = Vec::new();
    
    for line in lines {
        if line.starts_with("```") && line.contains("mermaid") {
            in_block = true;
            out.push(line.to_string());
            block_lines.clear();
            continue;
        }
        if in_block {
            if line.starts_with("```") {
                out.extend(sanitize_mermaid_block(&block_lines));
                out.push(line.to_string());
                in_block = false;
            } else {
                block_lines.push(line.to_string());
            }
            continue;
        }
        out.push(line.to_string());
    }
    
    if in_block {
        out.extend(sanitize_mermaid_block(&block_lines));
    }
    
    let mut result = out.join("\n");
    if text.ends_with('\n') {
        result.push('\n');
    }
    result
}

fn process_text(text: &str) -> String {
    let text = strip_preamble(text);
    let text = remove_link_copied(&text);
    let text = remove_ask_devin_lines(&text);
    let text = fix_internal_links(&text);
    let text = fix_section_links(&text);
    let text = strip_github_blob_sha(&text);
    let text = sanitize_mermaid(&text);
    text
}

fn parse_readme_index(readme_path: &Path) -> Vec<String> {
    let text = match fs::read_to_string(readme_path) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    
    // Fix broken links by removing newlines within markdown links
    let broken_link_re = Regex::new(r"\]\([^)]*\n[^)]*\)").unwrap();
    let text = broken_link_re.replace_all(&text, |caps: &regex::Captures| {
        caps[0].replace('\n', "")
    });
    
    let line_re = Regex::new(r"^\s*-\s+\[[^\]]+\]\(([^)]+)\)\s*$").unwrap();
    let mut items = Vec::new();
    
    for line in text.lines() {
        if let Some(caps) = line_re.captures(line) {
            let target = caps.get(1).unwrap().as_str();
            if target.starts_with("http://") || target.starts_with("https://") {
                continue;
            }
            if !target.to_lowercase().ends_with(".md") {
                continue;
            }
            items.push(target.to_string());
        }
    }
    
    items
}

fn build_ordinal_mapping(readme_path: &Path) -> HashMap<String, String> {
    let items = parse_readme_index(readme_path);
    let numbered_re = Regex::new(r"^\d{2}-").unwrap();
    let mut mapping = HashMap::new();
    
    for (idx, target) in items.iter().enumerate() {
        let target_path = Path::new(target);
        let filename = target_path.file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("");
        
        // Skip if already numbered
        if numbered_re.is_match(filename) {
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
        
        mapping.insert(target.clone(), new_path);
    }
    
    mapping
}

fn rewrite_markdown_links(text: &str, mapping: &HashMap<String, String>) -> String {
    let link_re = Regex::new(r"\]\(([^)]+)\)").unwrap();
    
    link_re.replace_all(text, |caps: &regex::Captures| {
        let target = caps.get(1).unwrap().as_str();
        
        if target.starts_with("http://") || target.starts_with("https://") {
            return caps[0].to_string();
        }
        
        let (target_part, anchor) = if let Some(pos) = target.find('#') {
            (&target[..pos], &target[pos..])
        } else {
            (target, "")
        };
        
        let mut prefix = String::new();
        let mut remaining = target_part.to_string();
        
        while remaining.starts_with("../") {
            prefix.push_str("../");
            remaining = remaining[3..].to_string();
        }
        if remaining.starts_with("./") {
            prefix.push_str("./");
            remaining = remaining[2..].to_string();
        }
        
        if let Some(new_target) = mapping.get(&remaining) {
            format!("]({}{}{})", prefix, new_target, anchor)
        } else {
            caps[0].to_string()
        }
    }).to_string()
}

fn apply_readme_ordinal(output_dir: &Path, dry_run: bool) -> Vec<PathBuf> {
    let readme_path = output_dir.join("README.md");
    if !readme_path.exists() {
        return Vec::new();
    }
    
    // Clean up the README first to fix broken links
    if let Ok(original_readme) = fs::read_to_string(&readme_path) {
        let cleaned_readme = process_text(&original_readme);
        if cleaned_readme != original_readme && !dry_run {
            let _ = fs::write(&readme_path, &cleaned_readme);
        }
    }
    
    let mapping = build_ordinal_mapping(&readme_path);
    if mapping.is_empty() {
        return Vec::new();
    }
    
    let mut changed: Vec<PathBuf> = Vec::new();
    
    // Update links in all markdown files
    for entry in WalkDir::new(output_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.extension().map_or(false, |e| e == "md") {
            if let Ok(original) = fs::read_to_string(path) {
                let updated = rewrite_markdown_links(&original, &mapping);
                if updated != original {
                    changed.push(path.to_path_buf());
                    if !dry_run {
                        let _ = fs::write(path, &updated);
                    }
                }
            }
        }
    }
    
    // Rename files
    for (old, new) in &mapping {
        let old_path = output_dir.join(old);
        let new_path = output_dir.join(new);
        
        if old_path == new_path {
            continue;
        }
        if old_path.exists() {
            changed.push(new_path.clone());
            if !dry_run {
                if let Some(parent) = new_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::rename(&old_path, &new_path);
            }
        }
    }
    
    changed
}

fn process_directory(input_dir: &Path, output_dir: &Path, dry_run: bool) -> Vec<PathBuf> {
    let mut changed_files: Vec<PathBuf> = Vec::new();
    
    for entry in WalkDir::new(input_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            continue;
        }
        
        let rel_path = path.strip_prefix(input_dir).unwrap_or(path);
        if rel_path.file_name()
            .and_then(|f| f.to_str())
            .map_or(false, |f| f.starts_with('.'))
        {
            continue;
        }
        
        let out_path = output_dir.join(rel_path);
        
        if path.extension().map_or(true, |e| e != "md") {
            // Copy non-markdown files
            if !dry_run {
                if let Some(parent) = out_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::copy(path, &out_path);
            }
            continue;
        }
        
        // Process markdown files
        if let Ok(original) = fs::read_to_string(path) {
            let updated = process_text(&original);
            if updated != original {
                changed_files.push(out_path.clone());
            }
            if !dry_run {
                if let Some(parent) = out_path.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&out_path, &updated);
            }
        }
    }
    
    // Apply ordinal renaming to each subdirectory that has a README.md
    for entry in WalkDir::new(output_dir).into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && path.file_name().map_or(false, |f| f == "README.md") {
            if let Some(parent) = path.parent() {
                changed_files.extend(apply_readme_ordinal(parent, dry_run));
            }
        }
    }
    
    changed_files
}

fn main() {
    let args = Args::parse();
    
    let output_dir = if args.in_place {
        args.input_dir.clone()
    } else {
        match args.output_dir {
            Some(dir) => dir,
            None => {
                eprintln!("Error: output_dir is required unless --in-place is set");
                std::process::exit(1);
            }
        }
    };
    
    let changed = process_directory(&args.input_dir, &output_dir, args.dry_run);
    
    if args.dry_run {
        for path in changed {
            println!("{}", path.display());
        }
    }
}
